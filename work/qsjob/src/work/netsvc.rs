// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the small server jobs (`QTOTNTP`, `QTLPD00001`) and the mail
//! commands share: their attributes from the server table, host names
//! through the TCP/IP host table, job log and `QSYSOPR` messages, and
//! noticing that the job is to end.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ipc::chan::Chan;
use stg::tcp::{TcpOp, TcpReply};
use stg::{Client, Shared};
use wm::ctl::ToJob;
use wm::obj::msgq::{MsgKind, QMsg};

/// A server's attribute bag from the `QUSRSYS` server table.
pub fn bag(link: &Shared, server: &str) -> Vec<(String, String)> {
    match link.borrow_mut().with(|l| l.tcp(TcpOp::Servers)) {
        TcpReply::Servers { list } => list
            .into_iter()
            .find(|s| s.server.eq_ignore_ascii_case(server))
            .map(|s| s.bag)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The addresses of `host`: an address as it is, then the TCP/IP host
/// table, then the system's resolver.
pub fn resolve(link: &Shared, host: &str, port: u16) -> Vec<SocketAddr> {
    let h = host.trim().trim_matches(['[', ']']);
    if let Ok(ip) = h.parse::<IpAddr>() {
        return vec![SocketAddr::new(ip, port)];
    }
    if let TcpReply::HostConfig { cfg } = link.borrow_mut().with(|l| l.tcp(TcpOp::HostConfig)) {
        let found = cfg.lookup(h);
        if !found.is_empty() {
            return found.into_iter().map(|ip| SocketAddr::new(ip, port)).collect();
        }
    }
    (h, port).to_socket_addrs().map(Iterator::collect).unwrap_or_default()
}

/// Now, microseconds since the epoch.
pub fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
}

/// An informational message in the job's log.
pub fn joblog(link: &Shared, program: &str, msgid: &str, text: &str) {
    let e = wm::joblog::Entry {
        msgid: msgid.to_string(),
        kind: "Information".into(),
        time: now_micros(),
        from: (program.to_string(), "QSYS".into()),
        from_inst: "*N".into(),
        to: ("*EXT".into(), String::new()),
        to_inst: "*N".into(),
        text: text.to_string(),
        external: true,
        ..wm::joblog::Entry::default()
    };
    link.borrow_mut().with(|l| l.joblog_put(vec![e]));
}

/// A message to `QSYSOPR` from `QTCPMSG`.
pub fn sysopr(link: &Shared, msgid: &str, severity: u8, text: &str) {
    let m = QMsg {
        kind: MsgKind::Info,
        severity,
        msgid: msgid.to_string(),
        msgf: ("QTCPMSG".into(), "QSYS".into()),
        text: text.to_string(),
        ..QMsg::default()
    };
    if let Err(e) = link.borrow_mut().with(|l| l.msgq_send("*SYSOPR".into(), m)) {
        eprintln!("qsjob: QSYSOPR: {e}");
    }
}

/// Notices the end of a server job: its parent's `End`, `ENDJOB`, or
/// its subsystem ending.
pub struct Ender {
    rx: mpsc::Receiver<bool>,
    me: u32,
}

impl Ender {
    /// Watch the control channel of job `me`.
    pub fn new(ctl: &Chan, me: u32) -> Self {
        let mut reader = ctl.try_clone().expect("ctl");
        let (tx, rx) = mpsc::channel::<bool>();
        std::thread::spawn(move || {
            while let Ok(m) = reader.recv::<ToJob>() {
                if let ToJob::End { immediate } = m {
                    let _ = tx.send(immediate);
                    return;
                }
            }
        });
        Self { rx, me }
    }

    /// Whether the job is to end, waiting up to `wait` for its parent.
    pub fn ended(&self, link: &Shared, wait: Duration) -> bool {
        if self.rx.recv_timeout(wait).is_ok() {
            return true;
        }
        let state = link.borrow_mut().with(Client::sbs_state);
        state.ending.is_some() || state.end_jobs.iter().any(|(n, _)| *n == self.me)
    }
}
