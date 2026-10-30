// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ADMIN` in `QHTTPSVR`: the HTTP administration instance
//! (`STRTCPSVR *HTTP HTTPSVR(*ADMIN)`). It serves `net/servers`' one
//! page on 2001 and, with TLS, on 2005 until `ENDTCPSVR` or `ENDJOB`
//! ends it. The TLS certificate is the system's own self-signed one
//! (`tls.rs`) until the job can open the `*SYSTEM` store.
// BLOCKED(phase10): the certificate of QIBM_HTTP_SERVER_ADMIN needs a
// storage service request that answers an application ID's identity
// (the stash key is the LIC partition's); servers::http::admin_tls is ready.

use std::sync::mpsc;
use std::time::Duration;

use ipc::chan::Chan;
use servers::attrs::http::HttpAttrs;
use servers::http::{Admin, SiteInfo};
use servers::listen::{self, Running};
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start, ToJob};

/// The system's name, for the page.
fn system_name(link: &Shared) -> String {
    link.borrow_mut()
        .with(|l| l.sysval("QSYSNAME".into()))
        .map_or_else(|| "QS400".to_string(), |v| ccsid::from_ebcdic(&v.data).trim().to_string())
}

/// Run the administration instance. Never returns.
pub fn run(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let a = HttpAttrs::from_rqsdta(&start.rqsdta);
    let system = system_name(link);
    let admin = Admin::new(SiteInfo::new(&system));
    let mut running: Vec<Running> = Vec::new();
    match listen::bind(a.port).and_then(|l| listen::serve(l, None, admin.clone())) {
        Ok(r) => {
            eprintln!("qsjob: ADMIN listening on port {}", a.port);
            running.push(r);
        }
        Err(e) => crate::start::end(&mut ctl, &format!("port {}: {e}", a.port)),
    }
    if a.tls_port != 0 {
        let cfg = super::tls::server_config(&mut link.borrow_mut(), &system);
        match cfg.and_then(|c| {
            listen::bind(a.tls_port)
                .and_then(|l| listen::serve(l, Some(c), admin.clone()))
                .map_err(|e| e.to_string())
        }) {
            Ok(r) => {
                eprintln!("qsjob: ADMIN listening on port {} (TLS)", a.tls_port);
                running.push(r);
            }
            Err(e) => eprintln!("qsjob: ADMIN port {}: {e}", a.tls_port),
        }
    }
    // Only an explicit End from the parent counts; ENDJOB and ENDSBS
    // come through the service's state.
    let mut reader = ctl.try_clone().expect("ctl");
    let (end_tx, end_rx) = mpsc::channel::<bool>();
    std::thread::spawn(move || {
        while let Ok(m) = reader.recv::<ToJob>() {
            if let ToJob::End { immediate } = m {
                let _ = end_tx.send(immediate);
                return;
            }
        }
    });
    let me = start.number;
    stg::remote::status(link, wm::job::Status::Selw, "");
    loop {
        if end_rx.recv_timeout(Duration::from_millis(500)).is_ok() {
            break;
        }
        let state = link.borrow_mut().with(Client::sbs_state);
        if state.ending.is_some() || state.end_jobs.iter().any(|(n, _)| *n == me) {
            break;
        }
    }
    for r in running {
        r.stop();
    }
    let _ = ctl.send(&FromJob::Ending {
        reason: "ENDTCPSVR".into(),
    });
    std::process::exit(0)
}
