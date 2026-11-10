// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QZLSSERVER`, NetServer's listener: a batch job of `QSERVER` that the
//! storage service submits with routing data `QZLSSTART` (`QZLSSTRS`,
//! `STRTCPSVR SERVER(*NETSVR)`). It listens on 445, and on 139 when
//! NetBIOS is allowed (answering the session request there first), and
//! gives each connection to a `QZLSFILE` prestart job through the
//! descriptor relay: the job that registered the connection's
//! `ClientGuid` (or, without one, its address) when there is one, so a
//! reconnecting client finds its durable opens; a new prestart job
//! otherwise.

use std::net::{TcpListener, TcpStream};
use std::os::fd::OwnedFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use netserver::{nbss, route};
use sockets::relay;
use stg::netsvr::{NsOp, NsReply};
use stg::{Client, Shared};
use wm::job::Status;

use super::{FILE_PROGRAM, SERVER_PROGRAM, config, ending, sysopr};
use crate::console::Console;

/// How long the first NEGOTIATE is waited for when routing.
const PEEK: Duration = Duration::from_secs(2);
/// How long the NetBIOS session request is waited for on 139.
const SESSION_REQUEST: Duration = Duration::from_secs(10);
/// `CPIB683` reason 6: an internal operation failed (the return code
/// is the error number).
const CANNOT_LISTEN: &str = "6";
/// `CPIB683` reason 7: the shares could not be offered.
const NO_SHARES: &str = "7";

/// Where one connection goes.
fn route_to(link: &Shared, s: &TcpStream) -> sockets::JobId {
    let guid = route::client_guid(s, PEEK)
        .map(|g| g.to_vec())
        .unwrap_or_default();
    let peer = s
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();
    match link
        .borrow_mut()
        .with(|l| l.netsvr(NsOp::Lookup { guid, peer }))
    {
        NsReply::Target { relay } if relay.len() == 16 => {
            let mut id = [0u8; 16];
            id.copy_from_slice(&relay);
            id
        }
        _ => relay::pool(FILE_PROGRAM),
    }
}

/// Accept on `l` until `stop`; `netbios` names the server a session
/// request on 139 must call.
fn accept(
    l: &TcpListener,
    me: sockets::JobId,
    netbios: Option<&str>,
    stop: &AtomicBool,
    link: &Shared,
) {
    let _ = l.set_nonblocking(true);
    while !stop.load(Ordering::Acquire) {
        let mut s = match l.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(_) => continue,
        };
        let _ = s.set_nonblocking(false);
        let _ = s.set_nodelay(true);
        let mut note = Vec::new();
        if let Some(server) = netbios {
            match nbss::answer(&mut s, server, SESSION_REQUEST) {
                Ok(calling) => {
                    note.push(b'N');
                    note.extend(calling.name.bytes().take(15));
                }
                Err(e) => {
                    eprintln!("qsjob: {SERVER_PROGRAM}: port 139: {e}");
                    continue;
                }
            }
        }
        let target = route_to(link, &s);
        let fd = sockets::Fd::from(OwnedFd::from(s));
        if let Err(e) = relay::give(relay::SOCKET, &fd, &target, &me, &note) {
            eprintln!("qsjob: {SERVER_PROGRAM}: handing a connection to {FILE_PROGRAM}: {e:?}");
        }
    }
}

fn bind(port: u16) -> Result<TcpListener, std::io::Error> {
    TcpListener::bind(("0.0.0.0", port))
}

/// Run the listener; the job's end code.
pub fn run(c: &mut Console, link: &Shared, number: u32, name: &str) -> u16 {
    stg::remote::status(link, Status::Run, "PGM-QZLSSERVER");
    let cfg = match config(link) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("qsjob: {SERVER_PROGRAM}: configuration: {e}");
            sysopr(link, "CPIB683", &[NO_SHARES, "0"]);
            link.borrow_mut().with(|l| l.netsvr(NsOp::JobEnded));
            return 1;
        }
    };
    let server = cfg.attrs.current.server_name.clone();
    let main = match bind(445) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("qsjob: {SERVER_PROGRAM}: port 445: {e}");
            let rc = e.raw_os_error().unwrap_or(0).to_string();
            sysopr(link, "CPIB683", &[CANNOT_LISTEN, &rc]);
            link.borrow_mut().with(|l| l.netsvr(NsOp::JobEnded));
            return 1;
        }
    };
    let netbios = if cfg.attrs.current.allow_netbios {
        match bind(139) {
            Ok(l) => Some(l),
            Err(e) => {
                eprintln!("qsjob: {SERVER_PROGRAM}: port 139: {e}");
                None
            }
        }
    } else {
        None
    };
    let me = relay_id(number, name);
    let stop = Arc::new(AtomicBool::new(false));
    let mut threads = Vec::new();
    for (l, nb) in std::iter::once((main, None)).chain(netbios.map(|l| (l, Some(server.clone())))) {
        let stop = stop.clone();
        let pid = std::process::id();
        threads.push(std::thread::spawn(move || {
            // The relay and the routing table are asked from this
            // thread's own link.
            match super::svc::thread_link(number, pid) {
                Ok(link) => accept(&l, me, nb.as_deref(), &stop, &link),
                Err(e) => eprintln!("qsjob: {SERVER_PROGRAM}: a thread's link: {e}"),
            }
        }));
    }
    link.borrow_mut().with(Client::netsvr_started);
    sysopr(link, "CPIB680", &[&server]);
    stg::remote::status(link, Status::Selw, "");
    while c.idle(Duration::from_millis(500)) {
        if ending(link) {
            break;
        }
        let state = link.borrow_mut().with(Client::sbs_state);
        if state.ending.is_some() || state.end_jobs.iter().any(|(n, _)| *n == number) {
            break;
        }
    }
    stop.store(true, Ordering::Release);
    for t in threads {
        let _ = t.join();
    }
    sysopr(link, "CPIB681", &[&server]);
    link.borrow_mut().with(|l| l.netsvr(NsOp::JobEnded));
    0
}

/// The job's identifier on the descriptor relay (the host servers').
fn relay_id(number: u32, name: &str) -> sockets::JobId {
    let start = wm::ctl::Start {
        number,
        name: name.to_string(),
        ..wm::ctl::Start::default()
    };
    crate::work::hostsvr::daemon::internal_id(&start)
}

/// `QZLSSERVER` started as a server job (role [`super::ROLE`]) rather
/// than routed. Never returns.
pub fn role(ctl: ipc::chan::Chan, link: &Shared, start: &wm::ctl::Start) -> ! {
    let mut ending_chan = ctl.try_clone().ok();
    let mut c = Console::new(ctl, link, start);
    c.started();
    let code = run(&mut c, link, start.number, &start.name);
    c.ended(code);
    if let Some(e) = ending_chan.as_mut() {
        let _ = e.send(&wm::ctl::FromJob::Ending {
            reason: format!("ended, code {code}"),
        });
    }
    std::process::exit(0)
}
