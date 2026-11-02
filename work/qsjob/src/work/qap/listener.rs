// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QAPD`: binds `CHGQAPA`'s port, TLS 1.3 only with the certificate
//! assigned to `QS400_ACCESS_SERVER` (no plain text and no fallback
//! certificate), says so to `QSYSOPR`, and starts a `QAPCNN` job in
//! `QUSRWRK` as `QUSER` for each connection, handing it the socket; the
//! connection job runs TLS. It runs until `ENDTCPSVR`, `ENDJOB` or its
//! subsystem ends it, then tells its connection jobs to end (each sends
//! `ServerEnding`). A start that fails says why and ends the job.

use std::net::{SocketAddr, TcpStream};
use std::os::fd::OwnedFd;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ipc::chan::Chan;
use servers::attrs::qap::QapAttrs;
use servers::listen::{self, Conn, Service};
use servers::msg;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start, ToJob};

use super::super::spawn::{self, Spawned};
use crate::work::netsvc::{self, Ender};

const PGM: &str = "QAPD";
/// The certificate application ID.
pub const APP_ID: &str = "QS400_ACCESS_SERVER";
/// The connection jobs' name and role.
const CONN_JOB: &str = "QAPCNN";
/// How long connection jobs get to say `ServerEnding` and close.
const END_WAIT: Duration = Duration::from_secs(5);

/// The accept loop's service: sockets go to the main thread, which
/// owns the link and starts the jobs.
struct Accept {
    tx: Mutex<Sender<TcpStream>>,
}

impl Service for Accept {
    fn serve(&self, conn: Conn, _peer: SocketAddr) {
        if let Conn::Plain(s) = conn {
            let _ = self.tx.lock().map(|tx| tx.send(s));
        }
    }
}

/// Tell `QSYSOPR` and the job log why the server did not start, and end.
fn refuse(mut ctl: Chan, link: &Shared, why: &str) -> ! {
    let text = format!("QAP server could not be started: {why}.");
    netsvc::sysopr(link, msg::QAP_NOT_STARTED, 40, &text);
    netsvc::joblog(link, PGM, msg::QAP_NOT_STARTED, &text);
    crate::start::end(&mut ctl, why)
}

/// Start a connection job and hand it the socket.
fn connection(link: &Shared, sock: TcpStream) -> Result<Spawned, String> {
    let number = link.borrow_mut().with(Client::job_number)?;
    let s = Start {
        name: CONN_JOB.into(),
        user: "QUSER".into(),
        number,
        handle: u64::from(number),
        kind: 'B',
        subsystem: "QUSRWRK".into(),
        role: CONN_JOB.into(),
        jobd: "QSYS/QSYSJOBD".into(),
        class: "QSYS/QSYSCLS20".into(),
        pool: "*BASE".into(),
        ..Start::default()
    };
    let mut job = spawn::job(s)?;
    spawn::ready(&mut job)?;
    let chan = job.chan.try_clone().map_err(|e| e.to_string())?;
    let via = sockets::Fd::from(chan.into_fd());
    let fd = sockets::Fd::from(OwnedFd::from(sock));
    sockets::sys::send_fd(&via, &fd).map_err(|e| format!("passing the connection: {e:?}"))?;
    Ok(job)
}

/// Tell every connection job to end; kill the ones still there after
/// [`END_WAIT`].
fn end_connections(jobs: &mut Vec<Spawned>) {
    for j in jobs.iter_mut() {
        let _ = j.chan.send(&ToJob::End { immediate: false });
    }
    let until = Instant::now() + END_WAIT;
    while Instant::now() < until {
        jobs.retain_mut(|j| !matches!(j.child.try_wait(), Ok(Some(_))));
        if jobs.is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    for j in jobs.iter_mut() {
        let _ = j.child.kill();
        let _ = j.child.wait();
    }
}

/// Run the listener. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let a = QapAttrs::from_bag(&netsvc::bag(link, "*QAP"));
    // The certificate is checked here so a server without one does not
    // start; each connection job builds its own TLS from it.
    if let Err(e) = crate::work::tls::assigned_tls13(&mut link.borrow_mut(), APP_ID) {
        refuse(ctl, link, &e);
    }
    let (tx, rx): (Sender<TcpStream>, Receiver<TcpStream>) = mpsc::channel();
    let service = Arc::new(Accept { tx: Mutex::new(tx) });
    let running = match listen::bind(a.port).and_then(|l| listen::serve(l, None, service)) {
        Ok(r) => r,
        Err(e) => refuse(ctl, link, &format!("port {}: {e}", a.port)),
    };
    let text = format!("QAP server started on port {}.", a.port);
    netsvc::sysopr(link, msg::QAP_STARTED, 0, &text);
    netsvc::joblog(link, PGM, msg::QAP_STARTED, &text);
    stg::remote::status(link, wm::job::Status::Selw, "");
    let ender = Ender::new(&ctl, start.number);
    let mut jobs: Vec<Spawned> = Vec::new();
    loop {
        while let Ok(sock) = rx.try_recv() {
            match connection(link, sock) {
                Ok(j) => jobs.push(j),
                Err(e) => eprintln!("qsjob: {PGM}: no connection job: {e}"),
            }
        }
        jobs.retain_mut(|j| !matches!(j.child.try_wait(), Ok(Some(_))));
        if ender.ended(link, Duration::from_millis(200)) {
            break;
        }
    }
    running.stop();
    end_connections(&mut jobs);
    // The sessions end with the server.
    link.borrow_mut().with(Client::qap_end_all);
    netsvc::sysopr(link, msg::QAP_ENDED, 0, "QAP server ended.");
    netsvc::joblog(link, PGM, msg::QAP_ENDED, "QAP server ended.");
    let mut ctl = ctl;
    let _ = ctl.send(&FromJob::Ending {
        reason: "ENDTCPSVR".into(),
    });
    std::process::exit(0)
}
