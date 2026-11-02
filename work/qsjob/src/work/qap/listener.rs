// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QAPD`: binds `CHGQAPA`'s port, TLS 1.3 only with the certificate
//! assigned to `QS400_ACCESS_SERVER` (no plain text and no fallback
//! certificate), says so to `QSYSOPR`, and runs until `ENDTCPSVR`,
//! `ENDJOB` or its subsystem ends it. A start that fails says why and
//! ends the job.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ipc::chan::Chan;
use servers::attrs::qap::QapAttrs;
use servers::listen::{self, Conn, Service};
use servers::msg;
use stg::Shared;
use wm::ctl::{FromJob, Start};

use crate::work::netsvc::{self, Ender};

const PGM: &str = "QAPD";
/// The certificate application ID.
pub const APP_ID: &str = "QS400_ACCESS_SERVER";

/// The accept loop's service.
struct Accept;

impl Service for Accept {
    fn serve(&self, mut conn: Conn, _peer: SocketAddr) {
        conn.close();
    }
}

/// Tell `QSYSOPR` and the job log why the server did not start, and end.
fn refuse(mut ctl: Chan, link: &Shared, why: &str) -> ! {
    let text = format!("QAP server could not be started: {why}.");
    netsvc::sysopr(link, msg::QAP_NOT_STARTED, 40, &text);
    netsvc::joblog(link, PGM, msg::QAP_NOT_STARTED, &text);
    crate::start::end(&mut ctl, why)
}

/// Run the listener. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let a = QapAttrs::from_bag(&netsvc::bag(link, "*QAP"));
    let tls = match crate::work::tls::assigned_tls13(&mut link.borrow_mut(), APP_ID) {
        Ok(c) => c,
        Err(e) => refuse(ctl, link, &e),
    };
    let running = match listen::bind(a.port).and_then(|l| listen::serve(l, Some(tls), Arc::new(Accept))) {
        Ok(r) => r,
        Err(e) => refuse(ctl, link, &format!("port {}: {e}", a.port)),
    };
    let text = format!("QAP server started on port {}.", a.port);
    netsvc::sysopr(link, msg::QAP_STARTED, 0, &text);
    netsvc::joblog(link, PGM, msg::QAP_STARTED, &text);
    stg::remote::status(link, wm::job::Status::Selw, "");
    let ender = Ender::new(&ctl, start.number);
    while !ender.ended(link, Duration::from_millis(500)) {}
    running.stop();
    netsvc::sysopr(link, msg::QAP_ENDED, 0, "QAP server ended.");
    netsvc::joblog(link, PGM, msg::QAP_ENDED, "QAP server ended.");
    let mut ctl = ctl;
    let _ = ctl.send(&FromJob::Ending {
        reason: "ENDTCPSVR".into(),
    });
    std::process::exit(0)
}
