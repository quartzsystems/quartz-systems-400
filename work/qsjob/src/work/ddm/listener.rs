// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QRWTLSTN`, the DDM/DRDA listener (`STRTCPSVR *DDM`): it listens on
//! 446 and 447, and 448 with TLS, and gives each connection to a
//! `QRWTSRVR` prestart job of `QUSRWRK` through the descriptor relay.

use std::net::TcpListener;
use std::os::fd::OwnedFd;
use std::time::Duration;

use ipc::chan::Chan;
use sockets::relay;
use stg::Shared;
use wm::ctl::{Start, ToJob};
use wm::job::Status;

/// The prestart job program the listener hands connections to.
pub const SERVER_PROGRAM: &str = "QRWTSRVR";

fn bind(port: u16) -> Option<TcpListener> {
    match TcpListener::bind(("0.0.0.0", port)) {
        Ok(l) => Some(l),
        Err(e) => {
            eprintln!("qsjob: DDM port {port}: {e}");
            None
        }
    }
}

fn hand_over(l: &TcpListener, tls: bool, me: sockets::JobId) {
    let pool = relay::pool(SERVER_PROGRAM);
    for s in l.incoming() {
        let Ok(s) = s else { continue };
        let _ = s.set_nodelay(true);
        let fd = sockets::Fd::from(OwnedFd::from(s));
        let note: &[u8] = if tls { b"T" } else { b"" };
        if let Err(e) = relay::give(relay::SOCKET, &fd, &pool, &me, note) {
            eprintln!("qsjob: QRWTLSTN: handing a connection to {SERVER_PROGRAM}: {e:?}");
        }
    }
}

/// Run the listener. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let mut ctl = ctl;
    let me = crate::work::hostsvr::daemon::internal_id(start);
    let Some(main) = bind(446) else {
        crate::start::end(&mut ctl, "port 446 is in use");
    };
    std::thread::spawn(move || hand_over(&main, false, me));
    if let Some(l) = bind(447) {
        std::thread::spawn(move || hand_over(&l, false, me));
    }
    match TcpListener::bind(("0.0.0.0", 448)) {
        Ok(l) => {
            std::thread::spawn(move || hand_over(&l, true, me));
        }
        Err(e) => {
            crate::work::netsvc::tls_not_started(link, &start.name, "DDM", 448, &e.to_string());
        }
    }
    stg::remote::status(link, Status::Selw, "");
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
