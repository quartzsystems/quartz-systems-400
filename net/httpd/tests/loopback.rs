// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An instance over loopback sockets: plain HTTP with keep-alive, then
//! a TLS virtual host with the certificate the store assigns to its
//! application ID, `SSLProtocolDisable`, and a controlled end that
//! waits for an open connection.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use certs::apps::{AppKind, AppRegistry, CaTrust};
use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::KeyAlgorithm;
use certs::files::MemFiles as CertFiles;
use certs::paths::{LOCAL_CA_STORE, SYSTEM_STORE};
use certs::store::{CertStore, StoreKind};
use certs::x509::build::{CertSpec, Subject};
use httpd::conf::parse;
use httpd::files::MemFiles;
use httpd::serve::auth::NoAuth;
use httpd::serve::site::Plan;
use httpd::serve::{Ctx, Deps, Server, Transport};
use tls::config::{AppConfig, ServerAuthMode, client_config, for_app, server_name, trusted_roots};
use tls::policy::{Protocol, TlsPolicy};
use tls::rustls::ServerConfig;
use tls::stream::TlsStream;

const HOST: &str = "shop.test";

/// Accept on `l` until `stop`, serving each connection on a thread.
fn accept(
    l: TcpListener,
    tls: Option<Arc<ServerConfig>>,
    server: Arc<Server>,
    stop: Arc<AtomicBool>,
) {
    l.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let Ok((s, peer)) = l.accept() else {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            };
            let server = Arc::clone(&server);
            let tls = tls.clone();
            std::thread::spawn(move || {
                s.set_nonblocking(false).unwrap();
                let ctx = Ctx {
                    local: s.local_addr().unwrap(),
                    peer,
                    secure: tls.is_some(),
                };
                let mut conn: Box<dyn Transport> = match tls {
                    Some(c) => match TlsStream::accept(c, s) {
                        Ok(t) => Box::new(t),
                        Err(_) => return,
                    },
                    None => Box::new(s),
                };
                server.serve_conn(conn.as_mut(), ctx);
            });
        }
    });
}

fn loopback() -> TcpListener {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap()
}

/// Read one response (head and a counted body).
fn response(r: &mut impl BufRead) -> String {
    let mut head = String::new();
    loop {
        let mut l = String::new();
        if r.read_line(&mut l).unwrap() == 0 {
            break;
        }
        head.push_str(&l);
        if l == "\r\n" {
            break;
        }
    }
    let len: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("Content-Length: "))
        .map_or(0, |v| v.trim().parse().unwrap());
    let mut body = vec![0; len];
    r.read_exact(&mut body).unwrap();
    head + &String::from_utf8(body).unwrap()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 120
}

#[test]
#[allow(clippy::too_many_lines)]
fn plain_and_tls_virtual_host_then_controlled_end() {
    let conf = "ServerRoot /www/shop\nListen 127.0.0.1:1\nListen 127.0.0.1:2\nDocumentRoot htdocs\nKeepAliveTimeout 5\n\
        <VirtualHost *:2>\n   ServerName shop.test\n   SSLEngine On\n   SSLProtocolDisable TLSv1.3\n   DocumentRoot /www/shop/secure\n</VirtualHost>\n";
    let plan = Plan::new("SHOP", "/www/shop", &parse(conf).unwrap());
    let spec = plan.listeners[1].tls.clone().unwrap();
    assert_eq!(spec.app_id, "QS400_HTTP_SERVER_SHOP");
    let files = Arc::new(MemFiles::new());
    files.put("/www/shop/htdocs/index.html", b"plain");
    files.put("/www/shop/secure/index.html", b"secure");

    // The certificate the store assigns to the instance's application ID.
    let fast = Argon2Params {
        m_cost: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let cf = CertFiles::new();
    let t = now();
    ca::create(
        &cf,
        "capass1",
        &Subject::cn("Test CA"),
        KeyAlgorithm::EcdsaP256,
        t,
        3650,
        fast,
    )
    .unwrap();
    let ca_store = CertStore::open(&cf, LOCAL_CA_STORE, "capass1").unwrap();
    let mut system =
        CertStore::create_with(&cf, SYSTEM_STORE, "system1", StoreKind::System, t, fast).unwrap();
    ca::issue(
        &ca_store,
        &mut system,
        "SHOP",
        &CertSpec::server(HOST, t, 30),
        KeyAlgorithm::EcdsaP256,
    )
    .unwrap();
    let mut apps = AppRegistry::load(&cf).unwrap();
    apps.register(&spec.app_id, AppKind::Server, "Shop")
        .unwrap();
    apps.assign(&spec.app_id, Some("SHOP")).unwrap();
    let policy = TlsPolicy::opsys();
    let AppConfig::Server {
        identity,
        client_auth,
        ..
    } = for_app(&system, &apps, &spec.app_id, &policy, t).unwrap()
    else {
        panic!("not a server application");
    };
    let roots = trusted_roots(&system, &CaTrust::All).unwrap();
    let cfg =
        httpd::serve::tls::config(&spec, &identity, &policy, roots.clone(), client_auth).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let (plain_l, tls_l) = (loopback(), loopback());
    let (plain_a, tls_a) = (plain_l.local_addr().unwrap(), tls_l.local_addr().unwrap());
    // The listeners bind ephemeral ports; the virtual host is matched by
    // the server's end, so the plan's ports are rewritten to them.
    let mut plan = plan;
    plan.listeners[0].port = plain_a.port();
    plan.listeners[1].port = tls_a.port();
    plan.vhosts[0].addrs = vec![(None, tls_a.port())];
    let server = Arc::new(Server::new(
        plan,
        Deps {
            files: files.clone(),
            auth: Arc::new(NoAuth),
            clock: Arc::new(now),
        },
    ));
    accept(plain_l, None, Arc::clone(&server), Arc::clone(&stop));
    accept(tls_l, Some(cfg), Arc::clone(&server), Arc::clone(&stop));

    // Plain, two requests on one connection.
    let s = TcpStream::connect(plain_a).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut w = s.try_clone().unwrap();
    let mut r = BufReader::new(s);
    w.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
    let one = response(&mut r);
    assert!(
        one.starts_with("HTTP/1.1 200 OK\r\n")
            && one.ends_with("plain")
            && one.contains("Connection: keep-alive"),
        "{one}"
    );
    w.write_all(b"GET /index.html HTTP/1.1\r\nHost: x\r\n\r\n")
        .unwrap();
    assert!(response(&mut r).ends_with("plain"));

    // TLS 1.2 only: the virtual host disabled 1.3.
    let client = client_config(&policy, roots, None, ServerAuthMode::Full).unwrap();
    let mut t = TlsStream::connect(client, server_name(HOST).unwrap(), {
        let s = TcpStream::connect(tls_a).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        s
    })
    .unwrap();
    t.write_all(b"GET / HTTP/1.1\r\nHost: shop.test\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut out = Vec::new();
    let _ = t.read_to_end(&mut out);
    let page = String::from_utf8(out).unwrap();
    assert!(
        page.starts_with("HTTP/1.1 200 OK\r\n") && page.ends_with("secure"),
        "{page}"
    );
    assert_eq!(t.session().unwrap().protocol, Protocol::Tls12);

    // A controlled end: the idle kept-alive connection closes; the
    // server drains.
    assert_eq!(server.connections(), 1);
    stop.store(true, Ordering::SeqCst);
    server.stop();
    assert!(server.drain(Duration::from_secs(5)));
    let mut rest = Vec::new();
    assert_eq!(r.read_to_end(&mut rest).unwrap_or(0), 0);
}
