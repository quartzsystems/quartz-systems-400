// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A session over a socket: the banner, then a command at a time until
//! `QUIT`, the client going away, or `INACTTIMO` seconds of silence.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use certs::apps::AppRegistry;
use certs::store::CertStore;
use tls::config::{AppConfig, for_app};
use tls::policy::TlsPolicy;
use tls::rustls::ServerConfig;

use crate::host::Host;
use crate::net::Net;
use crate::net::tcp::TcpNet;
use crate::reply::Reply;
use crate::session::{Flow, Session};

/// The FTP server's certificate application ID.
pub const APP_ID: &str = "QIBM_QTMF_FTP_SERVER";

/// The TLS configuration of `AUTH TLS` and port 990: the certificate
/// assigned to [`APP_ID`] in `store`.
pub fn server_tls(store: &CertStore, apps: &AppRegistry, policy: &TlsPolicy, now: u64) -> Result<Arc<ServerConfig>, String> {
    match for_app(store, apps, APP_ID, policy, now).map_err(|e| e.to_string())? {
        AppConfig::Server { config, .. } => Ok(config),
        AppConfig::Client { .. } => Err(format!("{APP_ID} is not a server application")),
    }
}

/// Serve one connection to the end.
pub fn serve<H: Host>(net: &mut TcpNet, session: &mut Session<H>) {
    if session.greet(net) == Flow::Close {
        net.shutdown();
        return;
    }
    let idle = match session.idle_secs() {
        0 => None,
        s => Some(Duration::from_secs(u64::from(s))),
    };
    loop {
        match net.read_line(idle) {
            Ok(Some(line)) => {
                if session.handle(&line, net) == Flow::Close {
                    break;
                }
            }
            Ok(None) => break,
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) => {
                let _ = net.send(&Reply::new(421, "The session was idle too long and is ending."));
                break;
            }
            Err(e) => {
                tracing::debug!("FTP control connection: {e}");
                break;
            }
        }
    }
    session.ended();
    net.shutdown();
}
