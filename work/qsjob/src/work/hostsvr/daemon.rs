// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A host server daemon job (`QZRCSRVSD`, ...): it listens on its
//! server's plain and TLS ports and gives each connection to the
//! server's prestart jobs through the descriptor relay; the port
//! mapper's daemon (`QZSOSMAPD`) answers its lookups itself.

use std::net::{TcpListener, TcpStream};
use std::os::fd::OwnedFd;
use std::time::Duration;

use hostserver::jobs::{self, Daemon};
use ipc::chan::Chan;
use sockets::relay;
use stg::Shared;
use wm::ctl::{Start, ToJob};
use wm::job::Status;

/// The job's internal identifier: its number and name, EBCDIC.
pub fn internal_id(start: &Start) -> sockets::JobId {
    let text = format!("{:0>6}{:<10}", start.number, start.name);
    let mut id = [0x40u8; 16];
    for (i, ch) in text.chars().take(16).enumerate() {
        id[i] = ccsid::byte_of(ch).unwrap_or(0x40);
    }
    id
}

fn bind(port: u16) -> Option<TcpListener> {
    match TcpListener::bind(("0.0.0.0", port)) {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!("qsjob: host server port {port}: {e}");
            None
        }
    }
}

/// Give accepted connections to the prestart pool.
fn hand_over(l: &TcpListener, d: &'static Daemon, tls: bool, me: sockets::JobId) {
    let program = if tls { d.prestart_tls } else { d.prestart };
    let pool = relay::pool(program);
    for s in l.incoming() {
        let Ok(s) = s else { continue };
        let _ = s.set_nodelay(true);
        let fd = sockets::Fd::from(OwnedFd::from(s));
        let note: &[u8] = if tls { b"T" } else { b"" };
        if let Err(e) = relay::give(relay::SOCKET, &fd, &pool, &me, note) {
            eprintln!("qsjob: {}: handing a connection to {program}: {e:?}", d.name);
        }
    }
}

/// Answer port mapper lookups.
fn mapper(l: &TcpListener) {
    for s in l.incoming() {
        let Ok(mut s) = s else { continue };
        std::thread::spawn(move || {
            let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
            let _ = hostserver::io::serve_svrmap::<TcpStream>(&mut s);
        });
    }
}

/// Run the daemon. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let mut ctl = ctl;
    let Some(d) = jobs::daemon(start.rqsdta.trim()) else {
        crate::start::end(&mut ctl, &format!("no host server {}", start.rqsdta.trim()));
    };
    let me = internal_id(start);
    let Some(plain) = bind(d.port) else {
        crate::start::end(&mut ctl, &format!("port {} is in use", d.port));
    };
    if d.server.is_none() {
        std::thread::spawn(move || mapper(&plain));
    } else {
        std::thread::spawn(move || hand_over(&plain, d, false, me));
        if let Some(l) = d.tls_port.and_then(bind) {
            std::thread::spawn(move || hand_over(&l, d, true, me));
        }
    }
    stg::remote::status(link, Status::Selw, "");
    // The parent (the job that ran STRHOSTSVR) may end long before the
    // daemon: only an explicit End from it counts; ENDJOB, ENDHOSTSVR
    // and ENDSBS reach the daemon through its subsystem's state.
    let mut reader = ctl.try_clone().ok();
    let (end_tx, end_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        while let Some(Ok(m)) = reader.as_mut().map(ipc::chan::Chan::recv::<ToJob>) {
            if matches!(m, ToJob::End { .. }) {
                let _ = end_tx.send(());
                return;
            }
        }
    });
    loop {
        std::thread::sleep(Duration::from_millis(500));
        if end_rx.try_recv().is_ok() {
            std::process::exit(0);
        }
        let state = link.borrow_mut().with(stg::Client::sbs_state);
        if state.ending.is_some() || state.end_jobs.iter().any(|(n, _)| *n == start.number) {
            std::process::exit(0);
        }
    }
}
