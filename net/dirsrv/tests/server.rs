// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The instance job's connection loop (`dirsrv::server`) on the
//! loopback against a fake storage service: a directory in memory on a
//! thread of its own, answering the eight requests as the service does.
//! Binds and operations, streamed searches, abandon, paging, persistent
//! searches, `StartTLS` with and without a certificate, the secure
//! port, the idle time-out, a bad message, the end's notice and the
//! counts.

#![allow(clippy::too_many_lines)]

mod harness;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpStream};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use certs::ca;
use certs::crypto::kdf::Argon2Params;
use certs::crypto::keys::KeyAlgorithm;
use certs::files::MemFiles;
use certs::paths::{LOCAL_CA_STORE, SYSTEM_STORE};
use certs::store::{CertStore, StoreKind};
use certs::x509::build::{CertSpec, Subject};
use dirsrv::ops::{Budget as OpBudget, Conn, Cursors, PageState, Reply, Session, Started};
use dirsrv::server::{
    Answer, Budget, Changes, Close, ConnInfo, Counts, JobSettings, Links, Listeners, Opened,
    Server, Service, State, SvcError, SvcResult,
};
use harness::{T, text};
use ldap::ResultCode;
use ldap::ber::frame_len;
use ldap::client::{Client, ClientError, Options};
use ldap::control::{ControlValue, EntryChange, PagedResults, PersistentSearch, change_type};
use ldap::msg::{LdapMessage, ProtocolOp, Scope};
use tls::config::{ClientAuthMode, Identity, ServerAuthMode, client_config, server_config};
use tls::policy::TlsPolicy;
use tls::rustls::{ClientConfig, ServerConfig};

const HOST: &str = "qs400.test";

/// One session in the fake service.
#[derive(Default)]
struct Sess {
    session: Session,
    cursors: Cursors,
    /// Cursor to the message ID of its search.
    ids: HashMap<u64, i32>,
    /// Persistent searches by message ID.
    watches: HashMap<i32, u64>,
}

/// The fake service's state.
struct Fake {
    t: T,
    next: u64,
    sessions: HashMap<u64, Sess>,
    settings: JobSettings,
    /// Entries per answer at most (the job's budget is larger).
    cap: u32,
    /// A pause in each `next`.
    delay: Duration,
    counts: Arc<Mutex<Vec<Counts>>>,
}

fn encode(id: i32, r: Reply) -> Vec<u8> {
    LdapMessage {
        id,
        op: r.op,
        controls: r.controls,
    }
    .encode()
}

impl Fake {
    fn sess(&mut self, s: u64) -> SvcResult<&mut Sess> {
        self.sessions
            .get_mut(&s)
            .ok_or_else(|| SvcError::new("CPF9898", "no such session"))
    }

    fn page(&mut self, s: u64, cursor: u64, budget: Budget) -> SvcResult<Answer> {
        std::thread::sleep(self.delay);
        let cap = self.cap;
        let mut txn = self.t.sys.store.begin().unwrap();
        let sess = self
            .sessions
            .get_mut(&s)
            .ok_or_else(|| SvcError::new("CPF9898", "no such session"))?;
        let id = *sess.ids.get(&cursor).unwrap_or(&0);
        let p = self.t.dir.next(
            &mut txn,
            &mut sess.cursors,
            cursor,
            OpBudget {
                entries: budget.entries.min(cap) as usize,
                bytes: budget.bytes as usize,
            },
        );
        txn.commit().unwrap();
        let state = match p.state {
            PageState::More => State::More(cursor),
            PageState::Paused => State::Paused(cursor),
            PageState::Done => {
                sess.ids.remove(&cursor);
                State::Done
            }
            PageState::Watching(w) => {
                sess.ids.remove(&cursor);
                sess.watches.insert(id, w);
                State::Watching(w)
            }
        };
        Ok(Answer {
            replies: p.replies.into_iter().map(|r| encode(id, r)).collect(),
            state,
            ..Answer::default()
        })
    }
}

impl Service for Fake {
    fn open(&mut self, _: &str, _: &ConnInfo) -> SvcResult<Opened> {
        self.next += 1;
        self.sessions.insert(self.next, Sess::default());
        Ok(Opened {
            session: self.next,
            settings: self.settings.clone(),
        })
    }

    fn bind(&mut self, s: u64, msg: &[u8], conn: &ConnInfo) -> SvcResult<Answer> {
        let m = LdapMessage::decode(msg).unwrap();
        let mut txn = self.t.sys.store.begin().unwrap();
        let o = self.t.dir.bind(
            &mut txn,
            &m,
            &Conn {
                tls: conn.tls,
                client_subject: conn.client_subject.clone(),
            },
        );
        txn.commit().unwrap();
        let sess = self.sess(s)?;
        if let Some(n) = o.session {
            sess.session = n;
        }
        Ok(Answer {
            replies: o.replies.into_iter().map(|r| encode(m.id, r)).collect(),
            bound: sess.session.dn.clone(),
            ..Answer::default()
        })
    }

    fn op(&mut self, s: u64, msg: &[u8]) -> SvcResult<Answer> {
        let m = LdapMessage::decode(msg).unwrap();
        let session = self.sess(s)?.session.clone();
        let mut txn = self.t.sys.store.begin().unwrap();
        let o = self.t.dir.op(&mut txn, &session, &m);
        if o.succeeded() {
            txn.commit().unwrap();
            let mut txn = self.t.sys.store.begin().unwrap();
            self.t.dir.publish(&mut txn, &o.changes);
            txn.commit().unwrap();
        } else {
            txn.abort();
        }
        Ok(Answer {
            replies: o.replies.into_iter().map(|r| encode(m.id, r)).collect(),
            start_tls: o.start_tls,
            ..Answer::default()
        })
    }

    fn search(&mut self, s: u64, msg: &[u8], budget: Budget) -> SvcResult<Answer> {
        let m = LdapMessage::decode(msg).unwrap();
        let mut txn = self.t.sys.store.begin().unwrap();
        let sess = self
            .sessions
            .get_mut(&s)
            .ok_or_else(|| SvcError::new("CPF9898", "no such session"))?;
        let started = self
            .t
            .dir
            .search(&mut txn, &sess.session.clone(), &m, &mut sess.cursors);
        txn.commit().unwrap();
        match started {
            Started::Done(v) => Ok(Answer {
                replies: v.into_iter().map(|r| encode(m.id, r)).collect(),
                ..Answer::default()
            }),
            Started::Cursor(c) => {
                sess.ids.insert(c, m.id);
                self.page(s, c, budget)
            }
        }
    }

    fn next(&mut self, s: u64, cursor: u64, budget: Budget) -> SvcResult<Answer> {
        self.page(s, cursor, budget)
    }

    fn close(&mut self, s: u64, what: Close) -> SvcResult<()> {
        match what {
            Close::Session => {
                if let Some(sess) = self.sessions.remove(&s) {
                    for w in sess.watches.into_values() {
                        self.t.dir.close_watch(w);
                    }
                }
            }
            Close::Message(id) => {
                let sess = self.sess(s)?;
                let w = sess.watches.remove(&id);
                let cs: Vec<u64> = sess
                    .ids
                    .iter()
                    .filter(|(_, m)| **m == id)
                    .map(|(c, _)| *c)
                    .collect();
                for c in cs {
                    sess.ids.remove(&c);
                    sess.cursors.close(c);
                }
                if let Some(w) = w {
                    self.t.dir.close_watch(w);
                }
            }
        }
        Ok(())
    }

    fn conns(&mut self, _: &str, counts: Counts) -> SvcResult<()> {
        self.counts.lock().unwrap().push(counts);
        Ok(())
    }

    fn changes(&mut self, s: u64, max: u32) -> SvcResult<Changes> {
        let watches: Vec<(i32, u64)> = self
            .sess(s)?
            .watches
            .iter()
            .map(|(a, b)| (*a, *b))
            .collect();
        let mut out = Changes::default();
        for (id, w) in watches {
            match self.t.dir.changes(w, max as usize) {
                Some(v) => out.replies.extend(v.into_iter().map(|r| encode(id, r))),
                None => out.ended.push(id),
            }
        }
        Ok(out)
    }
}

type Call = Box<dyn FnOnce(&mut Fake) + Send>;

/// The fake service on its own thread.
struct FakeLinks {
    tx: Mutex<Sender<Call>>,
}

/// One thread's link: every request runs on the service's thread.
struct FakeLink {
    tx: Sender<Call>,
}

impl FakeLink {
    fn call<R: Send + 'static>(&self, f: impl FnOnce(&mut Fake) -> R + Send + 'static) -> R {
        let (tx, rx) = mpsc::channel();
        self.tx
            .send(Box::new(move |fake| {
                let _ = tx.send(f(fake));
            }))
            .unwrap();
        rx.recv().unwrap()
    }
}

impl Service for FakeLink {
    fn open(&mut self, i: &str, c: &ConnInfo) -> SvcResult<Opened> {
        let (i, c) = (i.to_string(), c.clone());
        self.call(move |f| f.open(&i, &c))
    }
    fn bind(&mut self, s: u64, m: &[u8], c: &ConnInfo) -> SvcResult<Answer> {
        let (m, c) = (m.to_vec(), c.clone());
        self.call(move |f| f.bind(s, &m, &c))
    }
    fn op(&mut self, s: u64, m: &[u8]) -> SvcResult<Answer> {
        let m = m.to_vec();
        self.call(move |f| f.op(s, &m))
    }
    fn search(&mut self, s: u64, m: &[u8], b: Budget) -> SvcResult<Answer> {
        let m = m.to_vec();
        self.call(move |f| f.search(s, &m, b))
    }
    fn next(&mut self, s: u64, c: u64, b: Budget) -> SvcResult<Answer> {
        self.call(move |f| f.next(s, c, b))
    }
    fn close(&mut self, s: u64, w: Close) -> SvcResult<()> {
        self.call(move |f| f.close(s, w))
    }
    fn conns(&mut self, i: &str, c: Counts) -> SvcResult<()> {
        let i = i.to_string();
        self.call(move |f| f.conns(&i, c))
    }
    fn changes(&mut self, s: u64, m: u32) -> SvcResult<Changes> {
        self.call(move |f| f.changes(s, m))
    }
}

impl Links for FakeLinks {
    type S = FakeLink;
    fn link(&self) -> Result<FakeLink, String> {
        Ok(FakeLink {
            tx: self.tx.lock().unwrap().clone(),
        })
    }
}

/// What a test sets up.
struct Setup {
    settings: JobSettings,
    persons: usize,
    cap: u32,
    delay: Duration,
    tls: Option<Arc<ServerConfig>>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            settings: JobSettings {
                port: 0,
                secure_port: 0,
                ..JobSettings::default()
            },
            persons: 3,
            cap: 1000,
            delay: Duration::ZERO,
            tls: None,
        }
    }
}

struct Running {
    server: Option<Server>,
    port: u16,
    secure: Option<u16>,
    counts: Arc<Mutex<Vec<Counts>>>,
}

impl Running {
    fn client(&self) -> Client {
        let s = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        Client::from_tcp(s, HOST, opts())
    }

    fn admin(&self) -> Client {
        let mut c = self.client();
        assert!(
            c.simple_bind("cn=Administrator", b"secret")
                .unwrap()
                .result
                .is_success()
        );
        c
    }

    fn stop(&mut self) {
        if let Some(s) = self.server.take() {
            s.stop(Duration::from_secs(5));
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}

fn opts() -> Options {
    Options {
        timeout: Some(Duration::from_secs(20)),
        ..Options::default()
    }
}

fn start(setup: Setup) -> Running {
    let (tx, rx) = mpsc::channel::<Call>();
    let counts = Arc::new(Mutex::new(Vec::new()));
    let c2 = Arc::clone(&counts);
    let settings = setup.settings.clone();
    let (persons, cap, delay) = (setup.persons, setup.cap, setup.delay);
    std::thread::spawn(move || {
        let mut t = T::new();
        for i in 0..persons {
            t.person(&format!("p{i:04}"), "pw");
        }
        let mut fake = Fake {
            t,
            next: 0,
            sessions: HashMap::new(),
            settings,
            cap,
            delay,
            counts: c2,
        };
        while let Ok(call) = rx.recv() {
            call(&mut fake);
        }
    });
    let links = Arc::new(FakeLinks { tx: Mutex::new(tx) });
    let (l, failed) =
        Listeners::bind(IpAddr::V4(Ipv4Addr::LOCALHOST), &setup.settings, setup.tls).unwrap();
    assert!(failed.is_none());
    let (port, secure) = l.ports();
    let server = Server::start(links, "QUSRDIR", l);
    Running {
        server: Some(server),
        port,
        secure,
        counts,
    }
}

fn search(c: &mut Client, base: &str, filter: &str) -> ldap::client::SearchResult {
    let req = c.search_request(base, Scope::Sub, filter, &["cn"]).unwrap();
    c.search(&req, &[]).unwrap()
}

#[test]
fn binds_operations_and_streamed_searches() {
    let mut r = start(Setup {
        persons: 250,
        cap: 7,
        ..Setup::default()
    });
    let mut c = r.admin();
    let got = search(&mut c, "o=Acme", "(objectClass=inetOrgPerson)");
    assert!(got.result.is_success(), "{:?}", got.result);
    assert_eq!(got.entries.len(), 250);
    let a = c
        .add(
            "ou=Sales,o=Acme",
            vec![
                text("objectClass", &["organizationalUnit"]),
                text("ou", &["Sales"]),
            ],
            &[],
        )
        .unwrap();
    assert!(a.result.is_success(), "{:?}", a.result);
    assert_eq!(
        c.delete("ou=Nope,o=Acme", &[]).unwrap().result.code,
        ResultCode::NoSuchObject
    );
    assert_eq!(c.who_am_i().unwrap(), "dn:cn=Administrator");
    let bad = c.simple_bind("cn=Administrator", b"wrong").unwrap();
    assert_eq!(bad.result.code, ResultCode::InvalidCredentials);
    c.unbind().unwrap();
    // The counts reach the service.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let last = r.counts.lock().unwrap().last().copied();
        if last.is_some_and(|c| c.total >= 1 && c.current == 0) {
            break;
        }
        assert!(Instant::now() < deadline, "{last:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
    r.stop();
}

#[test]
fn paged_results_across_requests() {
    let r = start(Setup {
        persons: 12,
        ..Setup::default()
    });
    let mut c = r.admin();
    let req = c
        .search_request("o=Acme", Scope::Sub, "(objectClass=inetOrgPerson)", &["cn"])
        .unwrap();
    let mut cookie = Vec::new();
    let mut n = 0;
    loop {
        let ctl = PagedResults {
            size: 5,
            cookie: cookie.clone(),
        }
        .to_control(false);
        let got = c.search(&req, &[ctl]).unwrap();
        assert!(got.result.is_success());
        n += got.entries.len();
        let p = PagedResults::find(&got.controls).unwrap().unwrap();
        if p.cookie.is_empty() {
            break;
        }
        cookie = p.cookie;
    }
    assert_eq!(n, 12);
}

/// Every message of the stream up to the one with `stop_id`.
fn read_until(s: &mut TcpStream, stop_id: i32) -> Vec<LdapMessage> {
    let mut buf = Vec::new();
    let mut out = Vec::new();
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    loop {
        while let Some(n) = frame_len(&buf, 1 << 24).unwrap() {
            let m = LdapMessage::decode(&buf[..n]).unwrap();
            buf.drain(..n);
            let done = m.id == stop_id;
            out.push(m);
            if done {
                return out;
            }
        }
        let mut tmp = [0u8; 8192];
        let k = s.read(&mut tmp).unwrap();
        assert!(k > 0, "closed after {} messages", out.len());
        buf.extend_from_slice(&tmp[..k]);
    }
}

#[test]
fn abandon_stops_a_streamed_search() {
    let r = start(Setup {
        persons: 300,
        cap: 5,
        delay: Duration::from_millis(5),
        ..Setup::default()
    });
    let mut s = TcpStream::connect(("127.0.0.1", r.port)).unwrap();
    let bind = LdapMessage::new(
        1,
        ProtocolOp::BindRequest {
            version: 3,
            name: "cn=Administrator".into(),
            auth: ldap::msg::BindAuth::Simple(b"secret".to_vec()),
        },
    );
    s.write_all(&bind.encode()).unwrap();
    read_until(&mut s, 1);
    let req = ldap::msg::SearchRequest::new(
        "o=Acme".into(),
        Scope::Sub,
        ldap::Filter::parse("(objectClass=inetOrgPerson)").unwrap(),
    );
    let mut b = LdapMessage::new(2, ProtocolOp::SearchRequest(req)).encode();
    b.extend(LdapMessage::new(3, ProtocolOp::AbandonRequest(2)).encode());
    b.extend(LdapMessage::new(4, ldap::ext::who_am_i()).encode());
    s.write_all(&b).unwrap();
    let got = read_until(&mut s, 4);
    let entries = got
        .iter()
        .filter(|m| m.id == 2 && matches!(m.op, ProtocolOp::SearchResultEntry { .. }))
        .count();
    assert!(entries < 300, "{entries}");
    assert!(
        !got.iter()
            .any(|m| matches!(m.op, ProtocolOp::SearchResultDone(_))),
        "an abandoned search has no done"
    );
}

#[test]
fn persistent_search_delivers_changes() {
    let r = start(Setup::default());
    let mut watcher = r.admin();
    let req = watcher
        .search_request(
            "o=Acme",
            Scope::Sub,
            "(objectClass=organizationalUnit)",
            &["ou"],
        )
        .unwrap();
    let ps = PersistentSearch {
        change_types: change_type::ALL,
        changes_only: true,
        return_ecs: true,
    };
    let id = watcher
        .send(ProtocolOp::SearchRequest(req), vec![ps.to_control(true)])
        .unwrap();
    // The watch is set once the initial (empty) results are over.
    std::thread::sleep(Duration::from_millis(300));
    let mut writer = r.admin();
    for ou in ["One", "Two"] {
        let a = writer
            .add(
                &format!("ou={ou},o=Acme"),
                vec![
                    text("objectClass", &["organizationalUnit"]),
                    text("ou", &[ou]),
                ],
                &[],
            )
            .unwrap();
        assert!(a.result.is_success());
    }
    let mut seen = Vec::new();
    while seen.len() < 2 {
        let m = watcher
            .poll(Some(id), Some(Duration::from_secs(10)))
            .unwrap()
            .expect("a change");
        let ProtocolOp::SearchResultEntry { dn, .. } = &m.op else {
            panic!("{m:?}")
        };
        let ec = EntryChange::find(&m.controls).unwrap().unwrap();
        assert_eq!(ec.change_type, change_type::ADD);
        seen.push(dn.clone());
    }
    assert_eq!(seen, ["ou=One,o=Acme", "ou=Two,o=Acme"]);
    // Watching is not idle; abandon ends it.
    watcher.abandon(id).unwrap();
    assert_eq!(watcher.who_am_i().unwrap(), "dn:cn=Administrator");
}

#[test]
fn start_tls_without_a_certificate_is_protocol_error() {
    let r = start(Setup::default());
    let mut c = r.client();
    let (_, ccfg) = tls_pair();
    match c.start_tls(ccfg) {
        Err(ClientError::Result(res)) => assert_eq!(res.code, ResultCode::ProtocolError),
        other => panic!("{other:?}"),
    }
    // The connection goes on in the clear.
    assert!(
        c.simple_bind("cn=Administrator", b"secret")
            .unwrap()
            .result
            .is_success()
    );
}

#[test]
fn start_tls_and_the_secure_port() {
    let (scfg, ccfg) = tls_pair();
    // ibm-slapdSecurity none: a certificate changes nothing.
    let r = start(Setup {
        tls: Some(Arc::clone(&scfg)),
        ..Setup::default()
    });
    assert_eq!(r.secure, None);
    match r.client().start_tls(Arc::clone(&ccfg)) {
        Err(ClientError::Result(res)) => assert_eq!(res.code, ResultCode::ProtocolError),
        other => panic!("{other:?}"),
    }
    drop(r);
    let mut setup = Setup {
        tls: Some(scfg),
        ..Setup::default()
    };
    setup.settings.security = "SSLTLS".into();
    let r = start(setup);
    let mut c = r.client();
    c.start_tls(Arc::clone(&ccfg)).unwrap();
    assert!(c.is_tls());
    assert!(
        c.simple_bind("cn=Administrator", b"secret")
            .unwrap()
            .result
            .is_success()
    );
    match c.start_tls(Arc::clone(&ccfg)) {
        Err(ClientError::Unsupported(_)) => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(search(&mut c, "o=Acme", "(cn=p0001)").entries.len(), 1);
    c.unbind().unwrap();
    let s = TcpStream::connect(("127.0.0.1", r.secure.unwrap())).unwrap();
    let mut c = Client::tls_over(s, HOST, ccfg, opts()).unwrap();
    assert!(
        c.simple_bind("cn=Administrator", b"secret")
            .unwrap()
            .result
            .is_success()
    );
    assert_eq!(search(&mut c, "o=Acme", "(cn=p0002)").entries.len(), 1);
}

#[test]
fn idle_connections_end() {
    let mut setup = Setup::default();
    setup.settings.idle_timeout = 1;
    let r = start(setup);
    let mut c = r.client();
    std::thread::sleep(Duration::from_millis(1800));
    assert!(c.who_am_i().is_err(), "an idle connection is closed");
}

#[test]
fn a_bad_message_gets_a_notice() {
    let r = start(Setup::default());
    let mut s = TcpStream::connect(("127.0.0.1", r.port)).unwrap();
    // A SEQUENCE holding an INTEGER and nothing else.
    s.write_all(&[0x30, 0x03, 0x02, 0x01, 0x01]).unwrap();
    let got = read_until(&mut s, 0);
    let n = ldap::ext::as_notice_of_disconnection(got.last().unwrap()).unwrap();
    assert_eq!(n.code, ResultCode::ProtocolError);
}

#[test]
fn the_end_sends_a_notice() {
    let mut r = start(Setup::default());
    let mut c = r.admin();
    r.stop();
    match c.poll(None, Some(Duration::from_secs(5))) {
        Err(ClientError::Disconnected(res)) => assert_eq!(res.code, ResultCode::Unavailable),
        other => panic!("{other:?}"),
    }
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

/// Through the loop, an anonymous subtree search meets a referral
/// object as a continuation reference.
#[test]
fn continuation_references_reach_the_client() {
    let r = start(Setup::default());
    let mut c = r.admin();
    for (dn, attrs) in [
        (
            "ou=Sb,o=Acme",
            vec![
                text("objectClass", &["organizationalUnit"]),
                text("ou", &["Sb"]),
            ],
        ),
        (
            "ou=ref,ou=Sb,o=Acme",
            vec![
                text("objectClass", &["top", "referral", "extensibleObject"]),
                text("ou", &["ref"]),
                text("ref", &["ldap://ldaptest.invalid/ou=far,dc=example"]),
            ],
        ),
    ] {
        let a = c.add(dn, attrs, &[]).unwrap();
        assert!(a.result.is_success(), "{dn}: {:?}", a.result);
    }
    let mut anon = r.client();
    let req = anon
        .search_request("ou=Sb,o=Acme", Scope::Sub, "(objectClass=*)", &["1.1"])
        .unwrap();
    let got = anon.search(&req, &[]).unwrap();
    assert!(got.result.is_success(), "{:?}", got.result);
    assert_eq!(
        got.references,
        [vec!["ldap://ldaptest.invalid/ou=far,dc=example".to_string()]],
        "{got:?}"
    );
}

/// Not a test: the connection loop over a directory in memory for
/// `tools/ldaptest` to run against (`--suffix o=Acme`, the
/// administrator's password `secret`). The port goes to
/// `LDAPTEST_SERVE_FILE`; it serves for `LDAPTEST_SERVE_SECS` (600).
#[test]
#[ignore = "a server for ldaptest, run by hand"]
fn serve_for_ldaptest() {
    let r = start(Setup::default());
    let file = std::env::var("LDAPTEST_SERVE_FILE").unwrap_or_else(|_| "ldaptest-port.txt".into());
    std::fs::write(&file, r.port.to_string()).unwrap();
    let secs = std::env::var("LDAPTEST_SERVE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    std::thread::sleep(Duration::from_secs(secs));
    drop(r);
}
