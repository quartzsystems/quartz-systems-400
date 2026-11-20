// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the QSI server keeps: its identity (`QSIIDENT`, sealed under the
//! LIC partition's stash key), the consoles pinned (`QSICSL`) and its
//! event log (`QSIEVT`), files of the system under
//! `/QIBM/UserData/OS400/QSI` (the directory owner-only); and the
//! pairing password, in this process's memory alone: never written,
//! never logged, shown once to whoever made it, then given out one
//! attempt at a time to the server job. Every pairing, failed or not,
//! every removal and every password made or ended is a `QI` entry in
//! the audit journal.

use std::sync::{Mutex, MutexGuard, PoisonError};

use certs::stash::Sealer;
use qsisvr::store::{self, consoles, events, identity};
use qsitrust::{Fingerprint, Identity};
use sec::audit::{Event, Kind};
use stg::qsi::{QsiConsole, QsiPairing, QsiReply};

use super::Asker;
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::certs::{as_system, read_file, write_file};

/// How long a pairing password is good for.
pub const LIFETIME_SECS: u64 = 15 * 60;
/// How many attempts it is good for.
pub const ATTEMPTS: u32 = 5;

/// The password out.
struct Password {
    text: String,
    until: u64,
    attempts: u32,
}

static PASSWORD: Mutex<Option<Password>> = Mutex::new(None);

fn password() -> MutexGuard<'static, Option<Password>> {
    PASSWORD.lock().unwrap_or_else(PoisonError::into_inner)
}

fn path(name: &str) -> String {
    format!("{}/{name}", store::DIR)
}

fn read(system: &mut System, name: &str) -> Result<Option<Vec<u8>>, String> {
    let p = path(name);
    as_system(system, |c| read_file(c, &p))
}

fn write(system: &mut System, name: &str, bytes: &[u8]) -> Result<(), String> {
    let p = path(name);
    as_system(system, |c| write_file(c, &p, bytes))
}

fn failed_reply(e: &str) -> QsiReply {
    QsiReply::refused("CPF9898", e)
}

/// Where an entry was asked from: the service tools, a command, the
/// network.
fn wher(who: Asker) -> &'static str {
    match who {
        Asker::Dst => "D",
        Asker::Server => "N",
        Asker::Officer | Asker::Other => "C",
    }
}

fn audit(etyp: &str, who: Asker, fail: &str, c: &QsiConsole, user: &str) {
    let mut e = Event::new(Kind::Qi)
        .set("ETYP", etyp)
        .set("WHER", wher(who))
        .set("FAIL", fail)
        .set("CNAM", c.name.as_str())
        .set("FPRT", c.fingerprint.as_str())
        .set("ADDR", c.address.as_str())
        .set("USRP", user);
    if !user.is_empty() && who == Asker::Dst {
        e = e.by(user);
    }
    sec::audit::record(e);
}

// ---- The identity ----

/// The identity kept; none yet is `NoIdentity`.
pub fn identity(system: &mut System) -> QsiReply {
    match load_identity(system) {
        Ok(Some(id)) => QsiReply::Identity {
            cert: id.cert,
            key: id.key,
        },
        Ok(None) => QsiReply::NoIdentity,
        Err(e) => failed_reply(&e),
    }
}

fn load_identity(system: &mut System) -> Result<Option<Identity>, String> {
    let Some(bytes) = read(system, identity::NAME)? else {
        return Ok(None);
    };
    let sealer = crate::stgsvc::certs::sealer(system).ok_or("the LIC has no stash key")?;
    identity::decode(&bytes, |aad, s| sealer.unseal(aad, s).ok())
        .map(Some)
        .map_err(|e| format!("{}: {e}", identity::NAME))
}

/// Keep the identity the server made, when none is kept yet.
pub fn keep_identity(system: &mut System, cert: Vec<u8>, key: Vec<u8>) -> QsiReply {
    match load_identity(system) {
        Ok(Some(_)) => {
            return QsiReply::refused("CPF9898", "The system has a QSI identity already.");
        }
        Ok(None) => {}
        Err(e) => return failed_reply(&e),
    }
    let Ok(id) = Identity::new(cert, key) else {
        return QsiReply::refused("CPF9898", "Not an Ed25519 identity.");
    };
    let Some(sealer) = crate::stgsvc::certs::sealer(system) else {
        return failed_reply("the LIC has no stash key");
    };
    let bytes = match identity::encode(&id, sealer.generation(), |aad, p| sealer.seal(aad, p).ok())
    {
        Ok(b) => b,
        Err(e) => return failed_reply(&e.to_string()),
    };
    match write(system, identity::NAME, &bytes) {
        Ok(()) => QsiReply::Done,
        Err(e) => failed_reply(&e),
    }
}

/// The identity's fingerprint and when it was made.
pub fn fingerprint(system: &mut System) -> QsiReply {
    match load_identity(system) {
        Ok(Some(id)) => QsiReply::Fingerprint {
            hex: id.fingerprint().hex(),
            made: 0,
        },
        Ok(None) => QsiReply::NoIdentity,
        Err(e) => failed_reply(&e),
    }
}

// ---- The consoles ----

fn load_consoles(system: &mut System) -> Result<Vec<consoles::Console>, String> {
    match read(system, consoles::NAME)? {
        None => Ok(Vec::new()),
        Some(b) => consoles::decode(&b).map_err(|e| format!("{}: {e}", consoles::NAME)),
    }
}

fn store_consoles(system: &mut System, list: &[consoles::Console]) -> Result<(), String> {
    write(system, consoles::NAME, &consoles::encode(list))
}

fn wire(c: &consoles::Console) -> QsiConsole {
    QsiConsole {
        name: c.name.clone(),
        fingerprint: c.print().hex(),
        paired: c.paired,
        last_contact: c.last_contact,
        address: c.address.clone(),
    }
}

/// The consoles pinned.
pub fn consoles(system: &mut System) -> QsiReply {
    match load_consoles(system) {
        Ok(list) => QsiReply::Consoles {
            list: list.iter().map(wire).collect(),
        },
        Err(e) => failed_reply(&e),
    }
}

/// A pinned console had a session.
pub fn contact(system: &mut System, fingerprint: &str, address: &str) -> QsiReply {
    let Some(fp) = Fingerprint::from_hex(fingerprint) else {
        return QsiReply::Done;
    };
    let Ok(mut list) = load_consoles(system) else {
        return QsiReply::Done;
    };
    if let Some(c) = list.iter_mut().find(|c| c.fingerprint == fp.0) {
        c.last_contact = now_unix();
        c.address = address.chars().take(46).collect();
        let _ = store_consoles(system, &list);
    }
    QsiReply::Done
}

/// Remove a console by its name, its fingerprint or its place.
pub fn remove(
    system: &mut System,
    who: Asker,
    console: &str,
    by: &str,
    job_user: &str,
) -> QsiReply {
    let mut list = match load_consoles(system) {
        Ok(l) => l,
        Err(e) => return failed_reply(&e),
    };
    let want = console.trim();
    let fp = Fingerprint::from_hex(want);
    let at = list.iter().position(|c| {
        c.name.eq_ignore_ascii_case(want) || fp.is_some_and(|f| f.0 == c.fingerprint)
    });
    let at = at.or_else(|| {
        want.parse::<usize>()
            .ok()
            .filter(|n| (1..=list.len()).contains(n))
            .map(|n| n - 1)
    });
    let Some(at) = at else {
        return QsiReply::refused(
            "TCP1A86",
            &format!("No management console {want} is paired with this system."),
        );
    };
    let gone = list.remove(at);
    if let Err(e) = store_consoles(system, &list) {
        return failed_reply(&e);
    }
    let c = wire(&gone);
    audit("R", who, "", &c, &user_of(who, by, job_user));
    QsiReply::Removed { console: c }
}

fn user_of(who: Asker, by: &str, job_user: &str) -> String {
    let u = if who == Asker::Dst || job_user.is_empty() {
        by
    } else {
        job_user
    };
    u.trim().to_ascii_uppercase().chars().take(10).collect()
}

// ---- The password ----

/// The password's state, without its text; an expired one or one out
/// of attempts is gone first.
pub fn pairing() -> QsiReply {
    let mut p = password();
    expire(&mut p);
    QsiReply::Pairing {
        state: match p.as_ref() {
            Some(p) => QsiPairing {
                out: true,
                until: p.until,
                attempts: p.attempts,
            },
            None => QsiPairing::default(),
        },
    }
}

fn expire(p: &mut Option<Password>) {
    if p.as_ref()
        .is_some_and(|x| now_unix() >= x.until || x.attempts == 0)
    {
        *p = None;
    }
}

/// Make a pairing password in place of any out: its text, shown once.
pub fn start_pairing(system: &mut System, who: Asker, by: &str, job_user: &str) -> QsiReply {
    match load_consoles(system) {
        Ok(l) if l.len() >= consoles::MAX_CONSOLES => {
            return QsiReply::refused(
                "TCP1A84",
                "Two management consoles are paired with this system already; remove one first.",
            );
        }
        Ok(_) => {}
        Err(e) => return failed_reply(&e),
    }
    match load_identity(system) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return QsiReply::refused(
                "TCP1A85",
                "The system has no QSI identity yet: start the QSI server with STRTCPSVR SERVER(*QSI) first.",
            );
        }
        Err(e) => return failed_reply(&e),
    }
    let Ok(text) = qsitrust::pair::one_time_password() else {
        return failed_reply("no random source");
    };
    let until = now_unix() + LIFETIME_SECS;
    *password() = Some(Password {
        text: text.clone(),
        until,
        attempts: ATTEMPTS,
    });
    audit(
        "I",
        who,
        "",
        &QsiConsole::default(),
        &user_of(who, by, job_user),
    );
    QsiReply::Password {
        text,
        until,
        attempts: ATTEMPTS,
    }
}

/// End the password before it is used.
pub fn end_pairing(who: Asker, by: &str, job_user: &str) -> QsiReply {
    let mut p = password();
    expire(&mut p);
    if p.take().is_some() {
        audit(
            "E",
            who,
            "",
            &QsiConsole::default(),
            &user_of(who, by, job_user),
        );
    }
    QsiReply::Done
}

/// One attempt's password, counted.
pub fn attempt() -> QsiReply {
    let mut p = password();
    expire(&mut p);
    let text = p.as_mut().map(|x| {
        x.attempts -= 1;
        x.text.clone()
    });
    QsiReply::Attempt { password: text }
}

/// An attempt failed.
pub fn failed(name: &str, fingerprint: &str, address: &str, why: &str) -> QsiReply {
    let c = QsiConsole {
        name: consoles::kept_name(name),
        fingerprint: fingerprint.into(),
        address: address.into(),
        ..QsiConsole::default()
    };
    audit("F", Asker::Server, why, &c, "");
    QsiReply::Done
}

/// An attempt succeeded: pin the console and retire the password.
pub fn paired(system: &mut System, name: &str, fingerprint: &str, address: &str) -> QsiReply {
    let Some(fp) = Fingerprint::from_hex(fingerprint) else {
        return QsiReply::Flag { yes: false };
    };
    let Ok(mut list) = load_consoles(system) else {
        return QsiReply::Flag { yes: false };
    };
    if list.len() >= consoles::MAX_CONSOLES {
        return QsiReply::Flag { yes: false };
    }
    list.retain(|c| c.fingerprint != fp.0);
    let c = consoles::Console {
        name: consoles::kept_name(name),
        fingerprint: fp.0,
        paired: now_unix(),
        last_contact: 0,
        address: address.chars().take(46).collect(),
    };
    list.push(c.clone());
    if store_consoles(system, &list).is_err() {
        return QsiReply::Flag { yes: false };
    }
    *password() = None;
    audit("P", Asker::Server, "", &wire(&c), "");
    QsiReply::Flag { yes: true }
}

// ---- The event log ----

/// The event log file's bytes; empty when there is none.
pub fn load_events(system: &mut System) -> QsiReply {
    match read(system, events::NAME) {
        Ok(b) => QsiReply::Bytes {
            total: b.as_ref().map_or(0, |b| b.len() as u64),
            bytes: b.unwrap_or_default(),
        },
        Err(e) => failed_reply(&e),
    }
}

/// Keep the event log file.
pub fn save_events(system: &mut System, bytes: &[u8]) -> QsiReply {
    if events::decode(bytes).is_err() {
        return QsiReply::refused("CPF9898", "Not an event log.");
    }
    match write(system, events::NAME, bytes) {
        Ok(()) => QsiReply::Done,
        Err(e) => failed_reply(&e),
    }
}
