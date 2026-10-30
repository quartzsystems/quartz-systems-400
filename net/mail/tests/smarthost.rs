// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SMTP client against a fake smart host on loopback: the whole
//! dialogue in plain text with `AUTH LOGIN`, then with `STARTTLS` and
//! `AUTH PLAIN` under a certificate from a store in memory, refusals,
//! and dot-stuffing on the wire.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::JoinHandle;

use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::KeyAlgorithm;
use certs::files::MemFiles;
use certs::paths::{LOCAL_CA_STORE, SYSTEM_STORE};
use certs::store::{CertStore, StoreKind};
use certs::x509::build::{CertSpec, Subject};
use mail::smtp::{self, Envelope, NoTls, Relay, RustlsConnector, SmtpError, TlsMode};
use tls::config::{Identity, ServerAuthMode, client_config, server_config, trusted_roots};
use tls::policy::TlsPolicy;
use tls::rustls::ServerConfig;
use tls::stream::TlsStream;

const HOST: &str = "relay.test";

/// What the fake host offers and refuses.
#[derive(Clone)]
struct Script {
    starttls: Option<Arc<ServerConfig>>,
    auth: &'static str,
    refuse: &'static [&'static str],
}

trait Rw: Read + Write + Send {}
impl<T: Read + Write + Send> Rw for T {}

fn line(s: &mut dyn Rw) -> Option<String> {
    let mut out = Vec::new();
    let mut b = [0u8; 1];
    loop {
        if s.read(&mut b).ok()? == 0 {
            return None;
        }
        if b[0] == b'\n' {
            return Some(String::from_utf8_lossy(&out).trim_end_matches('\r').to_string());
        }
        out.push(b[0]);
    }
}

fn say(s: &mut dyn Rw, text: &str) {
    s.write_all(text.as_bytes()).unwrap();
    s.flush().unwrap();
}

/// The host's side: plain until STARTTLS.
enum Side {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    Gone,
}

impl Side {
    fn rw(&mut self) -> &mut dyn Rw {
        match self {
            Side::Plain(t) => t,
            Side::Tls(t) => t.as_mut(),
            Side::Gone => panic!("no connection"),
        }
    }
}

/// One session; what the client sent (commands, then the message).
fn host(listener: TcpListener, script: Script) -> (SocketAddr, JoinHandle<(Vec<String>, String)>) {
    let addr = listener.local_addr().unwrap();
    let t = std::thread::spawn(move || {
        let (tcp, _) = listener.accept().unwrap();
        let mut side = Side::Plain(tcp);
        let mut cmds = Vec::new();
        let mut data = String::new();
        say(side.rw(), "220 relay.test fake relay\r\n");
        while let Some(l) = line(side.rw()) {
            cmds.push(l.clone());
            let up = l.to_ascii_uppercase();
            let plain = matches!(side, Side::Plain(_));
            if up == "STARTTLS" {
                say(side.rw(), "220 go ahead\r\n");
                let Side::Plain(tcp) = std::mem::replace(&mut side, Side::Gone) else {
                    panic!("TLS twice");
                };
                let cfg = script.starttls.clone().unwrap();
                side = Side::Tls(Box::new(TlsStream::accept(cfg, tcp).unwrap()));
                continue;
            }
            let s = side.rw();
            if up.starts_with("EHLO") {
                let mut r = String::from("250-relay.test\r\n");
                if script.starttls.is_some() && plain {
                    r.push_str("250-STARTTLS\r\n");
                }
                r.push_str("250-AUTH ");
                r.push_str(script.auth);
                r.push_str("\r\n250 8BITMIME\r\n");
                say(s, &r);
            } else if up.starts_with("AUTH PLAIN") {
                say(s, "235 ok\r\n");
            } else if up == "AUTH LOGIN" {
                say(s, "334 VXNlcm5hbWU6\r\n");
                cmds.push(line(s).unwrap());
                say(s, "334 UGFzc3dvcmQ6\r\n");
                cmds.push(line(s).unwrap());
                say(s, "235 ok\r\n");
            } else if up.starts_with("RCPT TO") && script.refuse.iter().any(|r| up.contains(&r.to_ascii_uppercase())) {
                say(s, "550 no such user\r\n");
            } else if up == "DATA" {
                say(s, "354 send it\r\n");
                while let Some(d) = line(s) {
                    if d == "." {
                        break;
                    }
                    data.push_str(&d);
                    data.push('\n');
                }
                say(s, "250 queued as 42\r\n");
            } else if up == "QUIT" {
                say(s, "221 bye\r\n");
                break;
            } else {
                say(s, "250 ok\r\n");
            }
        }
        (cmds, data)
    });
    (addr, t)
}

fn listener() -> TcpListener {
    TcpListener::bind("127.0.0.1:0").unwrap()
}

fn relay(addr: SocketAddr, tls: TlsMode, auth: Option<(&str, &str)>) -> Relay {
    Relay {
        host: HOST.into(),
        port: addr.port(),
        addrs: vec![addr],
        tls,
        auth: auth.map(|(u, p)| (u.to_string(), p.to_string())),
        ..Relay::new(HOST, "qs400.example.com")
    }
}

fn envelope() -> Envelope {
    Envelope {
        from: "qsecofr@qs400.example.com".into(),
        to: vec!["anne@example.com".into(), "ghost@example.com".into()],
    }
}

const MESSAGE: &[u8] = b"Subject: t\r\n\r\nline one\n.hidden\nend\n";

#[test]
fn plain_dialogue_with_auth_login() {
    let (addr, t) = host(
        listener(),
        Script {
            starttls: None,
            auth: "LOGIN",
            refuse: &["ghost@"],
        },
    );
    let r = smtp::send(&relay(addr, TlsMode::Opportunistic, Some(("anne", "pw"))), &envelope(), MESSAGE, &NoTls).unwrap();
    assert_eq!(r.accepted, ["anne@example.com"]);
    assert_eq!(r.refused, [("ghost@example.com".to_string(), 550, "no such user".to_string())]);
    assert_eq!(r.reply, "250 queued as 42");
    assert!(!r.secure);
    let (cmds, data) = t.join().unwrap();
    assert_eq!(
        cmds,
        [
            "EHLO qs400.example.com",
            "AUTH LOGIN",
            "YW5uZQ==",
            "cHc=",
            "MAIL FROM:<qsecofr@qs400.example.com>",
            "RCPT TO:<anne@example.com>",
            "RCPT TO:<ghost@example.com>",
            "DATA",
            "QUIT"
        ]
    );
    // The period line went doubled.
    assert_eq!(data, "Subject: t\n\nline one\n..hidden\nend\n");
}

#[test]
fn refusals() {
    // Every recipient refused.
    let (addr, t) = host(
        listener(),
        Script {
            starttls: None,
            auth: "PLAIN",
            refuse: &["@example.com"],
        },
    );
    let e = smtp::send(&relay(addr, TlsMode::None, None), &envelope(), MESSAGE, &NoTls).unwrap_err();
    assert!(matches!(e, SmtpError::NoRecipients(ref r) if r.len() == 2), "{e:?}");
    t.join().unwrap();
    // TLS required but not offered.
    let (addr, _t) = host(
        listener(),
        Script {
            starttls: None,
            auth: "PLAIN",
            refuse: &[],
        },
    );
    let e = smtp::send(&relay(addr, TlsMode::Required, None), &envelope(), MESSAGE, &NoTls).unwrap_err();
    assert!(matches!(e, SmtpError::Tls(_)), "{e:?}");
    // Nobody listening.
    let free = listener().local_addr().unwrap();
    let e = smtp::send(&relay(free, TlsMode::None, None), &envelope(), MESSAGE, &NoTls).unwrap_err();
    assert!(matches!(e, SmtpError::Connect(_)), "{e:?}");
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 120
}

#[test]
fn starttls_and_auth_plain() {
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
    ca::issue(&ca_store, &mut system, "RELAY", &CertSpec::server(HOST, now, 30), KeyAlgorithm::EcdsaP256).unwrap();
    let policy = TlsPolicy::opsys();
    let id = Identity::from_store(&system, "RELAY", now).unwrap();
    let scfg = server_config(&id, &policy, tls::config::ClientAuthMode::None, tls::rustls::RootCertStore::empty()).unwrap();
    let roots = trusted_roots(&system, &certs::apps::CaTrust::All).unwrap();
    let ccfg = client_config(&policy, roots, None, ServerAuthMode::Full).unwrap();

    let (addr, t) = host(
        listener(),
        Script {
            starttls: Some(scfg),
            auth: "PLAIN LOGIN",
            refuse: &[],
        },
    );
    let r = smtp::send(
        &relay(addr, TlsMode::Required, Some(("anne", "pw"))),
        &envelope(),
        MESSAGE,
        &RustlsConnector(ccfg),
    )
    .unwrap();
    assert!(r.secure);
    assert_eq!(r.accepted.len(), 2);
    let (cmds, data) = t.join().unwrap();
    assert_eq!(cmds[..3], ["EHLO qs400.example.com", "STARTTLS", "EHLO qs400.example.com"]);
    // NUL anne NUL pw.
    assert_eq!(cmds[3], "AUTH PLAIN AGFubmUAcHc=");
    assert!(data.contains("..hidden"));
}
