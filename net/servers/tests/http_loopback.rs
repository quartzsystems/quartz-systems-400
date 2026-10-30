// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The administration listener over loopback sockets: the page over
//! plain HTTP, errors, then HTTPS with the certificate the store
//! assigns to `QIBM_HTTP_SERVER_ADMIN`, and the accept loop stopping.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use certs::apps::{AppRegistry, CaTrust};
use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::KeyAlgorithm;
use certs::files::MemFiles;
use certs::paths::{LOCAL_CA_STORE, SYSTEM_STORE};
use certs::store::{CertStore, StoreKind};
use certs::x509::build::{CertSpec, Subject};
use servers::http::{APP_ID, Admin, SiteInfo, admin_tls};
use servers::listen::{bind_at, serve};
use tls::config::{ServerAuthMode, client_config, server_name, trusted_roots};
use tls::policy::TlsPolicy;
use tls::stream::TlsStream;

const HOST: &str = "qs400.test";

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
fn plain_page_and_errors() {
    let running = serve(loopback(), None, Admin::new(SiteInfo::new("QS400T"))).unwrap();
    let addr = running.addr();
    let page = exchange(&mut connect(addr), "GET / HTTP/1.1\r\nHost: qs400\r\n\r\n");
    assert!(page.starts_with("HTTP/1.1 200 OK\r\n"), "{page}");
    assert!(page.contains("Content-Type: text/html; charset=utf-8\r\n"));
    assert!(page.contains("Connection: close\r\n"));
    assert!(page.contains("not yet available"));
    assert!(page.contains("QS400T"));
    assert!(page.contains("QS/400 V1R0M0"));
    let (head, body) = page.split_once("\r\n\r\n").unwrap();
    let len: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(len, body.len());
    let head_only = exchange(&mut connect(addr), "HEAD /HTTPAdmin HTTP/1.0\r\n\r\n");
    assert!(head_only.starts_with("HTTP/1.1 200 OK") && head_only.ends_with("\r\n\r\n"));
    assert!(exchange(&mut connect(addr), "GET /x HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 404 "));
    assert!(exchange(&mut connect(addr), "DELETE / HTTP/1.1\r\n\r\n").starts_with("HTTP/1.1 405 "));
    assert!(exchange(&mut connect(addr), "nonsense\r\n\r\n").starts_with("HTTP/1.1 400 "));
    running.stop();
    // Stopped: nothing accepts any more.
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
    ca::create(&files, "capass1", &Subject::cn("Test CA"), KeyAlgorithm::EcdsaP256, now, 3650, fast).unwrap();
    let ca_store = CertStore::open(&files, LOCAL_CA_STORE, "capass1").unwrap();
    let mut system = CertStore::create_with(&files, SYSTEM_STORE, "system1", StoreKind::System, now, fast).unwrap();
    ca::issue(&ca_store, &mut system, "ADMIN", &CertSpec::server(HOST, now, 30), KeyAlgorithm::EcdsaP256).unwrap();
    system.save(&files).unwrap();
    let mut apps = AppRegistry::load(&files).unwrap();
    apps.assign(APP_ID, Some("ADMIN")).unwrap();
    let policy = TlsPolicy::opsys();
    let cfg = admin_tls(&system, &apps, &policy, now).unwrap();

    let running = serve(loopback(), Some(cfg), Admin::new(SiteInfo::new("QS400T"))).unwrap();
    let roots = trusted_roots(&system, &CaTrust::All).unwrap();
    let client = client_config(&policy, roots, None, ServerAuthMode::Full).unwrap();
    let mut tls = TlsStream::connect(client, server_name(HOST).unwrap(), connect(running.addr())).unwrap();
    let page = exchange(&mut tls, "GET / HTTP/1.1\r\nHost: qs400.test:2005\r\n\r\n");
    assert!(page.starts_with("HTTP/1.1 200 OK\r\n"), "{page}");
    assert!(page.contains("not yet available"));
    // A plain client on the TLS port gets nothing.
    let plain = exchange(&mut connect(running.addr()), "GET / HTTP/1.1\r\n\r\n");
    assert!(!plain.contains("200 OK"));
    running.stop();
}
