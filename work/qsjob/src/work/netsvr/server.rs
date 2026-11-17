// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QZLSSERVER`, NetServer's listener: a batch job of `QSERVER` that the
//! storage service submits with routing data `QZLSSTART` (`QZLSSTRS`,
//! `STRTCPSVR SERVER(*NETSVR)`). It listens on 445, and on 139 when
//! NetBIOS is allowed (answering the session request there first, and
//! holding the server's NetBIOS names on UDP 137, `nbns`, and announcing
//! it to the workgroup's master browser on UDP 138, `nbdgm`), and
//! gives each connection to a `QZLSFILE` prestart job through the
//! descriptor relay: the job that registered the connection's
//! `ClientGuid` (or, without one, its address) when there is one, so a
//! reconnecting client finds its durable opens; a new prestart job
//! otherwise.

use std::net::{TcpListener, TcpStream};
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use netserver::{nbdgm, nbns, nbss, route};
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
/// How long the accept loop waits after an accept that failed.
const RETRY: Duration = Duration::from_millis(50);
/// How long [`wake`] waits for its connection.
const WAKE: Duration = Duration::from_secs(2);
/// `CPIB683` reason 6: an internal operation failed (the return code
/// is the error number).
const CANNOT_LISTEN: &str = "6";
/// `CPIB683` reason 7: the shares could not be offered.
const NO_SHARES: &str = "7";

/// A connection read as far as routing needs: the relay note, the
/// `ClientGuid` of its first NEGOTIATE (empty without one) and the peer.
struct Arrived {
    stream: TcpStream,
    note: Vec<u8>,
    guid: Vec<u8>,
    peer: String,
}

/// Reads what routing needs from a new connection: the NetBIOS session
/// request on 139 (`netbios` names the server it must call), then the
/// first NEGOTIATE, peeked. `None` when 139's session request fails.
fn arrive(mut s: TcpStream, netbios: Option<&str>) -> Option<Arrived> {
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
                return None;
            }
        }
    }
    let guid = route::client_guid(&s, PEEK)
        .map(|g| g.to_vec())
        .unwrap_or_default();
    let peer = s
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();
    Some(Arrived {
        stream: s,
        note,
        guid,
        peer,
    })
}

/// Where one connection goes.
fn route_to(link: &Shared, a: &Arrived) -> sockets::JobId {
    let (guid, peer) = (a.guid.clone(), a.peer.clone());
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
/// request on 139 must call. The accept blocks, so a connection is taken
/// the moment it arrives; [`wake`] ends it once `stop` is set. Each
/// connection is read on a thread of its own (a client slow to send its
/// session request or NEGOTIATE holds up no other) and handed to the
/// caller's thread through `to`, which routes it.
fn accept(l: &TcpListener, netbios: Option<&str>, stop: &AtomicBool, to: &mpsc::Sender<Arrived>) {
    let _ = l.set_nonblocking(false);
    while !stop.load(Ordering::Acquire) {
        let s = match l.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                eprintln!("qsjob: {SERVER_PROGRAM}: accept: {e}");
                std::thread::sleep(RETRY);
                continue;
            }
        };
        if stop.load(Ordering::Acquire) {
            // The wake-up, or a client too late.
            break;
        }
        let (to, nb) = (to.clone(), netbios.map(str::to_owned));
        std::thread::spawn(move || {
            if let Some(a) = arrive(s, nb.as_deref()) {
                let _ = to.send(a);
            }
        });
    }
}

/// Routes and hands over each connection `from` brings, until its
/// senders are gone.
fn hand_over(link: &Shared, me: sockets::JobId, from: &mpsc::Receiver<Arrived>) {
    while let Ok(a) = from.recv() {
        let target = route_to(link, &a);
        let fd = sockets::Fd::from(OwnedFd::from(a.stream));
        if let Err(e) = relay::give(relay::SOCKET, &fd, &target, &me, &a.note) {
            eprintln!("qsjob: {SERVER_PROGRAM}: handing a connection to {FILE_PROGRAM}: {e:?}");
        }
    }
}

/// Wakes the accept loop blocked on `port` with a connection of its own
/// (after `stop` is set); whether it connected.
fn wake(port: u16) -> bool {
    let to = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port));
    TcpStream::connect_timeout(&to, WAKE).is_ok()
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
        let port = l.local_addr().map_or(0, |a| a.port());
        let stop = stop.clone();
        let pid = std::process::id();
        let thread = std::thread::spawn(move || {
            // The relay and the routing table are asked from this
            // thread's own link.
            match super::svc::thread_link(number, pid) {
                Ok(link) => {
                    let (tx, rx) = mpsc::channel();
                    let acc = std::thread::spawn(move || accept(&l, nb.as_deref(), &stop, &tx));
                    hand_over(&link, me, &rx);
                    let _ = acc.join();
                }
                Err(e) => eprintln!("qsjob: {SERVER_PROGRAM}: a thread's link: {e}"),
            }
        });
        threads.push((Some(port), thread));
    }
    threads.extend(
        udp_services(&cfg, &server, &stop)
            .into_iter()
            .map(|t| (None, t)),
    );
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
    for (port, t) in threads {
        // Each loop blocks in accept: a connection of our own wakes it.
        // Not woken, the thread ends at its next connection; the job
        // ending closes the listener.
        if t.is_finished() || port.is_none_or(wake) {
            let _ = t.join();
        }
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

/// With NetBIOS allowed, the name service on UDP 137 (`nbns`) and the
/// host announcements on UDP 138 (`nbdgm`), each on a thread that looks
/// at `stop` between datagrams by itself. A port that cannot be bound is
/// logged, and the rest served without it.
fn udp_services(
    cfg: &netsvrcfg::Config,
    server: &str,
    stop: &Arc<AtomicBool>,
) -> Vec<std::thread::JoinHandle<()>> {
    let a = &cfg.attrs.current;
    if !a.allow_netbios {
        return Vec::new();
    }
    let mut threads = Vec::new();
    match nbns::Udp::bind(nbns::udp::PORT) {
        Ok(mut udp) => {
            let stop = stop.clone();
            let nb = nbns::NameConfig::new(server, &a.domain);
            threads.push(std::thread::spawn(move || {
                nbns::serve(&mut udp, &mut nbns::NameService::new(&nb), &stop);
            }));
        }
        Err(e) => {
            eprintln!("qsjob: {SERVER_PROGRAM}: UDP port 137: {e}");
            return threads;
        }
    }
    match nbns::Udp::bind(nbdgm::service::PORT) {
        Ok(mut udp) => {
            let stop = stop.clone();
            let announce = nbdgm::AnnounceConfig {
                server: server.to_owned(),
                workgroup: a.domain.clone(),
                comment: a.text.clone(),
                printing: cfg.shares.values().any(|s| s.as_print().is_some()),
                interval: Duration::from_millis(u64::try_from(a.browse_interval).unwrap_or(0)),
            };
            threads.push(std::thread::spawn(move || {
                nbdgm::serve(&mut udp, &mut nbdgm::Announcer::new(announce), &stop);
            }));
        }
        Err(e) => eprintln!("qsjob: {SERVER_PROGRAM}: UDP port 138: {e}"),
    }
    threads
}
