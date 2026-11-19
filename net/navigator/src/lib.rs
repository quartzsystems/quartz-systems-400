// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Navigator for QS/400, the `NAVIGATOR` instance on ports 2002 and
//! 2005 (docs/NAVIGATOR.md). Every page is the HTML form of a command or
//! API that already exists, rendered from the frame and partials of
//! [`qsweb`]; every action goes through the authority check of the
//! command behind it. Navigator keeps no state of its own beyond the
//! session and the Run SQL history.
//!
//! - [`env`]: what the product needs from the system, as traits by area.
//! - [`routes`]: the pages, by area.
//! - [`help`]: the help windows.
//! - [`fixture`]: the prototype's sample system, for tests and the demo.

pub mod env;
pub mod fixture;
pub mod help;
pub mod routes;
pub mod util;

use std::time::Duration;

use qsweb::brand::Brand;
use qsweb::frame::{NavGroup, Product};
use qsweb::http::{Handler, Peer, Request, Response};
use qsweb::session::Sessions;

pub use env::Env;

/// The navigation tree.
pub const NAV: &[NavGroup] = &[
    NavGroup {
        label: "System",
        items: &[
            ("System status", "/home"),
            ("System values", "/sysval"),
            ("Disk status", "/disk"),
            ("Hardware resources", "/hdwrsc"),
            ("Time management", "/time"),
            ("Power down / IPL", "/power"),
        ],
    },
    NavGroup {
        label: "Work management",
        items: &[
            ("Active jobs", "/jobs"),
            ("Job queues", "/jobq"),
            ("Subsystems", "/sbs"),
            ("Output queues", "/outq"),
            ("Printers", "/writers"),
            ("Scheduled jobs", "/jobscde"),
        ],
    },
    NavGroup {
        label: "Messages",
        items: &[
            ("QSYSOPR", "/msgq/QSYS/QSYSOPR"),
            ("My messages", "/mymsgs"),
            ("History log", "/qhst"),
        ],
    },
    NavGroup {
        label: "Users and groups",
        items: &[
            ("Users", "/users"),
            ("Groups", "/groups"),
            ("Authorization lists", "/autl"),
        ],
    },
    NavGroup {
        label: "Network",
        items: &[
            ("TCP/IP configuration", "/tcp"),
            ("Interfaces", "/tcp/ifc"),
            ("Routes", "/tcp/rte"),
            ("Servers", "/tcp/servers"),
            ("NetServer", "/netsvr"),
            ("NFS", "/nfs"),
            ("Directory server", "/dirsrv"),
        ],
    },
    NavGroup {
        label: "Database",
        items: &[("Schemas", "/db"), ("Run SQL", "/sql")],
    },
    NavGroup {
        label: "Security",
        items: &[
            ("Audit journal", "/audit"),
            ("Security values", "/sysval?cat=security"),
        ],
    },
    NavGroup {
        label: "Software",
        items: &[
            ("Licensed programs", "/licpgm"),
            ("PTFs", "/ptf"),
            ("Fixes", "/fixes"),
        ],
    },
    NavGroup {
        label: "Performance",
        items: &[
            ("System activity", "/perf/sys"),
            ("Disk activity", "/perf/disk"),
        ],
    },
];

/// QS/400's web products: the family [`PRODUCT`] belongs to.
pub static BRAND: Brand = Brand {
    name: "QS/400",
    vendor: "Quartz Systems",
    release: &release::RELEASE,
    server: "QS400",
    css_path: "/qs400/shared/qs400.css",
    nav_path: "/qs400/shared/qsnav.js",
    cookie: "QS400SID",
    signon_notice: "Unauthorized use of this system is prohibited. Sign-on attempts are logged to the audit journal.",
    notauth_note: "Contact the security officer (QSECOFR) if you need this authority. The attempt has been recorded in the audit journal (entry type AF).",
    job_log: true,
};

/// The product.
pub static PRODUCT: Product = Product {
    brand: &BRAND,
    wordmark: "Navigator for QS/400",
    crumb_root: "Navigator",
    prefix: "/qs400/navigator",
    nav: NAV,
    system_label: "System",
    nav_script: true,
};

/// The instance's own name, also its job's.
pub const SELF_INSTANCE: &str = "NAVIGATOR";

/// The certificate application ID of the TLS port.
pub const APP_ID: &str = "QS400_HTTP_SERVER_NAVIGATOR";

/// Minutes a session may sit idle.
pub const SESSION_MINUTES: u64 = 30;

/// The product over an environment: the request handler.
pub struct Navigator {
    /// The system.
    pub env: Env,
    /// Signed-on users.
    pub sessions: Sessions,
    /// Run SQL's statements and last result, per user, in memory.
    pub sql: routes::db::SqlState,
}

impl Navigator {
    /// The product over `env`.
    pub fn new(env: Env) -> Self {
        Self {
            env,
            sessions: Sessions::new(Duration::from_secs(SESSION_MINUTES * 60)),
            sql: routes::db::SqlState::default(),
        }
    }
}

impl Handler for Navigator {
    fn server(&self) -> String {
        BRAND.server_header()
    }

    fn handle(&self, req: &Request, peer: &Peer) -> Response {
        routes::dispatch(self, req, peer)
    }
}
