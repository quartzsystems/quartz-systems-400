// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QZLSFILE`, a prestart job of `QSERVER`: it takes a connection
//! `QZLSSERVER` gives its pool, serves it with `netserver::NetServer`
//! built from the configuration, runs as the profile that signs on,
//! reports its sessions into the service's table, takes back the
//! reconnects `QZLSSERVER` routes to it, and once no connection is
//! served and no durable open kept, runs as `QUSER` again and waits for
//! the next client, until `MAXUSE` clients were served.

use std::cell::RefCell;
use std::net::{IpAddr, TcpStream};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ipc::chan::Chan;
use netserver::NetServer;
use netserver::print::Printer;
use netserver::signon::SignOn;
use sockets::relay;
use stg::netsvr::NsOp;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start};
use wm::job::Status;

use super::printer::JobPrinter;
use super::report::{Admit, Table};
use super::signon::JobSignOn;
use super::svc::{Svc, thread_link};
use super::{FILE_PROGRAM, config, ending, server_config};
use crate::console::Console;
use crate::work::hostsvr::job::Job;

/// `MAXUSE(n)` of the request data the monitor passes; 0 for none.
fn maxuse(rqsdta: &str) -> u32 {
    rqsdta
        .split_once("MAXUSE(")
        .and_then(|(_, r)| r.split(')').next())
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

/// The relay note `QZLSSERVER` gives: `N` and the client's NetBIOS
/// name after a session request on 139.
fn workstation(note: &[u8]) -> Option<String> {
    note.strip_prefix(b"N").map(|n| String::from_utf8_lossy(n).trim().to_string())
}

/// Serve `stream` on a thread of its own.
fn spawn_serve(ns: &Arc<NetServer>, stream: TcpStream) {
    let ns = ns.clone();
    std::thread::spawn(move || {
        let _ = stream.set_nodelay(true);
        if let Err(e) = ns.serve_one(stream) {
            eprintln!("qsjob: QZLSFILE: connection: {e}");
        }
    });
}

/// One client, from its first connection until the job is released.
#[allow(clippy::too_many_lines)] // one client's life, start to end
fn serve_client(link: &Shared, start: &Start, me: sockets::JobId, stream: TcpStream, note: &[u8]) {
    let c = match config(link) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("qsjob: QZLSFILE: configuration: {e}");
            return;
        }
    };
    let svc = match Svc::start(start.number) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("qsjob: QZLSFILE: {e}");
            return;
        }
    };
    let mut cfg = server_config(&c);
    let signon = Arc::new(JobSignOn::new(svc.clone(), c.attrs.current.log_signon));
    let printer: Arc<dyn Printer> = Arc::new(JobPrinter::new(svc.clone(), &c));
    let table = Arc::new(Table::new(svc.clone()));
    if let Some(ws) = workstation(note) {
        table.set_workstation(&ws);
    }
    let (number, pid) = (start.number, std::process::id());
    let built = loop {
        let make: netserver::ifs::MakeClient = Box::new(move || match thread_link(number, pid) {
            Ok(l) => Box::new(stg::remote::RemoteIfs::new(l)),
            Err(e) => {
                eprintln!("qsjob: QZLSFILE: the IFS thread's link: {e}");
                Box::new(ifs::client::Unavailable(ifs::Errno::EIO))
            }
        });
        let so: Arc<dyn SignOn> = signon.clone();
        match NetServer::build_admitted(&cfg, make, so, &printer, Some(Arc::new(Admit(table.clone())))) {
            Ok(ns) => break Some(ns),
            // A share that cannot be served is left out, the rest served.
            Err(netserver::BuildError::Share { name, error }) => {
                eprintln!("qsjob: QZLSFILE: share {name} not served: {error}");
                cfg.file_shares.retain(|s| s.name != name);
            }
            Err(e) => {
                eprintln!("qsjob: QZLSFILE: {e}");
                break None;
            }
        }
    };
    let Some(ns) = built else {
        return;
    };
    // Leases broken by other jobs' opens: the IFS lease table's orders on
    // a link of their own.
    let orders: netserver::ifs::MakeClient = Box::new(move || match thread_link(number, pid) {
        Ok(l) => Box::new(stg::remote::RemoteIfs::new(l)),
        Err(e) => {
            eprintln!("qsjob: QZLSFILE: the lease thread's link: {e}");
            Box::new(ifs::client::Unavailable(ifs::Errno::EIO))
        }
    });
    if let Err(e) = ns.start_leases(orders) {
        eprintln!("qsjob: QZLSFILE: leases across jobs not started: {e}");
    }
    // The client's other connections come here from the start, not only
    // once the first report runs: change notify, share modes and leases
    // then see all of them in one job.
    if let Some(g) = netserver::route::client_guid(&stream, Duration::from_secs(2)) {
        link.borrow_mut().with(|l| {
            l.netsvr(NsOp::Route {
                relay: me.to_vec(),
                guids: vec![g.to_vec()],
                peers: Vec::new(),
            })
        });
    }
    let ns = Arc::new(ns);
    let peers: Mutex<Vec<IpAddr>> = Mutex::default();
    let mut shut = false;
    let mut next = Some(stream);
    // A connection routed here while the job was being released waits on
    // its relay identifier: it is served by the same server, and the
    // client stays here.
    while let Some(stream) = next.take() {
        let released = Arc::new(AtomicBool::new(false));
        {
            let (ns, released) = (ns.clone(), released.clone());
            std::thread::spawn(move || {
                let _ = stream.set_nodelay(true);
                if let Err(e) = ns.serve_one(stream) {
                    eprintln!("qsjob: QZLSFILE: connection: {e}");
                }
                ns.idle_until_released();
                released.store(true, Ordering::Release);
            });
        }
        // Reconnects QZLSSERVER routes here.
        let rerouted = {
            let (ns, released, table) = (ns.clone(), released.clone(), table.clone());
            std::thread::spawn(move || {
                while !released.load(Ordering::Acquire) {
                    match relay::take(relay::SOCKET, &me, None, 500) {
                        Ok(Some(t)) => {
                            if let Some(ws) = workstation(&t.note) {
                                table.set_workstation(&ws);
                            }
                            spawn_serve(&ns, TcpStream::from(t.fd.into_owned()));
                        }
                        Ok(None) => {}
                        Err(_) => std::thread::sleep(Duration::from_millis(500)),
                    }
                }
            })
        };
        let mut last = std::time::Instant::now();
        while !released.load(Ordering::Acquire) {
            // Released is seen within a tenth of a second, so the routes go
            // at once; the report runs once a second.
            std::thread::sleep(Duration::from_millis(100));
            if last.elapsed() < Duration::from_secs(1) {
                continue;
            }
            last = std::time::Instant::now();
            let now = ns.sessions();
            {
                let mut p = peers.lock().unwrap_or_else(PoisonError::into_inner);
                for s in &now {
                    if let Some(a) = s.peer.map(|a| a.ip())
                        && !p.contains(&a)
                    {
                        p.push(a);
                    }
                }
            }
            for sid in table.sync(&now) {
                let _ = ns.end_session(sid);
                table.ended(sid);
            }
            if !shut && ending(link) {
                // ENDTCPSVR, QZLSENDS: every connection closes after the
                // request it is answering; no durable open is kept for later.
                ns.shutdown();
                shut = true;
            }
            let guids: Vec<Vec<u8>> = ns.client_guids().into_iter().map(|g| g.to_vec()).collect();
            let peers: Vec<String> = if ns.disconnected_opens() > 0 {
                peers.lock().unwrap_or_else(PoisonError::into_inner).iter().map(ToString::to_string).collect()
            } else {
                Vec::new()
            };
            link.borrow_mut().with(|l| {
                l.netsvr(NsOp::Route {
                    relay: me.to_vec(),
                    guids,
                    peers,
                })
            });
        }
        // Withdrawn first, so nothing more is routed here.
        link.borrow_mut().with(|l| {
            l.netsvr(NsOp::Route {
                relay: me.to_vec(),
                guids: Vec::new(),
                peers: Vec::new(),
            })
        });
        let _ = rerouted.join();
        next = match relay::take(relay::SOCKET, &me, None, 300) {
            Ok(Some(t)) => Some(TcpStream::from(t.fd.into_owned())),
            _ => None,
        };
    }
    table.clear();
    drop(ns);
}

/// Run the prestart job. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let ending_chan = ctl.try_clone().ok();
    let job: Job = Rc::new(RefCell::new(Console::new(ctl, link, start)));
    job.borrow_mut().started();
    let pool = relay::pool(FILE_PROGRAM);
    let me = crate::work::hostsvr::daemon::internal_id(start);
    let limit = maxuse(&start.rqsdta);
    let mut uses = 0;
    loop {
        stg::remote::status(link, Status::Evtw, "PSRW");
        if !job.borrow_mut().idle(Duration::ZERO) {
            break;
        }
        let taken = match relay::take_as(relay::SOCKET, &pool, Some(&me), None, 1000) {
            Ok(Some(t)) => t,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("qsjob: {}: descriptor relay: {e:?}", start.name);
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        stg::remote::status(link, Status::Run, "");
        let stream = TcpStream::from(taken.fd.into_owned());
        serve_client(link, start, me, stream, &taken.note);
        link.borrow_mut().with(Client::host_reset);
        uses += 1;
        if limit > 0 && uses >= limit {
            break;
        }
    }
    link.borrow_mut().with(|l| l.netsvr(NsOp::JobEnded));
    job.borrow_mut().ended(0);
    if let Some(mut c) = ending_chan {
        let _ = c.send(&FromJob::Ending {
            reason: "prestart job ended".into(),
        });
    }
    std::process::exit(0)
}
