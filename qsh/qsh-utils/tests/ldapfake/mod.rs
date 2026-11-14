// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A fake directory on the loopback written with the `ldap` crate's own
//! codec (as `runtime/ldapclnt`'s tests have it), and a TLS pair.

#![allow(dead_code, clippy::too_many_lines)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use ldap::ber::frame_len;
use ldap::code::ResultCode;
use ldap::control::{
    Control, ControlValue, EntryChange, PagedResults, PersistentSearch, SortRequest, SortResponse,
};
use ldap::ext;
use ldap::msg::{Attribute, BindAuth, LdapMessage, LdapResult, ProtocolOp};

use tls::rustls::{ClientConfig, ServerConfig};
use tls::stream::TlsStream;

/// The name the test certificate is for.
pub const HOST: &str = "localhost";

enum Io {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    Gone,
}

/// The server's side of one connection.
pub struct Conn {
    io: Io,
    buf: Vec<u8>,
    tls: Option<Arc<ServerConfig>>,
    bound: String,
}

impl Conn {
    fn recv(&mut self) -> Option<LdapMessage> {
        loop {
            if let Ok(Some(n)) = frame_len(&self.buf, 1 << 20) {
                let m = LdapMessage::decode(&self.buf[..n]).expect("a good message");
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

    fn reply(&mut self, id: i32, op: ProtocolOp, controls: Vec<Control>) {
        let b = LdapMessage { id, op, controls }.encode();
        let _ = match &mut self.io {
            Io::Plain(s) => s.write_all(&b),
            Io::Tls(t) => t.write_all(&b).and_then(|()| t.flush()),
            Io::Gone => Ok(()),
        };
    }
}

/// What the server saw, one line per request.
pub type Seen = Arc<Mutex<Vec<String>>>;

fn entry(dn: &str, attrs: &[(&str, &[&str])]) -> ProtocolOp {
    ProtocolOp::SearchResultEntry {
        dn: dn.into(),
        attrs: attrs.iter().map(|(t, v)| Attribute::new(t, v)).collect(),
    }
}

fn people() -> Vec<ProtocolOp> {
    vec![
        entry(
            "cn=alice,dc=example",
            &[
                ("objectClass", &["person"]),
                ("cn", &["alice"]),
                ("sn", &["Ärger"]),
                ("mail", &["alice@example.com", "a@example.com"]),
            ],
        ),
        entry("cn=bob,dc=example", &[("cn", &["bob"]), ("sn", &["B"])]),
    ]
}

fn result(op: &ProtocolOp, r: LdapResult) -> ProtocolOp {
    match op {
        ProtocolOp::AddRequest { .. } => ProtocolOp::AddResponse(r),
        ProtocolOp::ModifyRequest { .. } => ProtocolOp::ModifyResponse(r),
        ProtocolOp::DelRequest(_) => ProtocolOp::DelResponse(r),
        ProtocolOp::ModDnRequest { .. } => ProtocolOp::ModDnResponse(r),
        _ => ProtocolOp::CompareResponse(r),
    }
}

fn handle(c: &mut Conn, m: LdapMessage, seen: &Seen) -> bool {
    let id = m.id;
    let log = |s: String| seen.lock().unwrap().push(s);
    match m.op {
        ProtocolOp::BindRequest {
            version,
            name,
            auth,
        } => {
            let ok = match &auth {
                BindAuth::Simple(p) => {
                    log(format!(
                        "bind v{version} {name} {}",
                        String::from_utf8_lossy(p)
                    ));
                    p.is_empty() || p == b"secret"
                }
                BindAuth::Sasl { mechanism, .. } => {
                    log(format!("sasl {mechanism}"));
                    mechanism == "EXTERNAL" && matches!(c.io, Io::Tls(_))
                }
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
                    result: LdapResult::with_message(code, if ok { "" } else { "wrong password" }),
                    server_sasl_creds: None,
                },
                vec![],
            );
        }
        ProtocolOp::UnbindRequest => {
            log("unbind".into());
            return false;
        }
        ProtocolOp::AbandonRequest(n) => log(format!("abandon {n}")),
        ProtocolOp::ExtendedRequest { name, value } => {
            log(format!("exop {name}"));
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
                let cfg = c.tls.clone().expect("TLS configured");
                if let Io::Plain(s) = std::mem::replace(&mut c.io, Io::Gone) {
                    c.io = Io::Tls(Box::new(TlsStream::accept(cfg, s).unwrap()));
                }
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
                log(format!(
                    "passwd {:?} {:?}",
                    req.user,
                    req.new.as_deref().map(String::from_utf8_lossy)
                ));
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
                        result: LdapResult::with_message(
                            ResultCode::ProtocolError,
                            "unsupported extended operation",
                        ),
                        name: None,
                        value: None,
                    },
                    vec![],
                );
            }
        }
        ProtocolOp::SearchRequest(req) => {
            log(format!(
                "search {} {:?} {} {} {} {}",
                req.base,
                req.scope,
                req.size_limit,
                req.time_limit,
                req.filter,
                req.attrs.join(",")
            ));
            if req.base == "dc=slow" {
                return true;
            }
            if PersistentSearch::find(&m.controls).unwrap().is_some() {
                let ec = EntryChange {
                    change_type: 4,
                    previous_dn: None,
                    change_number: Some(7),
                };
                c.reply(id, people().remove(0), vec![ec.to_control(false)]);
                // The tools end a persistent search when the connection does.
                return false;
            }
            let mut all = people();
            if req.base == "dc=refs" {
                c.reply(
                    id,
                    ProtocolOp::SearchResultReference(vec!["ldap://other:389/dc=refs".into()]),
                    vec![],
                );
                c.reply(
                    id,
                    ProtocolOp::SearchResultDone(LdapResult::success()),
                    vec![],
                );
                return true;
            }
            let mut ctl = Vec::new();
            if let Some(s) = SortRequest::find(&m.controls).unwrap() {
                if s.0[0].reverse {
                    all.reverse();
                }
                ctl.push(
                    SortResponse {
                        result: ResultCode::Success,
                        attr: None,
                    }
                    .to_control(false),
                );
            }
            if let Some(p) = PagedResults::find(&m.controls).unwrap() {
                let start = usize::from(!p.cookie.is_empty());
                let end = (start + usize::try_from(p.size).unwrap()).min(all.len());
                for e in &all[start..end] {
                    c.reply(id, e.clone(), vec![]);
                }
                let cookie = if end < all.len() {
                    b"1".to_vec()
                } else {
                    Vec::new()
                };
                ctl.push(PagedResults { size: 2, cookie }.to_control(false));
            } else {
                for e in all {
                    c.reply(id, e, vec![]);
                }
            }
            c.reply(id, ProtocolOp::SearchResultDone(LdapResult::success()), ctl);
        }
        op @ (ProtocolOp::AddRequest { .. }
        | ProtocolOp::ModifyRequest { .. }
        | ProtocolOp::DelRequest(_)
        | ProtocolOp::ModDnRequest { .. }
        | ProtocolOp::CompareRequest { .. }) => {
            let (line, r) = match &op {
                ProtocolOp::AddRequest { dn, attrs } => (
                    format!(
                        "add {dn} {}",
                        attrs
                            .iter()
                            .map(|a| format!(
                                "{}={}",
                                a.ty,
                                a.vals
                                    .iter()
                                    .map(|v| String::from_utf8_lossy(v).into_owned())
                                    .collect::<Vec<_>>()
                                    .join("|")
                            ))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                    LdapResult::success(),
                ),
                ProtocolOp::ModifyRequest { dn, changes } => (
                    format!(
                        "modify {dn} {}",
                        changes
                            .iter()
                            .map(|c| format!("{:?}:{}={}", c.op, c.attr.ty, c.attr.vals.len()))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                    LdapResult::success(),
                ),
                ProtocolOp::DelRequest(dn) if dn.starts_with("cn=missing") => (
                    format!("delete {dn}"),
                    LdapResult {
                        code: ResultCode::NoSuchObject,
                        matched_dn: "dc=example".into(),
                        message: "no such entry".into(),
                        referral: vec![],
                    },
                ),
                ProtocolOp::DelRequest(dn) => (format!("delete {dn}"), LdapResult::success()),
                ProtocolOp::ModDnRequest {
                    dn,
                    new_rdn,
                    delete_old_rdn,
                    new_superior,
                } => (
                    format!("moddn {dn} {new_rdn} {delete_old_rdn} {new_superior:?}"),
                    LdapResult::success(),
                ),
                ProtocolOp::CompareRequest { dn, attr, value } => {
                    let ok = attr == "cn" && value == b"alice";
                    (
                        format!("compare {dn} {attr}"),
                        LdapResult::new(if ok {
                            ResultCode::CompareTrue
                        } else {
                            ResultCode::CompareFalse
                        }),
                    )
                }
                _ => unreachable!(),
            };
            log(format!(
                "{line} ctl={}",
                m.controls
                    .iter()
                    .map(|c| c.oid.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
            c.reply(id, result(&op, r), vec![]);
        }
        _ => {}
    }
    true
}

/// A fake directory on the loopback; LDAPS when `ldaps`.
pub fn server(tls: Option<Arc<ServerConfig>>, ldaps: bool) -> (SocketAddr, Seen) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let s2 = seen.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(s) = s else { break };
            let tls = tls.clone();
            let seen = s2.clone();
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
                    if !handle(&mut c, m, &seen) {
                        break;
                    }
                }
            });
        }
    });
    (addr, seen)
}

/// A server configuration for `localhost` and a client one trusting it.
pub fn tls_pair() -> (Arc<ServerConfig>, Arc<ClientConfig>) {
    use certs::ca;
    use certs::crypto::kdf::Argon2Params;
    use certs::crypto::keys::KeyAlgorithm;
    use certs::files::MemFiles;
    use certs::paths::{LOCAL_CA_STORE, SYSTEM_STORE};
    use certs::store::{CertStore, StoreKind};
    use certs::x509::build::{CertSpec, Subject};
    use tls::config::{ClientAuthMode, Identity, ServerAuthMode, client_config, server_config};
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
    let policy = tls::policy::TlsPolicy::opsys();
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
