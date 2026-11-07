// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400 Access for Web, the `ACCESSWEB` instance on ports 2003 and
//! 2006 (docs/ACCESSWEB.md): the end user's web interface. Every page is
//! rendered from the frame and partials of [`qsweb`]; every action is
//! gated by the product's policies ([`policy`]) on top of the object
//! authority of the command behind it, and runs as the signed-on user.
//!
//! - [`env`]: what the product needs from the system, a trait per area.
//! - [`routes`]: the pages, by area.
//! - [`policy`]: the functions, their policies and the effective view.
//! - [`prefs`]: the per-user settings.
//! - [`msgs`]: the product's messages (`QUSRSYS/QZXWMSG`).
//! - [`data`]: SQL scripts, transfer files and `.qdx` requests.
//! - [`fixture`]: the prototype's sample system, for tests and the demo.
//! - [`s5250`]: the 5250 session's screen, its look and the desktop's files.

pub mod data;
pub mod env;
pub mod fixture;
pub mod help;
pub mod msgs;
pub mod policy;
pub mod prefs;
pub mod routes;
pub mod s5250;
pub mod util;

use std::time::Duration;

use qsweb::frame::{NavGroup, Product};
use qsweb::http::{Handler, Peer, Request, Response};
use qsweb::session::Sessions;

pub use env::Env;

/// The navigation tree. A group whose one item has its own label is a
/// leaf (`qsweb::frame::NavGroup`).
pub const NAV: &[NavGroup] = &[
    NavGroup {
        label: "My home page",
        items: &[("My home page", "/home")],
    },
    NavGroup {
        label: "5250 session",
        items: &[("5250 session", "/5250")],
    },
    NavGroup {
        label: "Database",
        items: &[
            ("Tables", "/db/tables"),
            ("Run SQL", "/sql"),
            ("My requests", "/requests"),
            ("Copy data to table", "/db/copy/to"),
        ],
    },
    NavGroup {
        label: "Files",
        items: &[("Browse files", "/files"), ("My files", "/files/shares")],
    },
    NavGroup {
        label: "Print",
        items: &[("Printer output", "/print"), ("Printers", "/print/printers")],
    },
    NavGroup {
        label: "Messages",
        items: &[("Messages", "/messages")],
    },
    NavGroup {
        label: "Jobs",
        items: &[("My jobs", "/jobs"), ("Server jobs", "/jobs/server")],
    },
    NavGroup {
        label: "Commands",
        items: &[("Run command", "/cmd"), ("My commands", "/cmd/saved")],
    },
    NavGroup {
        label: "Customize",
        items: &[
            ("Preferences", "/preferences"),
            ("Change password", "/password"),
            ("Policies", "/policies"),
        ],
    },
    NavGroup {
        label: "Download",
        items: &[("Download", "/download")],
    },
];

/// The product.
pub static PRODUCT: Product = Product {
    wordmark: "QS/400 Access for Web",
    crumb_root: "Access for Web",
    prefix: "/qs400/accessweb",
    nav: NAV,
    system_label: "System",
};

/// The instance's own name, also its job's.
pub const SELF_INSTANCE: &str = "ACCESSWEB";

/// The licensed program the product is.
pub const PRODUCT_ID: &str = "QS00AFW";

/// The certificate application ID of the TLS port.
pub const APP_ID: &str = "QS400_HTTP_SERVER_ACCESSWEB";

/// The plain port.
pub const PORT: u16 = 2003;

/// The TLS port.
pub const TLS_PORT: u16 = 2006;

/// Minutes a session may sit idle when the product's settings say
/// nothing else.
pub const SESSION_MINUTES: u64 = 30;

/// The product over an environment: the request handler.
pub struct AccessWeb {
    /// The system.
    pub env: Env,
    /// Signed-on users.
    pub sessions: Sessions,
}

impl AccessWeb {
    /// The product over `env`.
    pub fn new(env: Env) -> Self {
        Self {
            env,
            sessions: Sessions::new(Duration::from_secs(SESSION_MINUTES * 60)),
        }
    }
}

impl Handler for AccessWeb {
    fn handle(&self, req: &Request, peer: &Peer) -> Response {
        routes::dispatch(self, req, peer)
    }
}
