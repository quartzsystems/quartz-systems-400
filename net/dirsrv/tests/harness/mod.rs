// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A directory over a store in memory, with `o=Acme` as the user
//! suffix, a fixed clock and the administrator's password `secret`.

#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dirsrv::ops::{Budget, Conn, Cursors, Directory, Env, Outcome, PageState, Reply, Session, Settings};
use dirsrv::pwd::Scheme;
use dirsrv::schema::Schema;
use ldap::control::Control;
use ldap::filter::Filter;
use ldap::msg::{Attribute, BindAuth, DerefAliases, LdapMessage, LdapResult, ProtocolOp, Scope, SearchRequest};
use ldap::{Dn, ResultCode};

#[path = "../common/mod.rs"]
mod common;

/// 2026-11-13 12:00:00 UTC.
pub const T0: u64 = 1_794_571_200_000_000;

pub struct Clock(pub Arc<AtomicU64>, pub AtomicU64);

impl Env for Clock {
    fn now_micros(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    fn random(&self, buf: &mut [u8]) {
        let mut x = self.1.fetch_add(0x9e37_79b9, Ordering::SeqCst) | 1;
        for b in buf.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x.to_le_bytes()[0];
        }
    }
}

pub struct T {
    pub sys: common::Sys,
    pub dir: Directory,
    pub clock: Arc<AtomicU64>,
    pub admin: Session,
    pub cursors: Cursors,
}

pub fn settings() -> Settings {
    Settings {
        admin_pw: Some(Scheme::Ssha512.hash(b"secret", b"saltsalt")),
        suffixes: vec!["o=Acme".into()],
        server_id: "test-server".into(),
        ..Settings::default()
    }
}

pub fn text(ty: &str, vals: &[&str]) -> Attribute {
    Attribute::new(ty, vals)
}

impl T {
    pub fn new() -> Self {
        Self::with(settings())
    }

    pub fn with(s: Settings) -> Self {
        let mut sys = common::system(16384);
        let clock = Arc::new(AtomicU64::new(T0));
        let schema = Schema::shipped().unwrap();
        let admin_dn = Dn::parse(&s.admin_dn).unwrap();
        let admin = Session::admin(&schema, &admin_dn);
        let mut t = sys.store.begin().unwrap();
        let mut dir = Directory::new(&mut t, sys.db, schema, s, Box::new(Clock(clock.clone(), AtomicU64::new(7)))).unwrap();
        dir.seed(&mut t).unwrap();
        t.commit().unwrap();
        let mut me = Self {
            sys,
            dir,
            clock,
            admin,
            cursors: Cursors::default(),
        };
        let a = me.admin.clone();
        let ro = std::mem::replace(&mut me.dir.settings.read_only, false);
        me.add_ok(&a, "o=Acme", &[text("objectClass", &["organization"]), text("o", &["Acme"])]);
        me.dir.settings.read_only = ro;
        me
    }

    pub fn advance(&self, secs: u64) {
        self.clock.fetch_add(secs * 1_000_000, Ordering::SeqCst);
    }

    pub fn msg(op: ProtocolOp, controls: Vec<Control>) -> LdapMessage {
        LdapMessage {
            id: 1,
            op,
            controls,
        }
    }

    /// One operation in its own transaction, committed when it succeeds;
    /// published to the watches.
    pub fn run(&mut self, s: &Session, op: ProtocolOp, controls: Vec<Control>) -> Outcome {
        let msg = Self::msg(op, controls);
        let mut t = self.sys.store.begin().unwrap();
        let o = self.dir.op(&mut t, s, &msg);
        if o.succeeded() {
            t.commit().unwrap();
            let mut t = self.sys.store.begin().unwrap();
            self.dir.publish(&mut t, &o.changes);
            t.commit().unwrap();
        } else {
            t.abort();
        }
        o
    }

    pub fn code(o: &Outcome) -> ResultCode {
        o.replies[0].op.result().unwrap().code
    }

    pub fn add(&mut self, s: &Session, dn: &str, attrs: &[Attribute]) -> Outcome {
        self.run(
            s,
            ProtocolOp::AddRequest {
                dn: dn.into(),
                attrs: attrs.to_vec(),
            },
            Vec::new(),
        )
    }

    pub fn add_ok(&mut self, s: &Session, dn: &str, attrs: &[Attribute]) {
        let o = self.add(s, dn, attrs);
        assert_eq!(Self::code(&o), ResultCode::Success, "add {dn}: {:?}", o.replies[0].op);
    }

    /// A person under `o=Acme` with a password.
    pub fn person(&mut self, cn: &str, pw: &str) {
        let a = self.admin.clone();
        self.add_ok(
            &a,
            &format!("cn={cn},o=Acme"),
            &[
                text("objectClass", &["inetOrgPerson"]),
                text("cn", &[cn]),
                text("sn", &[&format!("S{cn}")]),
                text("userPassword", &[pw]),
            ],
        );
    }

    pub fn bind_msg(&mut self, msg: &LdapMessage, conn: &Conn) -> Outcome {
        let mut t = self.sys.store.begin().unwrap();
        let o = self.dir.bind(&mut t, msg, conn);
        t.commit().unwrap();
        o
    }

    /// A simple bind; the session it gives and the result.
    pub fn bind(&mut self, dn: &str, pw: &str) -> (Session, LdapResult, Vec<Control>) {
        let msg = Self::msg(
            ProtocolOp::BindRequest {
                version: 3,
                name: dn.into(),
                auth: BindAuth::Simple(pw.as_bytes().to_vec()),
            },
            vec![Control::new(ldap::control::PASSWORD_POLICY, false, None)],
        );
        let o = self.bind_msg(&msg, &Conn::default());
        let r = o.replies[0].op.result().unwrap().clone();
        (o.session.unwrap(), r, o.replies[0].controls.clone())
    }

    pub fn req(base: &str, scope: Scope, filter: &str, attrs: &[&str]) -> SearchRequest {
        SearchRequest {
            base: base.into(),
            scope,
            deref: DerefAliases::Always,
            size_limit: 0,
            time_limit: 0,
            types_only: false,
            filter: Filter::parse(filter).unwrap(),
            attrs: attrs.iter().map(|a| (*a).to_string()).collect(),
        }
    }

    /// A whole search (every page of a cursor until done or paused).
    pub fn search_req(&mut self, s: &Session, req: SearchRequest, controls: Vec<Control>) -> Vec<Reply> {
        let msg = Self::msg(ProtocolOp::SearchRequest(req), controls);
        let mut t = self.sys.store.begin().unwrap();
        let started = self.dir.search(&mut t, s, &msg, &mut self.cursors);
        let mut out = Vec::new();
        match started {
            dirsrv::ops::Started::Done(v) => out = v,
            dirsrv::ops::Started::Cursor(id) => loop {
                let p = self.dir.next(&mut t, &mut self.cursors, id, Budget { entries: 2, bytes: 1 << 20 });
                out.extend(p.replies);
                if p.state != PageState::More {
                    break;
                }
            },
        }
        t.commit().unwrap();
        out
    }

    pub fn search(&mut self, s: &Session, base: &str, scope: Scope, filter: &str, attrs: &[&str]) -> Vec<Reply> {
        self.search_req(s, Self::req(base, scope, filter, attrs), Vec::new())
    }
}

/// The DNs of a search's entries.
pub fn dns(r: &[Reply]) -> Vec<String> {
    r.iter()
        .filter_map(|x| match &x.op {
            ProtocolOp::SearchResultEntry { dn, .. } => Some(dn.clone()),
            _ => None,
        })
        .collect()
}

/// A search's final result.
pub fn done(r: &[Reply]) -> LdapResult {
    match &r.last().unwrap().op {
        ProtocolOp::SearchResultDone(x) => x.clone(),
        o => panic!("no done: {o:?}"),
    }
}

/// The attribute names (lower-cased, sorted) of the first entry.
pub fn names(r: &[Reply]) -> Vec<String> {
    for x in r {
        if let ProtocolOp::SearchResultEntry { attrs, .. } = &x.op {
            let mut v: Vec<String> = attrs.iter().map(|a| a.ty.to_ascii_lowercase()).collect();
            v.sort();
            return v;
        }
    }
    Vec::new()
}

/// The values of `ty` in the first entry, as text.
pub fn vals(r: &[Reply], ty: &str) -> Vec<String> {
    for x in r {
        if let ProtocolOp::SearchResultEntry { attrs, .. } = &x.op {
            return attrs
                .iter()
                .filter(|a| a.ty.eq_ignore_ascii_case(ty))
                .flat_map(|a| a.vals.iter().map(|v| String::from_utf8_lossy(v).to_string()))
                .collect();
        }
    }
    Vec::new()
}
