// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What both web products ask of the system, over the job's thread:
//! the system name, the job's dates, sign-on with the 5250 rules,
//! profiles, the audit journal, message files.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use qsweb::msg::{Kind, Message, SecondLevel, substitute};
use sec::api::{Answer, Query};
use stg::Client;

use super::bridge::{Bridge, Main};
use crate::httpenv::stamp;

/// How long a profile's authorities and the system name are kept.
const KEEP: Duration = Duration::from_secs(30);

/// Every special authority, for a profile read without a handle.
const SPECIAL: &[&str] = &[
    "*ALLOBJ", "*AUDIT", "*IOSYSCFG", "*JOBCTL", "*SAVSYS", "*SECADM", "*SERVICE", "*SPLCTL",
];

/// What the products ask of a profile.
#[derive(Clone, Debug, Default)]
struct Info {
    special: Vec<String>,
    groups: Vec<String>,
    class: String,
}

/// The job's date and time attributes and its time zone.
#[derive(Clone, Debug)]
pub struct Dates {
    /// `DATFMT`.
    pub datfmt: String,
    /// `DATSEP`.
    pub datsep: String,
    /// `TIMSEP`.
    pub timsep: String,
    /// `QTIMZON`.
    pub zone: String,
    /// The offset now, seconds, for a zone this system does not know.
    pub offset: i32,
}

impl Dates {
    /// The job's, read over the link.
    pub fn of(m: &Main) -> Self {
        let a = m.link.borrow_mut().with(Client::rtvjoba);
        let now = m.link.borrow_mut().with(Client::now);
        Self {
            datfmt: a.datfmt,
            datsep: a.datsep,
            timsep: a.timsep,
            zone: now.zone,
            offset: now.offset_min * 60,
        }
    }
}

/// The system name: `SYSNAME` of the network attributes, else
/// `QSYSNAME`.
pub fn system_name(m: &Main) -> String {
    let text = |v: Option<sysapi::SysVal>| v.map(|v| ccsid::from_ebcdic(&v.data).trim().to_string());
    let net = text(m.link.borrow_mut().with(|l| l.netattr("SYSNAME".into())));
    net.filter(|n| !n.is_empty())
        .or_else(|| text(m.link.borrow_mut().with(|l| l.sysval("QSYSNAME".into()))))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "QS400".into())
}

/// An error message.
pub fn error(id: &str, text: &str) -> Message {
    Message::new(Kind::Error, id, text)
}

/// Sign on with the 5250 rules, then take the profile's handle for
/// the commands the user runs.
fn sign_on(m: &mut Main, user: &str, password: &str) -> Result<String, Message> {
    let r = m
        .link
        .borrow_mut()
        .with(|l| l.sign_on(user.to_string(), password.to_string(), String::new(), false));
    let s = r.map_err(|e| error(&e.msgid, &e.text))?;
    if s.next == sec::signon::Next::ChangePassword {
        write_audit(m, "PW", user, "R", "");
        return Err(error(
            "CPF22E4",
            &format!("The password of user profile {user} has expired. Change it at a workstation, then sign on here."),
        ));
    }
    let q = Query::GetHandle {
        user: s.user.clone(),
        password: password.to_string(),
    };
    match m.sec(q) {
        Ok(Answer::Bytes { data }) => {
            if let Some(old) = m.handles.insert(s.user.clone(), data) {
                let _ = m.sec(Query::ReleaseHandle { handle: old });
            }
            Ok(s.user)
        }
        Ok(_) => Err(error("CPF22E2", "The sign-on could not be completed.")),
        Err(id) => Err(error(&id, &format!("User profile {} could not be used by this server.", s.user))),
    }
}

/// A profile's authorities, groups and class: read as the user when it
/// signed on here, else its special authorities alone.
fn info(m: &mut Main, user: &str) -> Info {
    let read = m.as_user(user, |m| {
        let p = m.link.borrow_mut().with(Client::whoami);
        let class = match m.sec(Query::Profile { user: user.to_string() }) {
            Ok(Answer::Profile { data }) => sec::profile::UsrPrf::decode(&data)
                .map(|u| u.usrcls.name().to_string())
                .unwrap_or_default(),
            _ => String::new(),
        };
        Info {
            special: p.special().names().into_iter().map(String::from).collect(),
            groups: p.groups.clone(),
            class,
        }
    });
    if let Ok(i) = read {
        return i;
    }
    let special = SPECIAL
        .iter()
        .filter(|a| {
            let q = Query::Special {
                user: user.to_string(),
                auths: vec![(*a).to_string()],
            };
            matches!(m.sec(q), Ok(Answer::Yes { yes: true }))
        })
        .map(|a| (*a).to_string())
        .collect();
    Info {
        special,
        ..Info::default()
    }
}

/// An audit journal entry through `web_audit` (`PW`, `AF`, `ZC`).
pub fn write_audit(m: &Main, kind: &str, user: &str, a: &str, b: &str) {
    let (kind, user, a, b) = (kind.to_string(), user.to_string(), a.to_string(), b.to_string());
    m.link.borrow_mut().with(|l| l.web_audit(kind, user, a, b));
}

/// The message file a message ID is in.
fn file_of(id: &str) -> (&'static str, &'static str) {
    if id.starts_with("HTP") {
        ("QZHBMSG", "QHTTPSVR")
    } else if id.starts_with("TCP") {
        ("QTCPMSG", "QSYS")
    } else if id.starts_with("SQL") {
        ("QSQLMSG", "QSYS")
    } else {
        ("QCPFMSG", "QSYS")
    }
}

/// The system as a web job sees it; each product's `System` is a thin
/// layer over it.
pub struct Core {
    bridge: Bridge,
    dates: Dates,
    name: Mutex<(String, Option<Instant>)>,
    infos: Mutex<HashMap<String, (Info, Instant)>>,
}

impl Core {
    /// Over `bridge`; `dates` the job's, `name` the system's now.
    pub fn new(bridge: Bridge, dates: Dates, name: String) -> Self {
        Self {
            bridge,
            dates,
            name: Mutex::new((name, Some(Instant::now()))),
            infos: Mutex::new(HashMap::new()),
        }
    }

    /// The job's thread.
    pub fn bridge(&self) -> &Bridge {
        &self.bridge
    }

    fn info(&self, user: &str) -> Info {
        let user = user.trim().to_ascii_uppercase();
        if let Some((i, at)) = self.infos.lock().ok().and_then(|m| m.get(&user).cloned())
            && at.elapsed() < KEEP
        {
            return i;
        }
        let u = user.clone();
        let i = self.bridge.call(move |m| info(m, &u)).unwrap_or_default();
        if let Ok(mut m) = self.infos.lock() {
            m.insert(user, (i.clone(), Instant::now()));
        }
        i
    }

    /// The system name, kept 30 seconds.
    pub fn name(&self) -> String {
        let fresh = self
            .name
            .lock()
            .ok()
            .and_then(|n| n.1.filter(|t| t.elapsed() < KEEP).map(|_| n.0.clone()));
        if let Some(n) = fresh {
            return n;
        }
        let Some(n) = self.bridge.call(|m| system_name(m)) else {
            return self.name.lock().map(|n| n.0.clone()).unwrap_or_default();
        };
        if let Ok(mut g) = self.name.lock() {
            *g = (n.clone(), Some(Instant::now()));
        }
        n
    }

    /// Seconds since 1970.
    pub fn now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }

    /// `secs` in the job's date and time formats.
    pub fn stamp(&self, secs: u64) -> (String, String) {
        let d = &self.dates;
        let offset = stamp::zone_offset(&d.zone, secs).unwrap_or(d.offset);
        stamp::stamp(secs, offset, &d.datfmt, &d.datsep, &d.timsep)
    }

    /// Sign on with the 5250 rules; the profile's name.
    pub fn sign_on(&self, user: &str, password: &str) -> Result<String, Message> {
        let u = user.trim().to_ascii_uppercase();
        if u.is_empty() {
            return Err(error("CPF1107", "Password or user profile is not correct."));
        }
        let pw = password.to_string();
        let r = self
            .bridge
            .call(move |m| sign_on(m, &u, &pw))
            .unwrap_or_else(|| Err(error("CPF1107", "The sign-on could not be checked.")));
        if let (Ok(u), Ok(mut m)) = (&r, self.infos.lock()) {
            m.remove(u);
        }
        r
    }

    /// The user's special authorities.
    pub fn special(&self, user: &str) -> Vec<String> {
        self.info(user).special
    }

    /// The user's groups.
    pub fn groups(&self, user: &str) -> Vec<String> {
        self.info(user).groups
    }

    /// The user's class.
    pub fn user_class(&self, user: &str) -> String {
        self.info(user).class
    }

    /// Whether profile `user` exists.
    pub fn profile_exists(&self, user: &str) -> bool {
        let u = user.trim().to_ascii_uppercase();
        self.bridge
            .call(move |m| match m.sec(Query::Profile { user: u }) {
                Ok(_) => true,
                // Not authorized to it: it is there.
                Err(id) => id == "CPF2217",
            })
            .unwrap_or(false)
    }

    /// Write an audit entry of `kind`.
    pub fn audit(&self, kind: &'static str, user: String, a: String, b: String) {
        let _ = self.bridge.call(move |m| write_audit(m, kind, &user, &a, &b));
    }

    /// A message's text from its message file, `data` substituted.
    pub fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel> {
        let id = id.trim().to_ascii_uppercase();
        let (file, lib) = file_of(&id);
        let ask = id.clone();
        let d = self
            .bridge
            .call(move |m| m.link.borrow_mut().with(|l| l.describe(ask, file.into(), lib.into())))
            .flatten()?;
        let (cause, recovery) = httpadmin::msgs::split_help(&d.help);
        Some(SecondLevel {
            id,
            text: substitute(&d.text, data),
            cause: substitute(&cause, data),
            recovery: substitute(&recovery, data),
            severity: u8::try_from(d.severity).unwrap_or(99),
        })
    }
}
