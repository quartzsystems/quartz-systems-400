// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The update service over HTTP or TLS: `ptf_update::server::Server`
//! answers the protocol (`docs/UPDATE_PROTOCOL.md`), a thread a
//! connection up to a limit, and `GET /healthz` answers a probe.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use ptf_update::http::{self, Response};
use ptf_update::server::Server;
use tls::rustls::ServerConfig;
use tls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tls::stream::TlsStream;

/// How the service listens.
pub struct Listen {
    /// The URL path it is under (`/qs400`), or empty.
    pub prefix: String,
    /// TLS, when it terminates TLS itself.
    pub tls: Option<Arc<ServerConfig>>,
    /// Connections served at once; more are closed.
    pub limit: usize,
}

/// A TLS configuration from a PEM certificate chain (leaf first) and a
/// PEM PKCS#8 private key.
pub fn tls_config(cert: &Path, key: &Path) -> Result<Arc<ServerConfig>> {
    let chain_pem = std::fs::read_to_string(cert).with_context(|| cert.display().to_string())?;
    let chain: Vec<CertificateDer<'static>> = certs::x509::pem_certificates(&chain_pem)?
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    let Some(leaf) = chain.first() else {
        bail!("{}: no certificate", cert.display());
    };
    let key_pem = std::fs::read_to_string(key).with_context(|| key.display().to_string())?;
    let Some(block) = certs::pem::blocks(&key_pem)?
        .into_iter()
        .find(|b| b.label == "PRIVATE KEY")
    else {
        bail!("{}: no PKCS#8 PRIVATE KEY block", key.display());
    };
    let identity = tls::config::Identity {
        label: "updates".into(),
        info: certs::x509::CertInfo::from_der(leaf)?,
        chain,
        key: Arc::new(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(block.der))),
    };
    Ok(tls::config::server_config(
        &identity,
        &tls::policy::TlsPolicy::opsys(),
        tls::config::ClientAuthMode::None,
        tls::rustls::RootCertStore::empty(),
    )?)
}

/// One request on `stream`: the probe, or the protocol.
fn answer(server: &Server, prefix: &str, stream: &mut (impl Read + Write)) {
    let Ok(req) = http::read_request(&mut *stream) else {
        return;
    };
    let res = if req.method == "GET" && req.target.split('?').next() == Some("/healthz") {
        Response::text(200, "ok\n")
    } else {
        server.handle(&req, prefix)
    };
    tracing::debug!(method = %req.method, target = %req.target, status = res.status);
    let _ = http::write_response(stream, &res);
}

fn connection(server: &Server, listen: &Listen, tcp: TcpStream) {
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(300)));
    match &listen.tls {
        None => {
            let mut s = tcp;
            answer(server, &listen.prefix, &mut s);
        }
        Some(cfg) => {
            let Ok(mut s) = TlsStream::accept(Arc::clone(cfg), tcp) else {
                return;
            };
            answer(server, &listen.prefix, &mut s);
            let _ = s.close();
        }
    }
}

/// Serve `dir` on `listener` until the process ends.
pub fn run(dir: &Path, listener: &TcpListener, listen: Listen) {
    let server = Arc::new(Server::new(dir));
    let listen = Arc::new(listen);
    let busy = Arc::new(AtomicUsize::new(0));
    for conn in listener.incoming() {
        let Ok(tcp) = conn else { continue };
        if busy.fetch_add(1, Ordering::SeqCst) >= listen.limit {
            busy.fetch_sub(1, Ordering::SeqCst);
            drop(tcp);
            continue;
        }
        let (server, listen, busy) = (Arc::clone(&server), Arc::clone(&listen), Arc::clone(&busy));
        std::thread::spawn(move || {
            connection(&server, &listen, tcp);
            busy.fetch_sub(1, Ordering::SeqCst);
        });
    }
}
