// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ADMIN` in `QHTTPSVR`: HTTP Server Administration
//! (`STRTCPSVR *HTTP HTTPSVR(*ADMIN)`, `net/httpadmin`) on 2001 and,
//! with TLS, on 2010, kept alive, until `ENDTCPSVR` or `ENDJOB` ends
//! it. Global settings' TLS port moves the second one. The TLS
//! certificate is the one `*SYSTEM` assigns `QS400_HTTP_SERVER_ADMIN`,
//! else the system's own (`tls.rs`).
//!
//! The job's thread holds the link and the runtime and answers the
//! connections' threads ([`bridge`]); [`system`] and [`jobs`] are the
//! product's environment over it.

mod bridge;
mod jobs;
mod system;

use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use httpadmin::Admin;
use httpadmin::env::Env;
use httpadmin::settings::Settings;
use ipc::chan::Chan;
use servers::attrs::http::HttpAttrs;
use servers::listen::{self, Running};
use servers::web::Web;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start, ToJob};

use bridge::{Bridge, Main};
use httpadmin::certmgr::wire::Remote;
use jobs::{CertLink, LiveFiles, LiveJobs};
use system::{Dates, LiveSystem};

/// The certificate application ID of the administration server.
const APP_ID: &str = "QS400_HTTP_SERVER_ADMIN";
/// How often the job looks whether it is to end.
const LOOK: Duration = Duration::from_millis(500);

/// The listeners, started off the job's thread (the product reads its
/// settings through the job's thread while it starts).
fn start_listeners(bridge: &Bridge, env: Env, a: HttpAttrs, system: String) -> Result<Vec<Running>, String> {
    let settings = Settings::load(env.files.as_ref());
    // The product redirects to its settings' TLS port: listen there.
    let port = if settings.admin_tls_port == 0 { a.tls_port } else { settings.admin_tls_port };
    let now = env.system.now();
    let (d, t) = env.system.stamp(now);
    let tls = if a.tls_port == 0 { 0 } else { port };
    match httpadmin::seed::ensure_admin(env.files.as_ref(), a.port, tls, &format!("{d} {t}"), now) {
        Ok(true) => eprintln!("qsjob: ADMIN made instance ADMIN in /www/admin"),
        Ok(false) => {}
        Err(e) => eprintln!("qsjob: ADMIN could not make its instance: {e}"),
    }
    let admin = Arc::new(Admin::new(env));
    // Not up until it binds: 2001 serves rather than redirect to it.
    admin.set_tls_up(false);
    let mut running = Vec::new();
    let plain = listen::bind(a.port)
        .and_then(|l| listen::serve(l, None, Web::new(admin.clone())))
        .map_err(|e| format!("port {}: {e}", a.port))?;
    eprintln!("qsjob: ADMIN listening on port {}", a.port);
    running.push(plain);
    if a.tls_port == 0 {
        return Ok(running);
    }
    let cfg = bridge
        .call(move |m| super::tls::app_server_config(&mut m.link.borrow_mut(), APP_ID, &system))
        .unwrap_or_else(|| Err("the job is ending".into()));
    match cfg.and_then(|c| {
        listen::bind(port)
            .and_then(|l| listen::serve(l, Some(c), Web::new(admin.clone())))
            .map_err(|e| e.to_string())
    }) {
        Ok(r) => {
            eprintln!("qsjob: ADMIN listening on port {port} (TLS)");
            admin.set_tls_up(true);
            running.push(r);
        }
        Err(e) => eprintln!("qsjob: ADMIN port {port}: {e}"),
    }
    Ok(running)
}

/// Whether the job is to end: `End` from the parent, its subsystem
/// ending or `ENDJOB`.
fn ending(m: &Main, me: u32) -> bool {
    while let Ok(msg) = m.console.host.inbox.try_recv() {
        if matches!(msg, ToJob::End { .. }) {
            return true;
        }
    }
    let state = m.link.borrow_mut().with(Client::sbs_state);
    state.ending.is_some() || state.end_jobs.iter().any(|(n, _)| *n == me)
}

/// Tell the parent why, and exit.
fn finish(m: &Main, reason: &str, code: i32) -> ! {
    let _ = m.console.host.ctl.borrow_mut().send(&FromJob::Ending {
        reason: reason.to_string(),
    });
    std::process::exit(code)
}

/// Run the administration server. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let a = HttpAttrs::from_rqsdta(&start.rqsdta);
    let console = crate::console::Console::new(ctl, link, start);
    let mut main = Main::new(link, console);
    let (bridge, rx) = Bridge::new();
    let name = system::system_name(&main);
    let job = format!("{:06}/{}/{}", start.number, start.user, start.name);
    let env = Env {
        system: Arc::new(LiveSystem::new(bridge.clone(), job, Dates::of(&main), name.clone())),
        jobs: Arc::new(LiveJobs { bridge: bridge.clone() }),
        files: Arc::new(LiveFiles { bridge: bridge.clone() }),
        certs: Arc::new(Remote(CertLink { bridge: bridge.clone() })),
    };
    let starter = bridge.clone();
    let mut setup: Option<JoinHandle<Result<Vec<Running>, String>>> =
        Some(std::thread::spawn(move || start_listeners(&starter, env, a, name)));
    // `bridge` stays: the job's thread is never without a caller.
    let mut running: Vec<Running> = Vec::new();
    stg::remote::status(link, wm::job::Status::Selw, "");
    let me = start.number;
    let mut looked = Instant::now();
    loop {
        match rx.recv_timeout(LOOK) {
            Ok(task) => task(&mut main),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if setup.as_ref().is_some_and(JoinHandle::is_finished) {
            match setup.take().map(JoinHandle::join) {
                Some(Ok(Ok(r))) => running = r,
                Some(Ok(Err(e))) => finish(&main, &e, 1),
                _ => finish(&main, "the listeners did not start", 1),
            }
        }
        if looked.elapsed() >= LOOK {
            looked = Instant::now();
            if ending(&main, me) {
                break;
            }
        }
    }
    for r in running {
        r.stop();
    }
    finish(&main, "ENDTCPSVR", 0)
}
