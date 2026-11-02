// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NAVIGATOR` in `QHTTPSVR`: Navigator for QS/400 (`net/navigator`)
//! on 2002 and, with TLS, on 2005, started and ended with `ADMIN`
//! (`STRTCPSVR *HTTP HTTPSVR(*ADMIN)`). The TLS certificate is the one
//! `*SYSTEM` assigns `QS400_HTTP_SERVER_NAVIGATOR`, else the system's
//! own.
//!
//! The job's thread holds the link and the runtime (`web::bridge`);
//! the product's environment reads through the same service requests
//! the 5250 panels use and runs every change as a CL command with the
//! job swapped to the signed-on user ([`Link`]).

mod config;
mod db;
mod msgs;
mod net;
mod parse;
mod perf;
mod sec;
mod sw;
mod system;
mod users;
mod work;

use std::sync::Arc;

use ipc::chan::Chan;
use navigator::{Env, Navigator};
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
    Message::new(Kind::Error, "CPF9897", "Navigator is ending; the request was not run.")
}

impl Link {
    /// `f` on the job's thread with the job swapped to `user`, so the
    /// service checks that user's authority; the default when the user
    /// is not signed on here or the job is ending.
    pub fn read<T: Send + Default + 'static>(&self, user: &str, f: impl FnOnce(&mut Main) -> T + Send + 'static) -> T {
        let u = user.to_string();
        self.bridge.call(move |m| m.as_user(&u, f).ok()).flatten().unwrap_or_default()
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
fn start_listeners(bridge: &Bridge, nav: Arc<Navigator>, a: HttpAttrs, system: String) -> Result<Vec<Running>, String> {
    let mut running = Vec::new();
    let plain = listen::bind(a.port)
        .and_then(|l| listen::serve(l, None, Web::new(nav.clone())))
        .map_err(|e| format!("port {}: {e}", a.port))?;
    eprintln!("qsjob: NAVIGATOR listening on port {}", a.port);
    running.push(plain);
    if a.tls_port == 0 {
        return Ok(running);
    }
    match super::web::listen_tls(bridge, navigator::APP_ID, system, a.tls_port, nav) {
        Ok(r) => {
            eprintln!("qsjob: NAVIGATOR listening on port {} (TLS)", a.tls_port);
            running.push(r);
        }
        Err(e) => eprintln!("qsjob: NAVIGATOR port {}: {e}", a.tls_port),
    }
    Ok(running)
}

/// Run Navigator. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let a = HttpAttrs::from_rqsdta(&start.rqsdta);
    let console = crate::console::Console::new(ctl, link, start);
    let main = Main::new(link, console);
    let (bridge, rx) = Bridge::new();
    let name = system_name(&main);
    let core = Core::new(bridge.clone(), Dates::of(&main), name.clone());
    let l = Link { bridge: bridge.clone() };
    let me = format!("{:06}/{}/{}", start.number, start.user, start.name);
    let env = Env {
        system: Arc::new(system::LiveSystem { core, link: l.clone() }),
        work: Arc::new(work::LiveWork::new(l.clone(), me)),
        msgs: Arc::new(msgs::LiveMsgs { link: l.clone() }),
        users: Arc::new(users::LiveUsers { link: l.clone() }),
        config: Arc::new(config::LiveConfig { link: l.clone() }),
        net: Arc::new(net::LiveNet { link: l.clone() }),
        db: Arc::new(db::LiveDb { link: l.clone() }),
        sec: Arc::new(sec::LiveSecurity { link: l.clone() }),
        sw: Arc::new(sw::LiveSoftware::new(l.clone())),
        perf: Arc::new(perf::LivePerf { link: l.clone() }),
    };
    let nav = Arc::new(Navigator::new(env));
    let starter = bridge.clone();
    let setup = std::thread::spawn(move || start_listeners(&starter, nav, a, name));
    super::web::serve(main, &rx, setup, start.number)
}

#[cfg(test)]
mod tests {
    /// Every request here goes through `Main::ask`, whose borrow of the
    /// link ends before the answer comes back: a borrow held across a
    /// chain that asks again panicked the job (`/writers`).
    #[test]
    fn the_link_only_through_ask() {
        let sources = [
            ("config.rs", include_str!("config.rs")),
            ("db.rs", include_str!("db.rs")),
            ("msgs.rs", include_str!("msgs.rs")),
            ("net.rs", include_str!("net.rs")),
            ("parse.rs", include_str!("parse.rs")),
            ("perf.rs", include_str!("perf.rs")),
            ("sec.rs", include_str!("sec.rs")),
            ("sw.rs", include_str!("sw.rs")),
            ("system.rs", include_str!("system.rs")),
            ("users.rs", include_str!("users.rs")),
            ("work.rs", include_str!("work.rs")),
        ];
        for (name, src) in sources {
            for (n, line) in src.lines().enumerate() {
                let held = line.contains(".borrow_mut()") || line.contains(".borrow()");
                assert!(!held, "{name}:{}: use Main::ask, not the link's RefCell: {}", n + 1, line.trim());
            }
        }
    }
}
