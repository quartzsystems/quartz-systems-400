// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ACCESSWEB` in `QHTTPSVR`: QS/400 Access for Web (`net/accessweb`)
//! on 2003 and, with TLS, on 2006, started with `STRTCPSVR *HTTP
//! HTTPSVR(ACCESSWEB)`. The TLS certificate is the one `*SYSTEM`
//! assigns `QS400_HTTP_SERVER_ACCESSWEB`, else the system's own.
//!
//! The job's thread holds the link and the runtime (`web::bridge`);
//! every read runs with the job swapped to the signed-on user, every
//! change as a CL command as that user ([`Link`]). The policy and
//! preference stores are the product's own (`QUSRSYS/QZXWPOL`,
//! `QZXWPRF`), read and written as the job.

mod cmd;
mod db;
mod download;
mod files;
mod jobs;
mod msgs;
mod print;
mod s5250;
mod store;
mod system;

use std::sync::Arc;

use accessweb::{AccessWeb, Env};
use ipc::chan::Chan;
use qsweb::msg::{Kind, Message};
use servers::attrs::http::HttpAttrs;
use servers::listen::{self, Running};
use servers::web::Web;
use stg::Shared;
use wm::ctl::Start;

use super::web::bridge::{Bridge, Main};
use super::web::system::{Core, Dates, system_name};

/// The job's thread, as the product's environment uses it.
#[derive(Clone)]
pub struct Link {
    bridge: Bridge,
}

fn gone() -> Message {
    Message::new(
        Kind::Error,
        "CPF9897",
        "Access for Web is ending; the request was not run.",
    )
}

impl Link {
    /// `f` on the job's thread with the job swapped to `user`, so the
    /// service checks that user's authority; the default when the user
    /// is not signed on here or the job is ending.
    pub fn read<T: Send + Default + 'static>(
        &self,
        user: &str,
        f: impl FnOnce(&mut Main) -> T + Send + 'static,
    ) -> T {
        let u = user.to_string();
        self.bridge
            .call(move |m| m.as_user(&u, f).ok())
            .flatten()
            .unwrap_or_default()
    }

    /// `f` on the job's thread as the job itself (the product's own
    /// stores); `None` when the job is ending.
    pub fn job<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Main) -> T + Send + 'static,
    ) -> Option<T> {
        self.bridge.call(f)
    }

    /// Run CL `command` through the analyzer as `user`: the completion
    /// or escape message.
    pub fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        let (u, line) = (user.to_string(), command.to_string());
        self.bridge
            .call(move |m| super::web::run_as(m, Some(&u), &line))
            .unwrap_or_else(|| Err(gone()))
    }
}

/// The listeners, started off the job's thread.
fn start_listeners(
    bridge: &Bridge,
    app: Arc<AccessWeb>,
    a: HttpAttrs,
    system: String,
) -> Result<Vec<Running>, String> {
    let mut running = Vec::new();
    let plain = listen::bind(a.port)
        .and_then(|l| listen::serve(l, None, Web::new(app.clone())))
        .map_err(|e| format!("port {}: {e}", a.port))?;
    eprintln!("qsjob: ACCESSWEB listening on port {}", a.port);
    running.push(plain);
    if a.tls_port == 0 {
        return Ok(running);
    }
    match super::web::listen_tls(bridge, accessweb::APP_ID, system, a.tls_port, app) {
        Ok(r) => {
            eprintln!("qsjob: ACCESSWEB listening on port {} (TLS)", a.tls_port);
            running.push(r);
        }
        Err(e) => eprintln!("qsjob: ACCESSWEB port {}: {e}", a.tls_port),
    }
    Ok(running)
}

/// Run Access for Web. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let mut a = HttpAttrs::from_rqsdta(&start.rqsdta);
    if servers::attrs::keyword(&start.rqsdta, "PORT").is_none() {
        a = HttpAttrs::accessweb(&[]);
    }
    let console = crate::console::Console::new(ctl, link, start);
    let main = Main::new(link, console);
    let (bridge, rx) = Bridge::new();
    let name = system_name(&main);
    let dates = Dates::of(&main);
    let core = Core::new(bridge.clone(), dates.clone(), name.clone());
    let print_core = Arc::new(Core::new(bridge.clone(), dates.clone(), name.clone()));
    let l = Link {
        bridge: bridge.clone(),
    };
    let env = Env {
        system: Arc::new(system::LiveSystem {
            core,
            dates,
            link: l.clone(),
        }),
        policy: Arc::new(store::LivePolicy { link: l.clone() }),
        prefs: Arc::new(store::LivePrefs { link: l.clone() }),
        session5250: Arc::new(s5250::LiveS5250::new(l.clone())),
        db: Arc::new(db::LiveDb::new(l.clone(), start.number)),
        files: Arc::new(files::LiveFiles::new(l.clone())),
        print: Arc::new(print::LivePrint {
            link: l.clone(),
            core: print_core,
        }),
        msgs: Arc::new(msgs::LiveMsgs { link: l.clone() }),
        jobs: Arc::new(jobs::LiveJobs { link: l.clone() }),
        cmd: Arc::new(cmd::LiveCmd::new(l.clone())),
        download: Arc::new(download::LiveDownload {
            link: l.clone(),
            system: name.clone(),
        }),
    };
    let app = Arc::new(AccessWeb::new(env));
    let starter = bridge.clone();
    let setup = std::thread::spawn(move || start_listeners(&starter, app, a, name));
    super::web::serve(main, &rx, setup, start.number)
}

#[cfg(test)]
mod tests {
    /// Every request here goes through `Main::ask`, whose borrow of the
    /// link ends before the answer comes back.
    #[test]
    fn the_link_only_through_ask() {
        let sources = [
            ("db.rs", include_str!("db.rs")),
            ("files.rs", include_str!("files.rs")),
            ("print.rs", include_str!("print.rs")),
            ("store.rs", include_str!("store.rs")),
            ("system.rs", include_str!("system.rs")),
            ("msgs.rs", include_str!("msgs.rs")),
            ("jobs.rs", include_str!("jobs.rs")),
            ("cmd.rs", include_str!("cmd.rs")),
            ("download.rs", include_str!("download.rs")),
        ];
        for (name, src) in sources {
            for (n, line) in src.lines().enumerate() {
                let held = line.contains(".borrow_mut()") || line.contains(".borrow()");
                assert!(
                    !held,
                    "{name}:{}: use Main::ask, not the link's RefCell: {}",
                    n + 1,
                    line.trim()
                );
            }
        }
    }
}
