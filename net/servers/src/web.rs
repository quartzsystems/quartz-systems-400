// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A web product ([`qsweb::http::Handler`]) as a listener's
//! [`Service`]: HTTP/1.1 with keep-alive on each connection, plain or
//! TLS, the peer's `secure` taken from the connection; a request whose
//! handler panics gets an error page and the process goes on
//! ([`qsweb::http::guard`]). The `ADMIN`
//! instance runs HTTP Server Administration this way on 2001 and 2010.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use certs::apps::AppRegistry;
use certs::store::CertStore;
use qsweb::http::{Handler, Peer};
use tls::config::{AppConfig, for_app};
use tls::policy::TlsPolicy;
use tls::rustls::ServerConfig;

use crate::listen::{Conn, Service};

/// How long a connection may sit idle between requests, or take to
/// send one.
pub const IDLE: Duration = Duration::from_secs(30);

/// A handler served on a listener's connections.
pub struct Web {
    handler: Arc<dyn Handler>,
    idle: Duration,
}

impl Web {
    /// Serving `handler`.
    pub fn new(handler: Arc<dyn Handler>) -> Arc<Self> {
        Self::with_idle(handler, IDLE)
    }

    /// Serving `handler`, a connection closed after `idle` without a
    /// request.
    pub fn with_idle(handler: Arc<dyn Handler>, idle: Duration) -> Arc<Self> {
        Arc::new(Self { handler, idle })
    }
}

impl Service for Web {
    fn serve(&self, mut conn: Conn, peer: SocketAddr) {
        let _ = conn.tcp().set_read_timeout(Some(self.idle));
        let _ = conn.tcp().set_write_timeout(Some(self.idle));
        let p = Peer {
            addr: peer,
            secure: conn.secure(),
        };
        qsweb::http::serve_guarded(&mut conn, &p, &self.handler);
        conn.close();
    }
}

/// The TLS configuration of application `app_id`: its certificate in
/// the `*SYSTEM` store (the store's default when none is assigned).
pub fn app_tls(
    store: &CertStore,
    apps: &AppRegistry,
    app_id: &str,
    policy: &TlsPolicy,
    now: u64,
) -> Result<Arc<ServerConfig>, String> {
    match for_app(store, apps, app_id, policy, now).map_err(|e| e.to_string())? {
        AppConfig::Server { config, .. } => Ok(config),
        AppConfig::Client { .. } => Err(format!("{app_id} is not a server application")),
    }
}
