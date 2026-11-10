// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's NetServer: the APIs themselves
//! (`netsvrcfg::api`) over a host in memory, so the pages meet the
//! same checks and messages as on the system. Four shares, three
//! sessions, one user disabled; the server active.

#![allow(clippy::doc_markdown)] // NetServer

use std::sync::Mutex;

use netsvrcfg::api::{ApiError, MemHost, ServerControl};
use netsvrcfg::client::{self, Failure};
use netsvrcfg::fmt::text::{bin4, bin8, text_at};
use netsvrcfg::{
    Attributes, Config, Kind, Permission, Qualified, Session, Settings, Share, SpoolType,
};
use qsweb::msg::{Kind as MsgKind, Message};

use super::{has_special, now, text};
use crate::env::netsvr::{Disabled, NetServer, Status};

/// The server control: start and end flip it.
#[derive(Debug)]
pub struct Server {
    /// `QZLSSERVER` is active.
    pub active: bool,
}

impl ServerControl for Server {
    fn active(&mut self) -> bool {
        self.active
    }

    fn start(&mut self, _reset: bool) -> Result<(), ApiError> {
        self.active = true;
        Ok(())
    }

    fn end(&mut self) -> Result<(), ApiError> {
        self.active = false;
        Ok(())
    }

    fn end_session(&mut self, _job: &str, _id: u64) -> Result<(), ApiError> {
        Ok(())
    }
}

/// The sample NetServer.
pub struct FixtureNetServer {
    /// The APIs' host.
    pub host: Mutex<MemHost<Server>>,
    /// When each disabled user was disabled.
    pub since: Vec<(String, u64)>,
    /// The APIs called, by name, in order.
    pub called: Mutex<Vec<String>>,
}

fn share(name: &str, path: &str, text: &str, f: impl FnOnce(&mut Share)) -> Share {
    let mut s = Share::file(name, path, text);
    f(&mut s);
    s
}

fn config() -> Config {
    let mut c = Config::shipped("QSHQ400");
    c.attrs.current.domain = "QSHDOM".into();
    c.attrs.current.text = "Quartz Systems head office".into();
    c.attrs.current.guest = "QSHGUEST".into();
    c.attrs.pending = c.attrs.current.clone();
    c.attrs.pending.idle_timeout = 3600;
    c.attrs.pending.min_dialect = netsvrcfg::Dialect::Smb30;
    let shares = [
        share("HOME", "/home", "Home directories", |_| {}),
        share("DOCS", "/home/shared/docs", "Department documents", |s| {
            if let Kind::File(f) = &mut s.kind {
                f.permission = Permission::ReadOnly;
                f.max_users = 20;
                f.text_conversion = true;
                f.extensions = vec!["TXT".into(), "CSV".into()];
            }
            s.autl = "NSUSERS".into();
        }),
        {
            let mut p = Share::print(
                "PRT01",
                Qualified::new("PRT01", "QUSRSYS"),
                "Second floor laser",
            );
            if let Kind::Print(ps) = &mut p.kind {
                ps.driver = "Generic PCL 6".into();
                ps.spool_type = SpoolType::UserAscii;
            }
            p
        },
    ];
    for s in shares {
        c.shares.insert(s.name.clone(), s);
    }
    c
}

fn session(ws: &str, expanded: &str, user: &str, job: &str, since: u64) -> Session {
    Session {
        id: 0,
        workstation: ws.into(),
        expanded: expanded.into(),
        user: user.into(),
        job: job.into(),
        guest: user == "QSHGUEST",
        since,
        last_active: since,
        connections: Vec::new(),
        ending: false,
    }
}

impl Default for FixtureNetServer {
    #[allow(clippy::many_single_char_names)]
    fn default() -> Self {
        let t = now();
        let mut h = MemHost::with_server("QSHQ400", Server { active: true });
        h.clock = t;
        h.config = Some(config());
        h.paths.extend([
            ("/home".into(), "QSYS".into()),
            ("/home/shared/docs".into(), "QSECOFR".into()),
            ("/home/QPGMR".into(), "QPGMR".into()),
        ]);
        h.outqs.extend([
            (Qualified::new("PRT01", "QUSRSYS"), "QSYS".to_string()),
            (Qualified::new("QPRINT", "QGPL"), "QSYS".to_string()),
        ]);
        h.autls.push("NSUSERS".into());
        for (p, sp) in [
            ("QSECOFR", vec!["*ALLOBJ".to_string()]),
            ("QPGMR", Vec::new()),
            ("WEBUSER", Vec::new()),
            ("QSHGUEST", Vec::new()),
            ("TESTUSER", Vec::new()),
        ] {
            h.profiles.push((p.into(), sp));
        }
        h.disabled.push("TESTUSER".into());
        let tbl = &mut h.table;
        tbl.started(t - 2 * 86_400);
        let a = tbl.register(session(
            "ACCT-PC01",
            "acct-pc01.qsh.example.com",
            "QPGMR",
            "123470/QUSER/QZLSFILE",
            t - 5_400,
        ));
        let c = tbl.connect(a, "HOME", 0, t - 5_380).expect("session");
        tbl.set_opens(a, c, 2, t - 120).expect("connection");
        tbl.connect(a, "PRT01", 1, t - 5_370).expect("session");
        let b = tbl.register(session(
            "10.0.10.77",
            "10.0.10.77",
            "QSHGUEST",
            "123471/QUSER/QZLSFILE",
            t - 900,
        ));
        let c = tbl.connect(b, "DOCS", 0, t - 890).expect("session");
        tbl.set_opens(b, c, 1, t - 60).expect("connection");
        let d = tbl.register(session(
            "LAB-WS12",
            "lab-ws12.qsh.example.com",
            "WEBUSER",
            "123472/QUSER/QZLSFILE",
            t - 300,
        ));
        tbl.touch(d, t - 30).expect("session");
        Self {
            host: Mutex::new(h),
            since: vec![("TESTUSER".into(), t - 7_200)],
            called: Mutex::new(Vec::new()),
        }
    }
}

/// The layout of each message's data, as its message description gives
/// it: `C` character of a length, `B` a binary of 4 or 8 bytes.
const VARS: &[(&str, &[(char, usize)])] = &[
    ("CPFB684", &[('C', 10), ('B', 4)]),
    ("CPFB685", &[('C', 10), ('B', 4), ('B', 4), ('C', 1)]),
    ("CPFB686", &[('C', 10), ('B', 4), ('B', 4)]),
    ("CPFB687", &[('C', 10), ('C', 10), ('C', 10)]),
    ("CPFB68A", &[('C', 10), ('C', 12), ('B', 4)]),
    ("CPFB68B", &[('C', 10), ('B', 4), ('C', 15)]),
    ("CPFB68C", &[('C', 10), ('C', 10), ('C', 10)]),
    (
        "CPFB68D",
        &[('C', 10), ('B', 4), ('B', 4), ('B', 4), ('B', 4)],
    ),
    ("CPFB68E", &[('C', 10), ('C', 15), ('B', 4), ('B', 8)]),
    ("CPFB694", &[('B', 4)]),
    ("CPDB680", &[('C', 10)]),
];

/// The message of a failed call.
pub fn message(f: &Failure) -> Message {
    let layout = VARS
        .iter()
        .find(|(id, _)| *id == f.id)
        .map_or(&[][..], |v| v.1);
    let mut at = 0;
    let mut vals = Vec::new();
    for (t, n) in layout {
        vals.push(match (t, n) {
            ('B', 8) => bin8(&f.data, at).to_string(),
            ('B', _) => bin4(&f.data, at).to_string(),
            _ => text_at(&f.data, at, *n),
        });
        at += n;
    }
    let refs: Vec<&str> = vals.iter().map(String::as_str).collect();
    let t = text::second_level(&f.id, &refs).map_or_else(|| f.id.clone(), |s| s.text);
    Message::new(MsgKind::Error, &f.id, &t).with_data(&refs)
}

impl FixtureNetServer {
    /// Run `f` over the host as `user`.
    fn call<T>(
        &self,
        user: &str,
        f: impl FnOnce(&mut dyn client::Api) -> Result<T, Failure>,
    ) -> Result<T, Message> {
        let mut h = self.host.lock().expect("netsvr");
        h.caller.user = user.into();
        h.caller.iosyscfg = has_special(user, "*IOSYSCFG");
        h.caller.secadm = has_special(user, "*SECADM");
        // Enabling needs *OBJMGT and *USE to the profile: the security
        // officer's, here.
        h.not_manageable = if has_special(user, "*SECADM") {
            Vec::new()
        } else {
            h.profiles.iter().map(|(p, _)| p.clone()).collect()
        };
        let called = &self.called;
        let mut api = |name: &str, parms: Vec<Vec<u8>>| {
            called.lock().expect("called").push(name.to_string());
            client::Api::call(&mut *h, name, parms)
        };
        f(&mut api).map_err(|e| message(&e))
    }
}

impl NetServer for FixtureNetServer {
    fn status(&self, _user: &str) -> Status {
        let h = self.host.lock().expect("netsvr");
        Status {
            active: h.server.active,
            job: if h.server.active {
                "123301/QSYS/QZLSSERVER".into()
            } else {
                String::new()
            },
        }
    }

    fn shares(&self, user: &str) -> Result<Vec<Share>, Message> {
        self.call(user, |a| client::list_shares(a, 37))
    }

    fn attributes(&self, user: &str) -> Result<Attributes, Message> {
        self.call(user, client::read_attributes)
    }

    fn sessions(&self, _user: &str) -> Vec<Session> {
        let h = self.host.lock().expect("netsvr");
        h.table.sessions().cloned().collect()
    }

    fn disabled(&self, _user: &str) -> Vec<Disabled> {
        let h = self.host.lock().expect("netsvr");
        let mut v: Vec<Disabled> = h
            .disabled
            .iter()
            .map(|u| Disabled {
                user: u.clone(),
                since: self
                    .since
                    .iter()
                    .find(|(n, _)| n == u)
                    .map_or(0, |(_, t)| *t),
            })
            .collect();
        v.sort_by(|a, b| a.user.cmp(&b.user));
        v
    }

    fn add_share(&self, user: &str, s: &Share) -> Result<(), Message> {
        self.call(user, |a| client::add_share(a, s))
    }

    fn change_share(&self, user: &str, s: &Share) -> Result<(), Message> {
        self.call(user, |a| client::change_share(a, s))
    }

    fn remove_share(&self, user: &str, name: &str) -> Result<(), Message> {
        self.call(user, |a| client::remove_share(a, name))
    }

    fn change_attributes(&self, user: &str, from: &Settings, to: &Settings) -> Result<(), Message> {
        self.call(user, |a| client::change_attributes(a, from, to))
    }

    fn start(&self, user: &str, reset: bool) -> Result<(), Message> {
        self.call(user, |a| client::start(a, reset))
    }

    fn end(&self, user: &str) -> Result<(), Message> {
        self.call(user, client::end)?;
        // The server's jobs end with their sessions.
        let mut h = self.host.lock().expect("netsvr");
        let ids: Vec<u64> = h.table.sessions().map(|s| s.id).collect();
        for id in ids {
            let _ = h.table.unregister(id, false);
        }
        Ok(())
    }

    fn end_session(&self, user: &str, id: u64) -> Result<(), Message> {
        self.call(user, |a| client::end_session(a, id))?;
        // The serving job ends the session it was asked to.
        let _ = self
            .host
            .lock()
            .expect("netsvr")
            .table
            .unregister(id, false);
        Ok(())
    }

    fn enable(&self, user: &str, name: &str) -> Result<(), Message> {
        self.call(user, |a| client::enable_users(a, &[name.to_string()]))
    }
}
