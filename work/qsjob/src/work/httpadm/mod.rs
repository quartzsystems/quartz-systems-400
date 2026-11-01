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
//! connections' threads (`web::bridge`); [`system`] and [`jobs`] are
//! the product's environment over it.

mod jobs;
mod system;

use std::sync::Arc;

use httpadmin::Admin;
use httpadmin::env::Env;
use httpadmin::settings::Settings;
use ipc::chan::Chan;
use servers::attrs::http::HttpAttrs;
use servers::listen::{self, Running};
use servers::web::Web;
use stg::Shared;
use wm::ctl::Start;

use super::web::bridge::{Bridge, Main};
use super::web::system::{Core, Dates, system_name};
use httpadmin::certmgr::wire::Remote;
use jobs::{CertLink, LiveFiles, LiveJobs};
use system::LiveSystem;

/// The certificate application ID of the administration server.
const APP_ID: &str = "QS400_HTTP_SERVER_ADMIN";
/// The program the job log names.
const PGM: &str = "QZHBHTTP";

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
        Err(e) => {
            // The job log says why Server instances will not list it.
            let text = format!("Files of HTTP server instance ADMIN could not be used: {e}");
            eprintln!("qsjob: ADMIN: {text}");
            bridge.call(move |m| super::netsvc::joblog(&m.link, PGM, "HTP8106", &text));
        }
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
    match super::web::listen_tls(bridge, APP_ID, system, port, admin.clone()) {
        Ok(r) => {
            eprintln!("qsjob: ADMIN listening on port {port} (TLS)");
            admin.set_tls_up(true);
            running.push(r);
        }
        Err(e) => eprintln!("qsjob: ADMIN port {port}: {e}"),
    }
    Ok(running)
}

/// Run the administration server. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let a = HttpAttrs::from_rqsdta(&start.rqsdta);
    let console = crate::console::Console::new(ctl, link, start);
    let main = Main::new(link, console);
    let (bridge, rx) = Bridge::new();
    let name = system_name(&main);
    let job = format!("{:06}/{}/{}", start.number, start.user, start.name);
    let core = Core::new(bridge.clone(), Dates::of(&main), name.clone());
    let env = Env {
        system: Arc::new(LiveSystem { core, job }),
        jobs: Arc::new(LiveJobs { bridge: bridge.clone() }),
        files: Arc::new(LiveFiles { bridge: bridge.clone() }),
        certs: Arc::new(Remote(CertLink { bridge: bridge.clone() })),
    };
    let starter = bridge.clone();
    let setup = std::thread::spawn(move || start_listeners(&starter, env, a, name));
    // `bridge` stays: the job's thread is never without a caller.
    super::web::serve(main, &rx, setup, start.number)
}
