// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client against a small fake server on the loopback, written with
//! the crate's own codec: binds, searches, the asynchronous face,
//! paging, sorting, referrals, timeouts, a notice of disconnection,
//! LDAPS and `StartTLS`.

#![allow(clippy::many_single_char_names, clippy::too_many_lines)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::KeyAlgorithm;
use certs::files::MemFiles;
use certs::paths::{LOCAL_CA_STORE, SYSTEM_STORE};
use certs::store::{CertStore, StoreKind};
use certs::x509::build::{CertSpec, Subject};
use ldap::ber::frame_len;
use ldap::client::{Client, ClientError, Options};
use ldap::code::ResultCode;
use ldap::control::{
    Control, ControlValue, ManageDsaIt, PagedResults, SortKey, SortRequest, SortResponse,
};
use ldap::ext;
use ldap::msg::{Attribute, LdapMessage, LdapResult, ProtocolOp, Scope, SearchRequest};
use ldap::url::LdapUrl;
use tls::config::{ClientAuthMode, Identity, ServerAuthMode, client_config, server_config};
use tls::policy::TlsPolicy;
use tls::rustls::{ClientConfig, ServerConfig};
use tls::stream::TlsStream;

const HOST: &str = "qs400.test";

enum Io {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    Gone,
}

/// The server's side of one connection.
struct Conn {
    io: Io,
    buf: Vec<u8>,
    tls: Option<Arc<ServerConfig>>,
    bound: String,
}

impl Conn {
    fn recv(&mut self) -> Option<LdapMessage> {
        loop {
            if let Ok(Some(n)) = frame_len(&self.buf, 1 << 20) {
                let m = LdapMessage::decode(&self.buf[..n]).expect("client sent a bad message");
                self.buf.drain(..n);
                return Some(m);
            }
            let mut tmp = [0u8; 4096];
            let n = match &mut self.io {
                Io::Plain(s) => s.read(&mut tmp).ok()?,
                Io::Tls(t) => t.read(&mut tmp).ok()?,
                Io::Gone => return None,
            };
            if n == 0 {
                return None;
            }
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }

    fn send(&mut self, m: &LdapMessage) {
        let b = m.encode();
        let _ = match &mut self.io {
            Io::Plain(s) => s.write_all(&b),
            Io::Tls(t) => t.write_all(&b).and_then(|()| t.flush()),
            Io::Gone => Ok(()),
        };
    }

    fn reply(&mut self, id: i32, op: ProtocolOp, controls: Vec<Control>) {
        self.send(&LdapMessage { id, op, controls });
    }

    fn start_tls(&mut self) {
        let cfg = self.tls.clone().expect("no TLS configured");
        if let Io::Plain(s) = std::mem::replace(&mut self.io, Io::Gone) {
            self.io = Io::Tls(Box::new(TlsStream::accept(cfg, s).unwrap()));
        }
    }
}

type Script = Arc<dyn Fn(&mut Conn, LdapMessage) -> bool + Send + Sync>;

/// A server on the loopback running `script` for every message of every
/// connection; `ldaps` handshakes first.
fn server(script: Script, tls: Option<Arc<ServerConfig>>, ldaps: bool) -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(s) = s else { break };
            let script = script.clone();
            let tls = tls.clone();
            std::thread::spawn(move || {
                let io = if ldaps {
                    Io::Tls(Box::new(
                        TlsStream::accept(tls.clone().unwrap(), s).unwrap(),
                    ))
                } else {
                    Io::Plain(s)
                };
                let mut c = Conn {
                    io,
                    buf: Vec::new(),
                    tls,
                    bound: String::new(),
                };
                while let Some(m) = c.recv() {
                    if !script(&mut c, m) {
                        break;
                    }
                }
            });
        }
    });
    addr
}

fn entry(dn: &str) -> ProtocolOp {
    ProtocolOp::SearchResultEntry {
        dn: dn.into(),
        attrs: vec![Attribute::new(
            "cn",
            &[dn.split(['=', ',']).nth(1).unwrap()],
        )],
    }
}

fn done(r: LdapResult) -> ProtocolOp {
    ProtocolOp::SearchResultDone(r)
}

/// The fake directory. `peer` is another server's address for referrals.
fn directory(peer: Option<SocketAddr>, seen: Arc<Mutex<Vec<String>>>) -> Script {
    Arc::new(move |c: &mut Conn, m: LdapMessage| {
        let id = m.id;
        match m.op {
            ProtocolOp::BindRequest { name, auth, .. } => {
                let ok = match auth {
                    ldap::msg::BindAuth::Simple(p) => p.is_empty() || p == b"secret",
                    ldap::msg::BindAuth::Sasl { .. } => false,
                };
                let code = if ok {
                    c.bound = name;
                    ResultCode::Success
                } else {
                    ResultCode::InvalidCredentials
                };
                c.reply(
                    id,
                    ProtocolOp::BindResponse {
                        result: LdapResult::new(code),
                        server_sasl_creds: None,
                    },
                    vec![],
                );
            }
            ProtocolOp::UnbindRequest => return false,
            ProtocolOp::AbandonRequest(n) => seen.lock().unwrap().push(format!("abandon {n}")),
            ProtocolOp::ExtendedRequest { name, value } => {
                if name == ext::START_TLS {
                    c.reply(
                        id,
                        ProtocolOp::ExtendedResponse {
                            result: LdapResult::success(),
                            name: Some(name),
                            value: None,
                        },
                        vec![],
                    );
                    c.start_tls();
                } else if name == ext::WHO_AM_I {
                    let authz = if c.bound.is_empty() {
                        String::new()
                    } else {
                        format!("dn:{}", c.bound)
                    };
                    c.reply(
                        id,
                        ext::who_am_i_response(LdapResult::success(), &authz),
                        vec![],
                    );
                } else if name == ext::PASSWORD_MODIFY {
                    let req = ext::PasswordModifyRequest::decode_value(value.as_deref()).unwrap();
                    let made = ext::PasswordModifyResponse {
                        generated: req.new.is_none().then(|| b"made".to_vec()),
                    };
                    c.reply(
                        id,
                        ProtocolOp::ExtendedResponse {
                            result: LdapResult::success(),
                            name: None,
                            value: made.encode_value(),
                        },
                        vec![],
                    );
                } else {
                    c.reply(
                        id,
                        ProtocolOp::ExtendedResponse {
                            result: LdapResult::with_message(ResultCode::ProtocolError, "no"),
                            name: None,
                            value: None,
                        },
                        vec![],
                    );
                }
            }
            ProtocolOp::SearchRequest(req) => {
                seen.lock().unwrap().push(format!(
                    "search {} {} {} {}",
                    req.base, req.size_limit, req.time_limit, req.filter
                ));
                match req.base.as_str() {
                    "dc=x" => {
                        let mut names: Vec<String> =
                            (0..7).map(|i| format!("cn=e{i},dc=x")).collect();
                        let mut ctl = vec![];
                        if let Some(s) = SortRequest::find(&m.controls).unwrap() {
                            if s.0[0].reverse {
                                names.reverse();
                            }
                            ctl.push(
                                SortResponse {
                                    result: ResultCode::Success,
                                    attr: None,
                                }
                                .to_control(false),
                            );
                        }
                        let (from, to) = match PagedResults::find(&m.controls).unwrap() {
                            Some(p) => {
                                let from: usize = if p.cookie.is_empty() {
                                    0
                                } else {
                                    String::from_utf8(p.cookie).unwrap().parse().unwrap()
                                };
                                let to = (from + usize::try_from(p.size).unwrap()).min(names.len());
                                let cookie = if to == names.len() {
                                    vec![]
                                } else {
                                    to.to_string().into_bytes()
                                };
                                ctl.push(PagedResults { size: 7, cookie }.to_control(false));
                                (from, to)
                            }
                            None => (0, names.len()),
                        };
                        for n in &names[from..to] {
                            c.reply(id, entry(n), vec![]);
                        }
                        c.reply(id, done(LdapResult::success()), ctl);
                    }
                    "dc=ref" => {
                        let peer = peer.expect("no peer");
                        c.reply(id, entry("cn=here,dc=ref"), vec![]);
                        c.reply(
                            id,
                            ProtocolOp::SearchResultReference(vec![format!(
                                "ldap://{peer}/dc=peer??sub"
                            )]),
                            vec![],
                        );
                        c.reply(id, done(LdapResult::success()), vec![]);
                    }
                    "dc=gone" => {
                        let peer = peer.expect("no peer");
                        let mut r = LdapResult::new(ResultCode::Referral);
                        r.referral = vec![format!("ldap://{peer}/dc=peer")];
                        c.reply(id, done(r), vec![]);
                    }
                    "dc=peer" => {
                        c.reply(id, entry(&format!("cn=peer {},dc=peer", c.bound)), vec![]);
                        c.reply(id, done(LdapResult::success()), vec![]);
                    }
                    "dc=slow" => {}
                    _ => c.reply(id, done(LdapResult::new(ResultCode::NoSuchObject)), vec![]),
                }
            }
            ProtocolOp::DelRequest(dn) => match dn.as_str() {
                "cn=bye" => {
                    c.send(&ext::notice_of_disconnection(
                        ResultCode::Unavailable,
                        "bye",
                    ));
                    return false;
                }
                "cn=moved" => {
                    let mut r = LdapResult::new(ResultCode::Referral);
                    r.referral = vec![format!("ldap://{}/cn=there", peer.unwrap())];
                    c.reply(id, ProtocolOp::DelResponse(r), vec![]);
                }
                _ => {
                    seen.lock()
                        .unwrap()
                        .push(format!("delete {dn} by {}", c.bound));
                    // Echo the controls back to show they arrived.
                    c.reply(
                        id,
                        ProtocolOp::DelResponse(LdapResult::success()),
                        m.controls,
                    );
                }
            },
            ProtocolOp::CompareRequest { value, .. } => {
                let code = if value == b"yes" {
                    ResultCode::CompareTrue
                } else {
                    ResultCode::CompareFalse
                };
                c.reply(
                    id,
                    ProtocolOp::CompareResponse(LdapResult::new(code)),
                    vec![],
                );
            }
            ProtocolOp::AddRequest { .. } => {
                c.reply(id, ProtocolOp::AddResponse(LdapResult::success()), vec![]);
            }
            ProtocolOp::ModifyRequest { .. } => {
                c.reply(
                    id,
                    ProtocolOp::ModifyResponse(LdapResult::success()),
                    vec![],
                );
            }
            ProtocolOp::ModDnRequest { .. } => {
                c.reply(id, ProtocolOp::ModDnResponse(LdapResult::success()), vec![]);
            }
            _ => panic!("unexpected request"),
        }
        true
    })
}

fn opts() -> Options {
    Options {
        timeout: Some(Duration::from_secs(10)),
        ..Options::default()
    }
}

fn start() -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    (server(directory(None, seen.clone()), None, false), seen)
}

fn connect(addr: SocketAddr, o: Options) -> Client {
    Client::connect("127.0.0.1", addr.port(), o).unwrap()
}

fn search(base: &str) -> SearchRequest {
    SearchRequest::new(base.into(), Scope::Sub, ldap::Filter::any())
}

#[test]
fn bind_search_and_operations() {
    let (addr, seen) = start();
    let mut c = connect(
        addr,
        Options {
            size_limit: 50,
            time_limit: 9,
            ..opts()
        },
    );
    assert_eq!(
        c.simple_bind("cn=a", b"bad").unwrap().result.code,
        ResultCode::InvalidCredentials
    );
    assert!(
        c.simple_bind("cn=a", b"secret")
            .unwrap()
            .result
            .is_success()
    );
    assert_eq!(c.who_am_i().unwrap(), "dn:cn=a");
    let r = c.search(&search("dc=x"), &[]).unwrap();
    assert_eq!(r.entries.len(), 7);
    assert_eq!(r.entries[0].first("CN"), Some("e0"));
    assert!(r.result.is_success());
    assert_eq!(seen.lock().unwrap()[0], "search dc=x 50 9 (objectClass=*)");
    let r = c.search(&search("dc=none"), &[]).unwrap();
    assert_eq!(r.result.code, ResultCode::NoSuchObject);

    let ctl = [ManageDsaIt.to_control(true)];
    let r = c.delete("cn=a,dc=x", &ctl).unwrap();
    assert_eq!(r.controls, ctl);
    assert!(
        seen.lock()
            .unwrap()
            .contains(&"delete cn=a,dc=x by cn=a".to_string())
    );
    assert_eq!(
        c.compare("cn=a", "sn", b"yes", &[]).unwrap().result.code,
        ResultCode::CompareTrue
    );
    assert_eq!(
        c.compare("cn=a", "sn", b"no", &[]).unwrap().result.code,
        ResultCode::CompareFalse
    );
    assert!(
        c.add("cn=n", vec![Attribute::new("cn", &["n"])], &[])
            .unwrap()
            .result
            .is_success()
    );
    assert!(c.modify("cn=n", vec![], &[]).unwrap().result.is_success());
    assert!(
        c.modify_dn("cn=n", "cn=m", true, Some("dc=y"), &[])
            .unwrap()
            .result
            .is_success()
    );
    let g = c
        .password_modify(&ext::PasswordModifyRequest::default())
        .unwrap();
    assert_eq!(g.generated.as_deref(), Some(&b"made"[..]));
    let (r, _, _) = c.extended("1.2.3.4", None, &[]).unwrap();
    assert_eq!(r.result.code, ResultCode::ProtocolError);
    c.unbind().unwrap();
}

#[test]
fn asynchronous_by_message_id() {
    let (addr, seen) = start();
    let mut c = connect(addr, opts());
    let a = c
        .send(ProtocolOp::SearchRequest(search("dc=x")), vec![])
        .unwrap();
    let b = c
        .send(
            ProtocolOp::CompareRequest {
                dn: "cn=a".into(),
                attr: "sn".into(),
                value: b"yes".to_vec(),
            },
            vec![],
        )
        .unwrap();
    assert_ne!(a, b);
    // The compare's answer comes after the search's eight messages; ask
    // for it first and the search's wait in the queue.
    let m = c
        .poll(Some(b), Some(Duration::from_secs(10)))
        .unwrap()
        .unwrap();
    assert!(matches!(m.op, ProtocolOp::CompareResponse(_)));
    let mut n = 0;
    loop {
        let m = c
            .poll(Some(a), Some(Duration::from_secs(10)))
            .unwrap()
            .unwrap();
        assert_eq!(m.id, a);
        n += 1;
        if matches!(m.op, ProtocolOp::SearchResultDone(_)) {
            break;
        }
    }
    assert_eq!(n, 8);
    // Nothing is outstanding: a zero wait finds nothing.
    assert!(c.poll(None, Some(Duration::ZERO)).unwrap().is_none());
    // Abandon a search that never ends.
    let s = c
        .send(ProtocolOp::SearchRequest(search("dc=slow")), vec![])
        .unwrap();
    c.abandon(s).unwrap();
    assert!(
        c.poll(Some(s), Some(Duration::from_millis(50)))
            .unwrap()
            .is_none()
    );
    let r = c.search(&search("dc=x"), &[]).unwrap();
    assert_eq!(r.entries.len(), 7);
    assert!(seen.lock().unwrap().contains(&format!("abandon {s}")));
}

#[test]
fn paging_and_sorting() {
    let (addr, _) = start();
    let mut c = connect(addr, opts());
    let (page, cookie) = c.search_page(&search("dc=x"), 3, &[], &[]).unwrap();
    assert_eq!(page.entries.len(), 3);
    assert_eq!(cookie, b"3");
    let all = c.search_paged(&search("dc=x"), 3, &[]).unwrap();
    assert_eq!(all.entries.len(), 7);
    assert_eq!(all.entries[6].dn, "cn=e6,dc=x");
    let (r, s) = c
        .search_sorted(
            &search("dc=x"),
            vec![SortKey {
                reverse: true,
                ..SortKey::new("cn")
            }],
            true,
            &[],
        )
        .unwrap();
    assert_eq!(r.entries[0].dn, "cn=e6,dc=x");
    assert_eq!(s.unwrap().result, ResultCode::Success);
}

#[test]
fn referrals_off_by_default_and_chased_on_request() {
    let peer_seen = Arc::new(Mutex::new(Vec::new()));
    let peer = server(directory(None, peer_seen.clone()), None, false);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let addr = server(directory(Some(peer), seen), None, false);

    let mut c = connect(addr, opts());
    let r = c.search(&search("dc=ref"), &[]).unwrap();
    assert_eq!(r.entries.len(), 1);
    assert_eq!(r.references.len(), 1);
    let r = c.search(&search("dc=gone"), &[]).unwrap();
    assert_eq!(r.result.code, ResultCode::Referral);
    assert_eq!(r.result.referral.len(), 1);
    let r = c.delete("cn=moved", &[]).unwrap();
    assert_eq!(r.result.code, ResultCode::Referral);

    let mut c = connect(
        addr,
        Options {
            referrals: true,
            ..opts()
        },
    );
    let asked = Arc::new(Mutex::new(Vec::new()));
    let a2 = asked.clone();
    c.set_rebind(Box::new(move |u: &LdapUrl| {
        a2.lock().unwrap().push(u.dn.clone());
        Some(("cn=chaser".into(), b"secret".to_vec()))
    }));
    let r = c.search(&search("dc=ref"), &[]).unwrap();
    assert_eq!(r.entries.len(), 2);
    assert_eq!(r.entries[1].dn, "cn=peer cn=chaser,dc=peer");
    assert!(r.references.is_empty());
    let r = c.search(&search("dc=gone"), &[]).unwrap();
    assert!(r.result.is_success());
    assert_eq!(r.entries.len(), 1);
    let r = c.delete("cn=moved", &[]).unwrap();
    assert!(r.result.is_success());
    assert!(
        peer_seen
            .lock()
            .unwrap()
            .contains(&"delete cn=there by cn=chaser".to_string())
    );
    assert_eq!(asked.lock().unwrap().len(), 3);
    // The peer saw the URL's scope: sub, as asked.
    assert!(
        peer_seen
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.starts_with("search dc=peer"))
    );

    // No hops allowed: the chase is refused.
    let mut c = connect(
        addr,
        Options {
            referrals: true,
            hop_limit: 0,
            ..opts()
        },
    );
    assert!(matches!(
        c.search(&search("dc=gone"), &[]),
        Err(ClientError::HopLimit)
    ));
}

#[test]
fn timeouts_and_disconnection() {
    let (addr, seen) = start();
    let mut c = connect(
        addr,
        Options {
            timeout: Some(Duration::from_millis(200)),
            ..opts()
        },
    );
    assert!(matches!(
        c.search(&search("dc=slow"), &[]),
        Err(ClientError::Timeout)
    ));
    // The timed-out search was abandoned.
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|s| s.starts_with("abandon"))
    );
    let r = c.delete("cn=bye", &[]);
    match r {
        Err(ClientError::Disconnected(r)) => assert_eq!(r.code, ResultCode::Unavailable),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        c.delete("cn=x", &[]),
        Err(ClientError::Disconnected(_))
    ));
}

/// A server configuration and a client configuration that trusts it.
fn tls_pair() -> (Arc<ServerConfig>, Arc<ClientConfig>) {
    let fast = Argon2Params {
        m_cost: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 120;
    let files = MemFiles::new();
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
        "DIRSRV",
        &CertSpec::server(HOST, now, 30),
        KeyAlgorithm::EcdsaP256,
    )
    .unwrap();
    let policy = TlsPolicy::opsys();
    let id = Identity::from_store(&system, "DIRSRV", now).unwrap();
    let scfg = server_config(
        &id,
        &policy,
        ClientAuthMode::None,
        tls::rustls::RootCertStore::empty(),
    )
    .unwrap();
    let roots = tls::config::trusted_roots(&system, &certs::apps::CaTrust::All).unwrap();
    let ccfg = client_config(&policy, roots, None, ServerAuthMode::Full).unwrap();
    (scfg, ccfg)
}

#[test]
fn ldaps_and_start_tls() {
    let (scfg, ccfg) = tls_pair();
    let seen = Arc::new(Mutex::new(Vec::new()));

    let ldaps = server(directory(None, seen.clone()), Some(scfg.clone()), true);
    let s = TcpStream::connect(ldaps).unwrap();
    let mut c = Client::tls_over(s, HOST, ccfg.clone(), opts()).unwrap();
    assert!(c.is_tls());
    assert!(
        c.simple_bind("cn=t", b"secret")
            .unwrap()
            .result
            .is_success()
    );
    assert_eq!(c.search(&search("dc=x"), &[]).unwrap().entries.len(), 7);
    c.unbind().unwrap();

    let plain = server(directory(None, seen), Some(scfg), false);
    let s = TcpStream::connect(plain).unwrap();
    let mut c = Client::from_tcp(s, HOST, opts());
    assert!(!c.is_tls());
    c.start_tls(ccfg.clone()).unwrap();
    assert!(c.is_tls());
    assert!(c.start_tls(ccfg).is_err());
    assert!(
        c.simple_bind("cn=t", b"secret")
            .unwrap()
            .result
            .is_success()
    );
    assert_eq!(c.who_am_i().unwrap(), "dn:cn=t");
    let all = c.search_paged(&search("dc=x"), 2, &[]).unwrap();
    assert_eq!(all.entries.len(), 7);
    c.unbind().unwrap();
}
