// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The publisher (docs/DIRSRV.md §10.1) against a small directory on
//! the loopback, written with `ldap`'s codec: first publication with the
//! parent made, changes, resyncs that delete only this agent's stale
//! entries, foreign entries never touched, a target that is down, busy
//! or refuses the bind, entries refused one by one, printers, and the
//! configuration read again.

#![allow(clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dirsrv::publish::config::{AgentConfig, Kind, PublishConfig};
use dirsrv::publish::map::{Object, PrinterInfo, UserInfo};
use dirsrv::publish::runner::CONFIG_EVERY;
use dirsrv::publish::{MemSource, Runner, SourceConfig, Status, TcpConnect};
use ldap::ber::frame_len;
use ldap::code::ResultCode;
use ldap::dn::Dn;
use ldap::filter::Filter;
use ldap::msg::{Attribute, BindAuth, LdapMessage, LdapResult, ModOp, ProtocolOp, Scope};

const OURS: &str = "QS400 TESTSYS";
const PARENT: &str = "ou=Users,dc=example,dc=com";

fn key(dn: &str) -> String {
    Dn::parse(dn)
        .map_or_else(|_| dn.to_string(), |d| d.to_string())
        .to_ascii_lowercase()
}

/// The directory's state, shared with the test.
#[derive(Default)]
struct Dit {
    entries: BTreeMap<String, (String, Vec<Attribute>)>,
    ops: Vec<String>,
    connections: usize,
    /// Every connection is closed at once.
    down: bool,
    /// The bind DN and password a bind must give, when set.
    password: Option<(String, String)>,
    /// Answer the next N updates busy.
    busy: usize,
    /// Adds of DNs containing this get objectClassViolation.
    refuse: Option<String>,
}

impl Dit {
    fn put(&mut self, dn: &str, attrs: Vec<Attribute>) {
        self.entries.insert(key(dn), (dn.to_string(), attrs));
    }

    fn get(&self, dn: &str) -> Option<&Vec<Attribute>> {
        self.entries.get(&key(dn)).map(|(_, a)| a)
    }

    fn values(&self, dn: &str, ty: &str) -> Vec<String> {
        self.get(dn)
            .and_then(|a| a.iter().find(|a| a.ty.eq_ignore_ascii_case(ty)))
            .map(|a| {
                a.vals
                    .iter()
                    .map(|v| String::from_utf8_lossy(v).into())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn matched(&self, dn: &str) -> String {
        let mut d = Dn::parse(dn).unwrap_or(Dn::root());
        while let Some(p) = d.parent() {
            if self.entries.contains_key(&key(&p.to_string())) {
                return self.entries[&key(&p.to_string())].0.clone();
            }
            d = p;
        }
        String::new()
    }

    fn updates(&self) -> Vec<&str> {
        self.ops
            .iter()
            .map(String::as_str)
            .filter(|o| !o.starts_with("search") && !o.starts_with("bind"))
            .collect()
    }
}

type Shared = Arc<Mutex<Dit>>;

fn eval(f: &Filter, attrs: &[Attribute]) -> bool {
    let has = |ty: &str, v: Option<&[u8]>| {
        attrs.iter().any(|a| {
            a.ty.eq_ignore_ascii_case(ty)
                && v.is_none_or(|v| a.vals.iter().any(|x| x.eq_ignore_ascii_case(v)))
        })
    };
    match f {
        Filter::And(v) => v.iter().all(|f| eval(f, attrs)),
        Filter::Or(v) => v.iter().any(|f| eval(f, attrs)),
        Filter::Not(f) => !eval(f, attrs),
        Filter::Equality(t, v) => has(t, Some(v)),
        Filter::Present(t) => t.eq_ignore_ascii_case("objectClass") || has(t, None),
        _ => false,
    }
}

fn res(code: ResultCode) -> LdapResult {
    LdapResult::new(code)
}

fn handle(dit: &Shared, op: ProtocolOp, out: &mut Vec<ProtocolOp>) {
    let mut d = dit.lock().unwrap();
    let busy = |d: &mut Dit| {
        if d.busy > 0 {
            d.busy -= 1;
            true
        } else {
            false
        }
    };
    match op {
        ProtocolOp::BindRequest { name, auth, .. } => {
            d.ops.push(format!("bind {name}"));
            let BindAuth::Simple(pw) = auth else {
                unreachable!()
            };
            let ok = match &d.password {
                None => true,
                Some((n, p)) => key(n) == key(&name) && p.as_bytes() == pw.as_slice(),
            };
            out.push(ProtocolOp::BindResponse {
                result: res(if ok {
                    ResultCode::Success
                } else {
                    ResultCode::InvalidCredentials
                }),
                server_sasl_creds: None,
            });
        }
        ProtocolOp::SearchRequest(req) => {
            d.ops.push(format!("search {} {}", req.base, req.filter));
            let base = key(&req.base);
            if !d.entries.contains_key(&base) {
                let mut r = res(ResultCode::NoSuchObject);
                r.matched_dn = d.matched(&req.base);
                out.push(ProtocolOp::SearchResultDone(r));
                return;
            }
            let bdn = Dn::parse(&req.base).unwrap();
            for (dn, attrs) in d.entries.values() {
                let e = Dn::parse(dn).unwrap();
                let inside = match req.scope {
                    Scope::Base => key(dn) == base,
                    Scope::One => e.parent().is_some_and(|p| key(&p.to_string()) == base),
                    Scope::Sub => key(dn) == base || e.is_descendant_of(&bdn),
                };
                if inside && eval(&req.filter, attrs) {
                    out.push(ProtocolOp::SearchResultEntry {
                        dn: dn.clone(),
                        attrs: attrs.clone(),
                    });
                }
            }
            out.push(ProtocolOp::SearchResultDone(LdapResult::success()));
        }
        ProtocolOp::AddRequest { dn, attrs } => {
            d.ops.push(format!("add {dn}"));
            let code = if busy(&mut d) {
                ResultCode::Busy
            } else if d.refuse.as_ref().is_some_and(|r| dn.contains(r.as_str())) {
                ResultCode::ObjectClassViolation
            } else if d.entries.contains_key(&key(&dn)) {
                ResultCode::EntryAlreadyExists
            } else if Dn::parse(&dn)
                .unwrap()
                .parent()
                .is_some_and(|p| !d.entries.contains_key(&key(&p.to_string())))
            {
                ResultCode::NoSuchObject
            } else {
                d.put(&dn, attrs);
                ResultCode::Success
            };
            out.push(ProtocolOp::AddResponse(res(code)));
        }
        ProtocolOp::ModifyRequest { dn, changes } => {
            d.ops.push(format!("modify {dn}"));
            let k = key(&dn);
            let code = if busy(&mut d) {
                ResultCode::Busy
            } else if let Some((_, attrs)) = d.entries.get_mut(&k) {
                for c in changes {
                    let at = attrs
                        .iter()
                        .position(|a| a.ty.eq_ignore_ascii_case(&c.attr.ty));
                    match (c.op, at) {
                        (ModOp::Replace, Some(i)) if c.attr.vals.is_empty() => {
                            attrs.remove(i);
                        }
                        (ModOp::Replace, Some(i)) => attrs[i] = c.attr,
                        (ModOp::Replace | ModOp::Add, None) if !c.attr.vals.is_empty() => {
                            attrs.push(c.attr);
                        }
                        (ModOp::Add, Some(i)) => attrs[i].vals.extend(c.attr.vals),
                        (ModOp::Delete, Some(i)) => {
                            attrs.remove(i);
                        }
                        _ => {}
                    }
                }
                ResultCode::Success
            } else {
                ResultCode::NoSuchObject
            };
            out.push(ProtocolOp::ModifyResponse(res(code)));
        }
        ProtocolOp::DelRequest(dn) => {
            d.ops.push(format!("delete {dn}"));
            let k = key(&dn);
            let me = Dn::parse(&dn).unwrap();
            let code = if busy(&mut d) {
                ResultCode::Busy
            } else if !d.entries.contains_key(&k) {
                ResultCode::NoSuchObject
            } else if d
                .entries
                .values()
                .any(|(x, _)| Dn::parse(x).unwrap().is_descendant_of(&me))
            {
                ResultCode::NotAllowedOnNonLeaf
            } else {
                d.entries.remove(&k);
                ResultCode::Success
            };
            out.push(ProtocolOp::DelResponse(res(code)));
        }
        ProtocolOp::ModDnRequest {
            dn,
            new_rdn,
            delete_old_rdn,
            ..
        } => {
            d.ops.push(format!("moddn {dn} {new_rdn}"));
            let old = Dn::parse(&dn).unwrap();
            let rdn = ldap::dn::Rdn::parse(&new_rdn).unwrap();
            let new = old.parent().unwrap().child(rdn.clone()).to_string();
            let code = if !d.entries.contains_key(&key(&dn)) {
                ResultCode::NoSuchObject
            } else if d.entries.contains_key(&key(&new)) {
                ResultCode::EntryAlreadyExists
            } else {
                let (_, mut attrs) = d.entries.remove(&key(&dn)).unwrap();
                if delete_old_rdn {
                    for ava in &old.rdn().unwrap().0 {
                        if let Some(a) = attrs
                            .iter_mut()
                            .find(|a| a.ty.eq_ignore_ascii_case(&ava.ty))
                        {
                            a.vals.retain(|v| *v != ava.value);
                        }
                    }
                }
                for ava in &rdn.0 {
                    match attrs
                        .iter_mut()
                        .find(|a| a.ty.eq_ignore_ascii_case(&ava.ty))
                    {
                        Some(a) if !a.vals.contains(&ava.value) => a.vals.push(ava.value.clone()),
                        Some(_) => {}
                        None => attrs.push(Attribute {
                            ty: ava.ty.clone(),
                            vals: vec![ava.value.clone()],
                        }),
                    }
                }
                d.put(&new, attrs);
                ResultCode::Success
            };
            out.push(ProtocolOp::ModDnResponse(res(code)));
        }
        _ => {}
    }
}

fn conn(dit: &Shared, mut s: TcpStream) {
    let mut buf = Vec::new();
    loop {
        while let Ok(Some(n)) = frame_len(&buf, 1 << 20) {
            let m = LdapMessage::decode(&buf[..n]).expect("bad message from the publisher");
            buf.drain(..n);
            if matches!(m.op, ProtocolOp::UnbindRequest) {
                return;
            }
            let mut out = Vec::new();
            handle(dit, m.op, &mut out);
            for op in out {
                let b = LdapMessage {
                    id: m.id,
                    op,
                    controls: vec![],
                }
                .encode();
                if s.write_all(&b).is_err() {
                    return;
                }
            }
        }
        let mut tmp = [0u8; 4096];
        match s.read(&mut tmp) {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    }
}

fn server(dit: Shared) -> SocketAddr {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(s) = s else { break };
            let down = {
                let mut d = dit.lock().unwrap();
                d.connections += 1;
                d.down
            };
            if down {
                drop(s);
                continue;
            }
            let dit = dit.clone();
            std::thread::spawn(move || conn(&dit, s));
        }
    });
    addr
}

fn oc(classes: &[&str]) -> Attribute {
    Attribute::new("objectClass", classes)
}

/// A directory with the suffix only.
fn directory() -> (Shared, SocketAddr) {
    let dit: Shared = Arc::default();
    dit.lock().unwrap().put(
        "dc=example,dc=com",
        vec![oc(&["top", "domain"]), Attribute::new("dc", &["example"])],
    );
    let addr = server(dit.clone());
    (dit, addr)
}

fn user(name: &str, text: &str) -> Object {
    Object::User(UserInfo {
        name: name.into(),
        text: text.into(),
        mail: None,
    })
}

fn printer(dev: &str, text: &str) -> Object {
    Object::Printer(PrinterInfo {
        device: dev.into(),
        text: text.into(),
        outq: format!("QUSRSYS/{dev}"),
        make_model: Some("*HP4".into()),
        uri: Some(format!("lpd://qs.example.com/{dev}")),
    })
}

fn agent(kind: Kind, addr: SocketAddr, parent: &str) -> AgentConfig {
    let mut a = AgentConfig::new(kind);
    a.enabled = true;
    a.host = "127.0.0.1".into();
    a.port = addr.port();
    a.parent_dn = Some(parent.into());
    a.retry = 30;
    a.publisher = Some(OURS.into());
    a
}

fn source(agents: &[AgentConfig]) -> MemSource {
    let mut config = PublishConfig::default();
    for a in agents {
        *config.agent_mut(a.kind) = a.clone();
    }
    MemSource::new(SourceConfig {
        config,
        passwords: Vec::new(),
        system: "TESTSYS".into(),
    })
}

fn connector() -> TcpConnect {
    TcpConnect::new(None, Duration::from_secs(5))
}

/// A runner of `kind` turned once at `now`.
fn turn(r: &mut Runner, src: &mut MemSource, now: u64) -> Status {
    r.tick(src, &mut connector(), now);
    r.status()
}

const T0: u64 = 1_000_000;

#[test]
fn users_are_published_under_a_parent_made_for_them() {
    let (dit, addr) = directory();
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.objects = vec![user("JSMITH", "Jane Smith"), user("BOB", "")];
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    assert_eq!(s.last_error, None, "{s:?}");
    assert_eq!(s.last_success, Some(T0));
    assert_eq!((s.pending, s.published, s.errors), (0, 2, 0));
    assert!(s.enabled && s.connected);
    let d = dit.lock().unwrap();
    assert_eq!(
        d.values(PARENT, "objectClass"),
        ["top", "organizationalUnit"]
    );
    let jane = "cn=Jane Smith,ou=Users,dc=example,dc=com";
    assert_eq!(d.values(jane, "uid"), ["JSMITH"]);
    assert_eq!(d.values(jane, "sn"), ["Smith"]);
    assert_eq!(d.values(jane, "publisherName"), [OURS]);
    assert!(
        d.values(jane, "objectClass")
            .contains(&"inetOrgPerson".to_string())
    );
    assert!(d.values(jane, "mail").is_empty());
    assert_eq!(d.values("cn=BOB,ou=Users,dc=example,dc=com", "sn"), ["BOB"]);
    assert_eq!(
        d.updates(),
        [
            "add ou=Users,dc=example,dc=com",
            "add cn=BOB,ou=Users,dc=example,dc=com",
            "add cn=Jane Smith,ou=Users,dc=example,dc=com",
        ]
    );
}

#[test]
fn ancestors_below_the_matched_dn_are_made() {
    let (dit, addr) = directory();
    let parent = "ou=People,o=Sales,dc=example,dc=com";
    let mut src = source(&[agent(Kind::Users, addr, parent)]);
    src.objects = vec![user("A", "Ann")];
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    assert_eq!(s.last_error, None);
    let d = dit.lock().unwrap();
    assert_eq!(
        d.values("o=Sales,dc=example,dc=com", "objectClass"),
        ["top", "organization"]
    );
    assert_eq!(d.values(parent, "ou"), ["People"]);
    assert!(
        d.get("cn=Ann,ou=People,o=Sales,dc=example,dc=com")
            .is_some()
    );
}

#[test]
fn a_missing_parent_without_leave_to_make_it_waits() {
    let (dit, addr) = directory();
    let mut a = agent(Kind::Users, addr, PARENT);
    a.create_parent = false;
    let mut src = source(&[a]);
    src.objects = vec![user("A", "Ann")];
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    assert!(s.last_error.unwrap().1.contains("does not exist"));
    assert_eq!(s.pending, 1);
    assert_eq!(s.next_try, Some(T0 + 30));
    assert!(dit.lock().unwrap().updates().is_empty());
}

#[test]
fn changes_follow_the_profiles() {
    let (dit, addr) = directory();
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.objects = vec![user("JSMITH", "Jane Smith"), user("BOB", "Bob Jones")];
    let mut r = Runner::new(Kind::Users);
    turn(&mut r, &mut src, T0);
    dit.lock().unwrap().ops.clear();
    // Jane marries, Bob goes, Carl comes.
    src.objects = vec![user("JSMITH", "Jane Doe"), user("CARL", "Carl")];
    let s = turn(&mut r, &mut src, T0 + 5);
    assert_eq!(s.last_error, None, "{s:?}");
    assert_eq!((s.pending, s.published), (0, 2));
    let d = dit.lock().unwrap();
    // In the order of the profiles' names.
    assert_eq!(
        d.updates(),
        [
            "delete cn=Bob Jones,ou=Users,dc=example,dc=com",
            "add cn=Carl,ou=Users,dc=example,dc=com",
            "moddn cn=Jane Smith,ou=Users,dc=example,dc=com cn=Jane Doe",
            "modify cn=Jane Doe,ou=Users,dc=example,dc=com",
        ]
    );
    let jane = "cn=Jane Doe,ou=Users,dc=example,dc=com";
    assert_eq!(d.values(jane, "cn"), ["Jane Doe"]);
    assert_eq!(d.values(jane, "sn"), ["Doe"]);
    assert!(d.get("cn=Jane Smith,ou=Users,dc=example,dc=com").is_none());
    // Nothing changed: nothing sent.
    drop(d);
    dit.lock().unwrap().ops.clear();
    turn(&mut r, &mut src, T0 + 10);
    assert!(dit.lock().unwrap().updates().is_empty());
}

/// A directory with entries of others and a stale one of ours.
fn crowded() -> (Shared, SocketAddr) {
    let (dit, addr) = directory();
    {
        let mut d = dit.lock().unwrap();
        d.put(
            PARENT,
            vec![
                oc(&["top", "organizationalUnit"]),
                Attribute::new("ou", &["Users"]),
            ],
        );
        let person = |cn: &str, uid: &str, publisher: Option<&str>| {
            let mut v = vec![
                oc(&["top", "person", "inetOrgPerson", "publisher"]),
                Attribute::new("cn", &[cn]),
                Attribute::new("sn", &[cn]),
                Attribute::new("uid", &[uid]),
            ];
            if let Some(p) = publisher {
                v.push(Attribute::new("publisherName", &[p]));
            }
            v
        };
        // Ours, for a profile that is gone.
        d.put(
            "cn=Old User,ou=Users,dc=example,dc=com",
            person("Old User", "OLDUSR", Some(OURS)),
        );
        // Another system's, for a profile of the same name as one of ours.
        d.put(
            "cn=Jane Elsewhere,ou=Users,dc=example,dc=com",
            person("Jane Elsewhere", "JSMITH", Some("QS400 OTHER")),
        );
        // Nobody's, holding the name our profile wants.
        d.put(
            "cn=Jane Smith,ou=Users,dc=example,dc=com",
            person("Jane Smith", "jane", None),
        );
        // Nobody's, for a gone profile's name.
        d.put(
            "cn=Gone,ou=Users,dc=example,dc=com",
            person("Gone", "OLDUSR", None),
        );
    }
    (dit, addr)
}

#[test]
fn a_resync_deletes_only_our_stale_entries() {
    let (dit, addr) = crowded();
    let before = dit.lock().unwrap().entries.clone();
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.objects = vec![user("JSMITH", "Jane Smith")];
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    assert_eq!(s.last_error, None, "{s:?}");
    let d = dit.lock().unwrap();
    // The stale one is gone.
    assert!(d.get("cn=Old User,ou=Users,dc=example,dc=com").is_none());
    // Foreign ones are as they were, byte for byte.
    for dn in [
        "cn=Jane Elsewhere,ou=Users,dc=example,dc=com",
        "cn=Jane Smith,ou=Users,dc=example,dc=com",
        "cn=Gone,ou=Users,dc=example,dc=com",
    ] {
        assert_eq!(d.entries.get(&key(dn)), before.get(&key(dn)), "{dn}");
    }
    // Ours took the second name, since the first is someone else's.
    let ours = "cn=Jane Smith+uid=JSMITH,ou=Users,dc=example,dc=com";
    assert_eq!(d.values(ours, "publisherName"), [OURS]);
    // No update ever named a foreign entry but the refused add.
    for op in d.updates() {
        assert!(
            !op.contains("Elsewhere")
                && !op.contains("Gone")
                && !op.starts_with("modify cn=Jane Smith,"),
            "{op}"
        );
    }
    assert!(
        d.updates()
            .contains(&"delete cn=Old User,ou=Users,dc=example,dc=com")
    );
    drop(d);
    // A later resync finds ours where it is and leaves it.
    dit.lock().unwrap().ops.clear();
    r.publisher().unwrap();
    src.tracker = dirsrv::publish::Tracker::new(2); // the service restarted
    let s = turn(&mut r, &mut src, T0 + 5);
    assert_eq!(s.last_error, None);
    let d = dit.lock().unwrap();
    assert_eq!(d.updates(), [format!("modify {ours}")]);
}

#[test]
fn deleting_a_profile_never_deletes_another_systems_entry() {
    let (dit, addr) = crowded();
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.objects = vec![user("JSMITH", "Jane Smith")];
    let mut r = Runner::new(Kind::Users);
    turn(&mut r, &mut src, T0);
    src.objects.clear();
    let s = turn(&mut r, &mut src, T0 + 1);
    assert_eq!(s.last_error, None);
    assert_eq!(s.published, 0);
    let d = dit.lock().unwrap();
    assert!(
        d.get("cn=Jane Smith+uid=JSMITH,ou=Users,dc=example,dc=com")
            .is_none()
    );
    assert!(
        d.get("cn=Jane Elsewhere,ou=Users,dc=example,dc=com")
            .is_some()
    );
    assert!(d.get("cn=Jane Smith,ou=Users,dc=example,dc=com").is_some());
}

#[test]
fn two_profiles_with_one_text_get_two_names() {
    let (dit, addr) = directory();
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.objects = vec![user("SALES1", "Sales"), user("SALES2", "Sales")];
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    assert_eq!((s.published, s.errors), (2, 0));
    let d = dit.lock().unwrap();
    assert_eq!(
        d.values("cn=Sales,ou=Users,dc=example,dc=com", "uid"),
        ["SALES1"]
    );
    assert_eq!(
        d.values("cn=Sales+uid=SALES2,ou=Users,dc=example,dc=com", "uid"),
        ["SALES2"]
    );
}

#[test]
fn a_target_that_is_down_is_tried_again_with_backoff() {
    let (dit, addr) = directory();
    dit.lock().unwrap().down = true;
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.objects = vec![user("A", "Ann"), user("B", "Ben")];
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    let (when, why) = s.last_error.clone().unwrap();
    assert_eq!(when, T0);
    assert!(!why.is_empty());
    assert_eq!(
        (s.pending, s.next_try, s.connected),
        (2, Some(T0 + 30), false)
    );
    assert_eq!(s.last_success, None);
    let tries = || dit.lock().unwrap().connections;
    assert_eq!(tries(), 1);
    // Not before the interval.
    turn(&mut r, &mut src, T0 + 29);
    assert_eq!(tries(), 1);
    // Then doubled at each failure.
    let s = turn(&mut r, &mut src, T0 + 30);
    assert_eq!(tries(), 2);
    assert_eq!(s.next_try, Some(T0 + 30 + 60));
    let s = turn(&mut r, &mut src, T0 + 90);
    assert_eq!(s.next_try, Some(T0 + 90 + 120));
    // Changes made meanwhile wait in the queue.
    src.objects.push(user("C", "Cy"));
    let s = turn(&mut r, &mut src, T0 + 100);
    assert_eq!(s.pending, 3);
    // The target is back.
    dit.lock().unwrap().down = false;
    let s = turn(&mut r, &mut src, T0 + 210);
    assert_eq!(s.last_success, Some(T0 + 210));
    assert_eq!((s.pending, s.published, s.next_try), (0, 3, None));
    assert_eq!(dit.lock().unwrap().updates().len(), 4);
}

#[test]
fn the_backoff_stops_doubling() {
    let (dit, addr) = directory();
    dit.lock().unwrap().down = true;
    let mut a = agent(Kind::Users, addr, PARENT);
    a.retry = 86_400;
    let mut src = source(&[a]);
    src.objects = vec![user("A", "Ann")];
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    assert_eq!(s.next_try, Some(T0 + 86_400));
    // At most a day, however often it fails.
    let s = turn(&mut r, &mut src, T0 + 86_400);
    assert_eq!(s.next_try, Some(T0 + 2 * 86_400));
}

#[test]
fn a_refused_bind_is_retried() {
    let (dit, addr) = directory();
    dit.lock().unwrap().password = Some(("cn=Administrator".into(), "right".into()));
    let mut a = agent(Kind::Users, addr, PARENT);
    a.bind_dn = Some("cn=Administrator".into());
    let mut src = source(std::slice::from_ref(&a));
    if let Ok(c) = &mut src.config {
        c.passwords = vec![(Kind::Users, "wrong".into())];
    }
    src.objects = vec![user("A", "Ann")];
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    let why = s.last_error.unwrap().1;
    assert!(why.contains("bind") && why.contains("49"), "{why}");
    assert_eq!(s.pending, 1);
    // The password is fixed; the configuration is read again.
    if let Ok(c) = &mut src.config {
        c.passwords = vec![(Kind::Users, "right".into())];
    }
    let s = turn(&mut r, &mut src, T0 + CONFIG_EVERY);
    assert_eq!(s.last_error, None, "{s:?}");
    assert_eq!(s.published, 1);
}

#[test]
fn a_busy_target_keeps_the_queue() {
    let (dit, addr) = directory();
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.objects = vec![user("A", "Ann")];
    let mut r = Runner::new(Kind::Users);
    turn(&mut r, &mut src, T0);
    dit.lock().unwrap().busy = 1;
    src.objects.push(user("B", "Ben"));
    let s = turn(&mut r, &mut src, T0 + 1);
    assert!(s.last_error.unwrap().1.contains("51"));
    assert_eq!((s.pending, s.next_try), (1, Some(T0 + 31)));
    let s = turn(&mut r, &mut src, T0 + 31);
    assert_eq!((s.pending, s.published), (0, 2));
}

#[test]
fn an_entry_the_target_refuses_does_not_hold_up_the_others() {
    let (dit, addr) = directory();
    dit.lock().unwrap().refuse = Some("Bad".into());
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.objects = vec![user("A", "Ann"), user("B", "Bad Name"), user("C", "Cy")];
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    assert_eq!((s.errors, s.published, s.pending), (1, 2, 0));
    let (_, why) = s.last_error.unwrap();
    assert!(why.contains("Bad Name") && why.contains("65"), "{why}");
    // Not a target failure: no retry clock.
    assert_eq!(s.next_try, None);
    assert_eq!(s.last_success, Some(T0));
}

#[test]
fn printers_are_published() {
    let (dit, addr) = directory();
    let parent = "ou=Printers,dc=example,dc=com";
    let mut src = source(&[agent(Kind::Printers, addr, parent)]);
    src.objects = vec![
        printer("PRT01", "Second floor"),
        user("A", "Ann"),
        printer("PRT02", ""),
    ];
    let mut r = Runner::new(Kind::Printers);
    let s = turn(&mut r, &mut src, T0);
    assert_eq!(s.last_error, None);
    assert_eq!(s.published, 2);
    let d = dit.lock().unwrap();
    let p1 = "printer-name=PRT01,ou=Printers,dc=example,dc=com";
    assert_eq!(
        d.values(p1, "objectClass"),
        ["top", "printerService", "printerIPP", "publisher"]
    );
    assert_eq!(d.values(p1, "printer-location"), ["Second floor"]);
    assert_eq!(d.values(p1, "printer-uri"), ["lpd://qs.example.com/PRT01"]);
    assert_eq!(d.values(p1, "printer-make-and-model"), ["*HP4"]);
    let p2 = "printer-name=PRT02,ou=Printers,dc=example,dc=com";
    assert!(d.values(p2, "printer-location").is_empty());
    // Users are not the printer agent's.
    assert!(d.updates().iter().all(|o| !o.contains("Ann")));
    drop(d);
    // The location is cleared: the attribute goes.
    src.objects = vec![printer("PRT01", ""), printer("PRT02", "")];
    turn(&mut r, &mut src, T0 + 1);
    assert!(
        dit.lock()
            .unwrap()
            .values(p1, "printer-location")
            .is_empty()
    );
}

#[test]
fn a_disabled_agent_only_reports() {
    let (dit, addr) = directory();
    let mut a = agent(Kind::Users, addr, PARENT);
    a.enabled = false;
    let mut src = source(&[a]);
    src.objects = vec![user("A", "Ann")];
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    assert!(!s.enabled);
    assert_eq!(dit.lock().unwrap().connections, 0);
    assert_eq!(src.reported.len(), 1);
    assert!(!src.reported[0].1.enabled);
}

#[test]
fn the_status_is_reported_with_each_ask() {
    let (_dit, addr) = directory();
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.objects = vec![user("A", "Ann")];
    let mut r = Runner::new(Kind::Users);
    turn(&mut r, &mut src, T0);
    // The first turn lists; the second asks for changes with the status.
    assert!(src.reported.is_empty());
    turn(&mut r, &mut src, T0 + 1);
    let (k, s) = src.reported.last().unwrap();
    assert_eq!(*k, Kind::Users);
    assert_eq!((s.published, s.last_success), (1, Some(T0)));
}

#[test]
fn a_new_configuration_moves_the_entries() {
    let (dit, addr) = directory();
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.objects = vec![user("A", "Ann")];
    let mut r = Runner::new(Kind::Users);
    turn(&mut r, &mut src, T0);
    let other = "ou=Staff,dc=example,dc=com";
    if let Ok(c) = &mut src.config {
        c.config.agent_mut(Kind::Users).parent_dn = Some(other.into());
    }
    // Read only every CONFIG_EVERY seconds.
    turn(&mut r, &mut src, T0 + 1);
    assert!(
        dit.lock()
            .unwrap()
            .get("cn=Ann,ou=Staff,dc=example,dc=com")
            .is_none()
    );
    let s = turn(&mut r, &mut src, T0 + CONFIG_EVERY);
    assert_eq!(s.last_error, None);
    let d = dit.lock().unwrap();
    assert!(d.get("cn=Ann,ou=Staff,dc=example,dc=com").is_some());
    // The old parent is another publisher's business now: left as it is.
    assert!(d.get("cn=Ann,ou=Users,dc=example,dc=com").is_some());
}

#[test]
fn a_periodic_resync_repairs_the_target() {
    let (dit, addr) = directory();
    let mut a = agent(Kind::Users, addr, PARENT);
    a.resync = 3600;
    let mut src = source(&[a]);
    src.objects = vec![user("A", "Ann")];
    let mut r = Runner::new(Kind::Users);
    turn(&mut r, &mut src, T0);
    // Someone deletes our entry behind our back.
    dit.lock()
        .unwrap()
        .entries
        .remove(&key("cn=Ann,ou=Users,dc=example,dc=com"));
    turn(&mut r, &mut src, T0 + 100);
    assert!(
        dit.lock()
            .unwrap()
            .get("cn=Ann,ou=Users,dc=example,dc=com")
            .is_none()
    );
    turn(&mut r, &mut src, T0 + 3600);
    assert!(
        dit.lock()
            .unwrap()
            .get("cn=Ann,ou=Users,dc=example,dc=com")
            .is_some()
    );
}

#[test]
fn a_source_that_fails_is_reported() {
    let (_dit, addr) = directory();
    let mut src = source(&[agent(Kind::Users, addr, PARENT)]);
    src.fail = Some("the service is ending".into());
    let mut r = Runner::new(Kind::Users);
    let s = turn(&mut r, &mut src, T0);
    assert!(s.last_error.unwrap().1.contains("the service is ending"));
    src.fail = None;
    src.objects = vec![user("A", "Ann")];
    let s = turn(&mut r, &mut src, T0 + 1);
    assert_eq!(s.published, 1);
}
