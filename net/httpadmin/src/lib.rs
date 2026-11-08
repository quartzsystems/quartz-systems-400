// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400 HTTP Server Administration, the `ADMIN` instance on port 2001
//! (docs/HTTPADMIN.md). Every page is server-rendered HTML from the
//! frame and partials of [`qsweb`]; every action goes through the
//! authority check of the command behind it.
//!
//! - [`env`]: what the product needs from the system, as traits.
//! - [`routes`]: the pages, by area.
//! - [`msgs`]: the `HTP` messages (`QHTTPSVR/QZHBMSG`).
//! - [`settings`]: Global settings.
//! - [`seed`]: the `ADMIN` instance made on a new system.
//! - [`fixture`]: the prototype's sample system, for tests and the demo.

pub mod certmgr;
pub mod env;
pub mod fixture;
pub mod help;
pub mod msgs;
pub mod routes;
pub mod seed;
pub mod settings;
pub mod util;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use qsweb::frame::{NavGroup, Product};
use qsweb::http::{Handler, Peer, Request, Response};
use qsweb::session::Sessions;

pub use env::Env;

/// The navigation tree.
pub const NAV: &[NavGroup] = &[
    NavGroup {
        label: "Manage",
        items: &[("Server instances", "/instances")],
    },
    NavGroup {
        label: "Create",
        items: &[("New HTTP server", "/create/step/1")],
    },
    NavGroup {
        label: "Certificates",
        items: &[
            ("Certificate stores", "/certs/stores"),
            ("Applications", "/certs/apps"),
        ],
    },
    NavGroup {
        label: "Advanced",
        items: &[("Global settings", "/global")],
    },
];

/// The product.
pub static PRODUCT: Product = Product {
    wordmark: "QS/400 HTTP Server Administration",
    crumb_root: "HTTP Server Administration",
    prefix: "/qs400/httpadmin",
    nav: NAV,
    system_label: "System",
    nav_script: false,
};

/// The administration instance's own name.
pub const SELF_INSTANCE: &str = "ADMIN";

/// The product over an environment: the request handler.
pub struct Admin {
    /// The system.
    pub env: Env,
    /// Signed-on users.
    pub sessions: Sessions,
    /// Whether the TLS port listens: "Require TLS" redirects to it only
    /// then, so a TLS port that failed to start cannot lock
    /// administration out.
    tls_up: AtomicBool,
}

impl Admin {
    /// The product over `env`.
    pub fn new(env: Env) -> Self {
        let s = settings::Settings::load(env.files.as_ref());
        Self {
            env,
            sessions: Sessions::new(Duration::from_secs(
                u64::from(s.session_minutes.max(1)) * 60,
            )),
            tls_up: AtomicBool::new(true),
        }
    }

    /// Whether the TLS port listens.
    pub fn tls_up(&self) -> bool {
        self.tls_up.load(Ordering::Relaxed)
    }

    /// Record whether the TLS port listens (the job, as it binds it).
    pub fn set_tls_up(&self, up: bool) {
        self.tls_up.store(up, Ordering::Relaxed);
    }
}

impl Handler for Admin {
    fn handle(&self, req: &Request, peer: &Peer) -> Response {
        routes::dispatch(self, req, peer)
    }
}
