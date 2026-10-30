// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server over loopback sockets, driven by the `FTP` command's
//! client: a batch script in the clear, with `AUTH TLS` and `PROT P`,
//! and on an implicit TLS port; active and passive data connections;
//! block mode.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::sync::Arc;

use certs::apps::AppRegistry;
use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::KeyAlgorithm;
use certs::files::MemFiles;
use certs::paths::{LOCAL_CA_STORE, SYSTEM_STORE};
use certs::store::{CertStore, StoreKind};
use certs::x509::build::{CertSpec, Subject};
use ftp::client::{Client, Console, Options, Security};
use ftp::memhost::MemHost;
use ftp::net::tcp::TcpNet;
use ftp::session::{Config, Session};
use servers::attrs::ftp::FtpAttrs;
use tls::config::{ServerAuthMode, client_config, trusted_roots};
use tls::policy::TlsPolicy;
use tls::rustls::{ClientConfig, ServerConfig};

const HOST: &str = "localhost";

struct Script {
    lines: VecDeque<String>,
    out: Vec<String>,
}

impl Console for Script {
    fn read_line(&mut self, _prompt: &str) -> Option<String> {
        self.lines.pop_front()
    }
    fn write(&mut self, line: &str) {
        self.out.push(line.to_string());
    }
}

fn script(lines: &[&str]) -> Script {
    Script {
        lines: lines.iter().map(|s| (*s).to_string()).collect(),
        out: Vec::new(),
    }
}

/// The server: one session per connection, each on its own thread with
/// a world of its own, `n` connections.
fn server(tls: Option<Arc<ServerConfig>>, implicit: bool, n: usize) -> SocketAddr {
    let l = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for s in l.incoming().take(n).flatten() {
            let tls = tls.clone();
            std::thread::spawn(move || {
                let mut host = MemHost::new(&[("BOB", "SECRET")]);
                host.add_library("BOBLIB");
                host.add_file("BOBLIB", "QCLSRC", 80, true, 37, "BOB");
                host.add_member("BOBLIB", "QCLSRC", "HELLO", &["PGM", "ENDPGM"]);
                let mut a = FtpAttrs::default();
                a.set("NAMEFMT", "*PATH").unwrap();
                a.set("CURDIR", "*HOMEDIR").unwrap();
                let mut net = TcpNet::new(s, tls, implicit, None).unwrap();
                let mut session = Session::new(host, Config::new(a, "QS400T"));
                ftp::server::serve(&mut net, &mut session);
            });
        }
    });
    addr
}

/// The local side: a library with a member to send and room to receive.
fn local() -> MemHost {
    let mut h = MemHost::new(&[("CLIENT", "X")]);
    h.add_library("MYLIB");
    h.add_file("MYLIB", "QTXTSRC", 80, true, 37, "CLIENT");
    h.add_member("MYLIB", "QTXTSRC", "NOTE", &["first line", "second line"]);
    h
}

fn tls_pair() -> (Arc<ServerConfig>, Arc<ClientConfig>) {
    let fast = Argon2Params {
        m_cost: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let files = MemFiles::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 120;
    ca::create(&files, "capass1", &Subject::cn("Test CA"), KeyAlgorithm::EcdsaP256, now, 3650, fast).unwrap();
    let ca_store = CertStore::open(&files, LOCAL_CA_STORE, "capass1").unwrap();
    let mut system = CertStore::create_with(&files, SYSTEM_STORE, "system1", StoreKind::System, now, fast).unwrap();
    ca::issue(&ca_store, &mut system, "FTPSVR", &CertSpec::server(HOST, now, 30), KeyAlgorithm::EcdsaP256).unwrap();
    system.save(&files).unwrap();
    let mut apps = AppRegistry::load(&files).unwrap();
    apps.assign(ftp::server::APP_ID, Some("FTPSVR")).unwrap();
    let policy = TlsPolicy::opsys();
    let server = ftp::server::server_tls(&system, &apps, &policy, now).unwrap();
    let roots = trusted_roots(&system, &certs::apps::CaTrust::All).unwrap();
    let client = client_config(&policy, roots, None, ServerAuthMode::Full).unwrap();
    (server, client)
}

/// Run a batch script against `addr`; the client's world and log.
fn run(addr: SocketAddr, opts: Options, lines: &[&str]) -> (MemHost, Vec<String>, Vec<u16>) {
    let mut c = Client::new(local(), script(lines), Options { port: addr.port(), ..opts }, "/QSYS.LIB/MYLIB.LIB");
    c.run(HOST);
    let log = c.console.out.clone();
    let codes = c.codes.clone();
    (c.host, log, codes)
}

const ROUND_TRIP: &[&str] = &[
    "bob SECRET",
    "put QTXTSRC.NOTE note.txt",
    "get note.txt QTXTSRC.BACK",
    "namefmt 1",
    "cd /QSYS.LIB/BOBLIB.LIB",
    "namefmt 0",
    "get QCLSRC.HELLO QTXTSRC.HELLO",
    "binary",
    "ls",
    "quote rcmd CHKOBJ OBJ(BOBLIB/QCLSRC) OBJTYPE(*FILE)",
    "quit",
];

fn check_round_trip(host: &MemHost, log: &[String], codes: &[u16]) {
    assert_eq!(host.member_text("MYLIB", "QTXTSRC", "BACK").unwrap(), ["first line", "second line"], "{log:#?}");
    assert_eq!(host.member_text("MYLIB", "QTXTSRC", "HELLO").unwrap(), ["PGM", "ENDPGM"], "{log:#?}");
    assert!(log.iter().any(|l| l == "QCLSRC.HELLO"), "{log:#?}");
    assert!(codes.contains(&230) && codes.contains(&221), "{codes:?}");
    assert!(codes.iter().filter(|c| **c >= 400).count() == 0, "{log:#?}");
}

#[test]
fn a_batch_script_in_the_clear() {
    let addr = server(None, false, 1);
    let (host, log, codes) = run(
        addr,
        Options {
            batch: true,
            ..Options::default()
        },
        ROUND_TRIP,
    );
    check_round_trip(&host, &log, &codes);
    // The password never shows.
    assert!(log.iter().all(|l| !l.contains("SECRET")), "{log:#?}");
    assert!(log.iter().any(|l| l.starts_with("215 ") && l.contains("OS/400")));
}

#[test]
fn active_data_connections_and_block_mode() {
    let addr = server(None, false, 1);
    let (host, log, codes) = run(
        addr,
        Options::default(),
        &[
            "bob SECRET",
            "sendpasv off",
            "put QTXTSRC.NOTE note.txt",
            "mode b",
            "namefmt 1",
            "cd /QSYS.LIB/BOBLIB.LIB",
            "namefmt 0",
            "get QCLSRC.HELLO QTXTSRC.HELLO",
            "quit",
        ],
    );
    assert_eq!(host.member_text("MYLIB", "QTXTSRC", "HELLO").unwrap(), ["PGM", "ENDPGM"], "{log:#?}");
    assert!(codes.contains(&200), "{log:#?}");
}

#[test]
fn explicit_tls_with_private_data() {
    let (server_cfg, client_cfg) = tls_pair();
    let addr = server(Some(server_cfg), false, 1);
    let (host, log, codes) = run(
        addr,
        Options {
            security: Security::Explicit,
            private: true,
            tls: Some(client_cfg),
            ..Options::default()
        },
        ROUND_TRIP,
    );
    assert!(codes.contains(&234), "{log:#?}");
    check_round_trip(&host, &log, &codes);
}

#[test]
fn implicit_tls() {
    let (server_cfg, client_cfg) = tls_pair();
    let addr = server(Some(server_cfg), true, 1);
    let (host, log, codes) = run(
        addr,
        Options {
            security: Security::Implicit,
            private: true,
            tls: Some(client_cfg),
            ..Options::default()
        },
        ROUND_TRIP,
    );
    check_round_trip(&host, &log, &codes);
}

#[test]
fn a_refused_sign_on_and_a_missing_system() {
    let addr = server(None, false, 1);
    let (_, log, codes) = run(addr, Options::default(), &["bob WRONG", "pwd", "quit"]);
    assert!(codes.contains(&530) && codes.contains(&530), "{log:#?}");
    let mut c = Client::new(local(), script(&["quit"]), Options { port: 9, ..Options::default() }, "/QSYS.LIB/MYLIB.LIB");
    c.run("127.0.0.1");
    assert!(c.console.out.iter().any(|l| l.starts_with("No connection")), "{:?}", c.console.out);
}
