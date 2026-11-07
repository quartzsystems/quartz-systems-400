// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `accessweb::env::System` over the job's thread: the common part
//! (`web::system::Core`: name, special authorities, class, groups,
//! audit, message files) and what Access for Web adds: sign-on that
//! lets an expired password through to Change password, the password
//! change (`QSYCHGPW`) and rules, the zone, the profile's text, the
//! profiles (`QSYLAUTU` + `QSYRUSRI`) and the home page's counts
//! (`DSPMSG`, `WRKSPLF`, `WRKUSRJOB`), read as the user.

use accessweb::env::{Audit, Counts, Profile, SignOn, System};
use qsweb::msg::{Kind, Message, SecondLevel};
use sec::api::{Answer, Query};
use sec::profile::UsrPrf;
use stg::Client;
use wm::obj::msgq::MsgKind;

use super::Link;
use crate::httpenv::stamp;
use crate::work::web::bridge::Main;
use crate::work::web::system::{Core, Dates};

/// The system as the `ACCESSWEB` job sees it.
pub struct LiveSystem {
    /// Name, dates, profiles, audit, messages.
    pub core: Core,
    /// The job's date and time attributes and zone.
    pub dates: Dates,
    /// The job's thread.
    pub link: Link,
}

fn sysval(m: &Main, name: &str) -> String {
    let n = name.to_string();
    m.ask(|l| l.sysval(n))
        .map(|v| ccsid::from_ebcdic(&v.data).trim().to_string())
        .unwrap_or_default()
}

fn error(id: &str, text: &str) -> Message {
    Message::new(Kind::Error, id, text)
}

/// Sign on with the 5250 rules; an expired password signs on to
/// change it only (no profile handle). Otherwise the profile's handle
/// for everything the user does.
fn sign_on(m: &mut Main, user: &str, password: &str) -> Result<SignOn, Message> {
    let (who, pw) = (user.to_string(), password.to_string());
    let s = m
        .ask(|l| l.sign_on(who, pw, String::new(), false))
        .map_err(|e| error(&e.msgid, &e.text))?;
    let mut out = SignOn {
        user: s.user.clone(),
        previous: s.previous,
        expires_in: s.expires_in,
        expired: s.next == sec::signon::Next::ChangePassword,
    };
    if out.expired {
        out.expires_in = None;
        return Ok(out);
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
            Ok(out)
        }
        Ok(_) => Err(error("CPF22E2", "The sign-on could not be completed.")),
        Err(id) => Err(error(
            &id,
            &format!("User profile {} could not be used by this server.", s.user),
        )),
    }
}

/// A profile's record, read as `as_user`.
fn profile(m: &Main, user: &str) -> Option<UsrPrf> {
    match m.sec(Query::Profile {
        user: user.to_string(),
    }) {
        Ok(Answer::Profile { data }) => UsrPrf::decode(&data).ok(),
        _ => None,
    }
}

/// Every profile the user may see; a group is one another profile
/// names or that has a GID.
fn profiles(m: &Main) -> Vec<Profile> {
    let q = Query::Users {
        select: "*ALL".into(),
        group: String::new(),
    };
    let Ok(Answer::Users { rows }) = m.sec(q) else {
        return Vec::new();
    };
    let all: Vec<UsrPrf> = rows
        .iter()
        .filter(|r| r.name != "*PUBLIC")
        .filter_map(|r| profile(m, &r.name))
        .collect();
    let named = |n: &str| {
        all.iter()
            .any(|p| p.grpprf == n || p.supgrpprf.iter().any(|g| g == n))
    };
    let mut out: Vec<Profile> = all
        .iter()
        .map(|p| Profile {
            name: p.name.clone(),
            class: p.usrcls.name().to_string(),
            group: p.gid != 0 || named(&p.name),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The home page's counts, read as the user.
fn counts(m: &Main, user: &str) -> Counts {
    let q = format!("QUSRSYS/{user}");
    let messages = m
        .ask(|l| l.msgq_list(q))
        .map_or(0, |v| v.iter().filter(|x| x.kind != MsgKind::Rpy).count());
    let u = user.to_string();
    let spooled = m.ask(|l| l.splf_list(u, String::new())).len();
    let active = m
        .ask(Client::active_jobs)
        .iter()
        .filter(|j| j.user == user)
        .count();
    let n = |x: usize| u32::try_from(x).unwrap_or(u32::MAX);
    Counts {
        messages: n(messages),
        spooled: n(spooled),
        active_jobs: n(active),
    }
}

impl System for LiveSystem {
    fn name(&self) -> String {
        self.core.name()
    }

    fn now(&self) -> u64 {
        self.core.now()
    }

    fn offset(&self, secs: u64) -> i64 {
        i64::from(stamp::zone_offset(&self.dates.zone, secs).unwrap_or(self.dates.offset))
    }

    fn zone_label(&self) -> String {
        let z = self.dates.zone.trim().to_string();
        sysval::tz::find(&z).map_or(z, |t| t.text.clone())
    }

    fn sign_on(&self, user: &str, password: &str, _from: &str) -> Result<SignOn, Message> {
        let u = user.trim().to_ascii_uppercase();
        if u.is_empty() {
            return Err(error("CPF1107", "No user was given."));
        }
        let pw = password.to_string();
        self.core
            .bridge()
            .call(move |m| sign_on(m, &u, &pw))
            .unwrap_or_else(|| Err(error("CPF1107", "The sign-on could not be checked.")))
    }

    fn sign_off(&self, _user: &str) {
        // The handle stays for the user's other sessions here, as
        // Navigator keeps it; the next sign-on replaces it.
    }

    fn change_password(&self, user: &str, current: &str, new: &str) -> Result<(), Message> {
        let (u, c, n) = (user.to_string(), current.to_string(), new.to_string());
        let r = self
            .core
            .bridge()
            .call(move |m| {
                // BLOCKED(rel): the password approval program (QPWDVLDPGM) is not called from a web job; refused with CPF22D3 while one is set.
                let pgm = sysval(m, "QPWDVLDPGM");
                if !pgm
                    .split_whitespace()
                    .next()
                    .unwrap_or("*NONE")
                    .starts_with('*')
                {
                    return Err("CPF22D3".to_string());
                }
                match m.sec(Query::ChangePassword {
                    user: u,
                    current: c,
                    new: n,
                }) {
                    Ok(_) => Ok(()),
                    Err(id) => Err(id),
                }
            })
            .unwrap_or_else(|| Err("CPF9897".into()));
        r.map_err(|id| {
            let text = self
                .core
                .message(&id, &[user])
                .map(|s| s.text)
                .unwrap_or_default();
            error(&id, &text)
        })
    }

    fn password_rules(&self) -> String {
        self.link
            .job(|m| {
                ["QPWDMINLEN", "QPWDMAXLEN", "QPWDRQDDIF", "QPWDLVL"]
                    .iter()
                    .map(|n| format!("{n} {}", sysval(m, n)))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    }

    fn special(&self, user: &str) -> Vec<String> {
        self.core.special(user)
    }

    fn user_class(&self, user: &str) -> String {
        self.core.user_class(user)
    }

    fn groups(&self, user: &str) -> Vec<String> {
        self.core.groups(user)
    }

    fn text(&self, user: &str) -> String {
        let u = user.to_string();
        self.link.read(user, move |m| {
            profile(m, &u).map(|p| p.text).unwrap_or_default()
        })
    }

    fn profiles(&self, user: &str) -> Vec<Profile> {
        self.link.read(user, |m| profiles(m))
    }

    fn counts(&self, user: &str) -> Counts {
        let u = user.to_string();
        self.link.read(user, move |m| counts(m, &u))
    }

    fn audit(&self, entry: Audit) {
        match entry {
            Audit::Password { user, code } => {
                self.core.audit("PW", user, code.to_string(), String::new())
            }
            Audit::Refused { user, object, need } => self.core.audit("AF", user, object, need),
        }
    }

    fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel> {
        self.core.message(id, data)
    }
}
