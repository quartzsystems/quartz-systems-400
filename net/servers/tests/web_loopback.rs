// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A web handler behind the listener over loopback sockets: requests
//! kept alive on one connection, the peer plain, then HTTPS with the
//! certificate the store assigns to an application ID and the peer
//! secure, and the accept loop stopping.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use certs::apps::{AppRegistry, CaTrust};
use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::KeyAlgorithm;
use certs::files::MemFiles;
use certs::paths::{LOCAL_CA_STORE, SYSTEM_STORE};
use certs::store::{CertStore, StoreKind};
use certs::x509::build::{CertSpec, Subject};
use qsweb::http::{Handler, Peer, Request, Response};
use servers::listen::{bind_at, serve};
use servers::web::{Web, app_tls};
use tls::config::{ServerAuthMode, client_config, server_name, trusted_roots};
use tls::policy::TlsPolicy;
use tls::stream::TlsStream;

const HOST: &str = "qs400.test";
const APP_ID: &str = "QS400_HTTP_SERVER_ADMIN";

/// Says what it saw.
struct Echo;

impl Handler for Echo {
    fn handle(&self, req: &Request, peer: &Peer) -> Response {
        Response::html(
            200,
            format!("{} {} secure={}", req.method, req.path, peer.secure),
        )
    }
}

fn loopback() -> std::net::TcpListener {
    bind_at(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)), 1).unwrap()
}

fn exchange(stream: &mut (impl Read + Write), request: &str) -> String {
    stream.write_all(request.as_bytes()).unwrap();
    let mut out = Vec::new();
    let _ = stream.read_to_end(&mut out);
    String::from_utf8(out).unwrap()
}

fn connect(addr: SocketAddr) -> TcpStream {
    let s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s
}

#[test]
fn keep_alive_on_plain_http() {
    let running = serve(loopback(), None, Web::new(Arc::new(Echo))).unwrap();
    let addr = running.addr();
    let two = exchange(
        &mut connect(addr),
        "GET /a HTTP/1.1\r\nHost: x\r\n\r\nGET /b HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(two.matches("HTTP/1.1 200 OK").count(), 2, "{two}");
    assert!(two.contains("GET /a secure=false") && two.contains("GET /b secure=false"));
    assert!(exchange(&mut connect(addr), "nonsense\r\n\r\n").starts_with("HTTP/1.1 400 "));
    // An idle connection is closed.
    let quick = serve(
        loopback(),
        None,
        Web::with_idle(Arc::new(Echo), Duration::from_millis(200)),
    )
    .unwrap();
    let mut idle = connect(quick.addr());
    let mut b = [0u8; 16];
    assert_eq!(idle.read(&mut b).unwrap_or(0), 0);
    quick.stop();
    running.stop();
    let after = TcpStream::connect_timeout(&addr, Duration::from_millis(500));
    assert!(after.is_err(), "still accepting");
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 120
}

#[test]
fn https_with_the_application_ids_certificate() {
    let fast = Argon2Params {
        m_cost: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let files = MemFiles::new();
    let now = now();
    ca::create(
        &files,
        "capass1",
        &Subject::cn("Test CA"),
        KeyAlgorithm::EcdsaP256,
        now,
        3650,
        fast,
    )
    .unwrap();
    let ca_store = CertStore::open(&files, LOCAL_CA_STORE, "capass1").unwrap();
    let mut system = CertStore::create_with(
        &files,
        SYSTEM_STORE,
        "system1",
        StoreKind::System,
        now,
        fast,
    )
    .unwrap();
    ca::issue(
        &ca_store,
        &mut system,
        "ADMIN",
        &CertSpec::server(HOST, now, 30),
        KeyAlgorithm::EcdsaP256,
    )
    .unwrap();
    system.save(&files).unwrap();
    let mut apps = AppRegistry::load(&files).unwrap();
    apps.assign(APP_ID, Some("ADMIN")).unwrap();
    let policy = TlsPolicy::opsys();
    let cfg = app_tls(&system, &apps, APP_ID, &policy, now).unwrap();

    let running = serve(loopback(), Some(cfg), Web::new(Arc::new(Echo))).unwrap();
    let roots = trusted_roots(&system, &CaTrust::All).unwrap();
    let client = client_config(&policy, roots, None, ServerAuthMode::Full).unwrap();
    let mut tls =
        TlsStream::connect(client, server_name(HOST).unwrap(), connect(running.addr())).unwrap();
    let page = exchange(
        &mut tls,
        "GET /s HTTP/1.1\r\nHost: qs400.test:2010\r\nConnection: close\r\n\r\n",
    );
    assert!(page.starts_with("HTTP/1.1 200 OK\r\n"), "{page}");
    assert!(page.contains("GET /s secure=true"));
    // A plain client on the TLS port gets nothing.
    let plain = exchange(&mut connect(running.addr()), "GET / HTTP/1.1\r\n\r\n");
    assert!(!plain.contains("200 OK"));
    running.stop();
}
