// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Security in the storage service: the shipped profiles and the guard
//! on ASP 1 at start, each job's session, which requests run as the job
//! (`docs/SECURITY.md`).

use std::cell::RefCell;
use std::rc::Rc;

use sec::auth::cache::Session;
use sec::auth::principal::Principal;
use sec::guard::SecGuard;
use stg::Request;

use crate::ipl::system::System;

/// The system values the guard reads when objects are created.
const GUARD_SYSVALS: [&str; 3] = ["QCRTAUT", "QCRTOBJAUD", "QSECURITY"];

/// A numeric system value, or `default` when unset.
pub fn sysval_num(system: &System, name: &str, default: u8) -> u8 {
    system
        .sysvals
        .as_ref()
        .and_then(|v| v.get(name).ok())
        .and_then(|v| v.text().trim().parse().ok())
        .unwrap_or(default)
}

/// A system value's text, empty when unset.
pub fn sysval_text(system: &System, name: &str) -> String {
    system
        .sysvals
        .as_ref()
        .and_then(|v| v.get(name).ok())
        .map(|v| v.text())
        .unwrap_or_default()
}

/// The shipped user profiles that are missing; the first time, the
/// existing objects re-owned.
pub fn install(system: &mut System) -> Result<sec::profile::shipped::Installed, String> {
    let qsecurity = sysval_num(system, "QSECURITY", 40);
    let pwdlvl = sysval_num(system, "QPWDLVL", 3);
    let Some(s) = system.storage.as_mut() else {
        return Ok(sec::profile::shipped::Installed::default());
    };
    let Some(qsys) = s.qsys else {
        return Ok(sec::profile::shipped::Installed::default());
    };
    let now = crate::ipl::system::now_unix();
    let mut txn = s.store.begin().map_err(|e| e.to_string())?;
    let d = sec::profile::shipped::install(&mut txn, qsys, qsecurity, pwdlvl, now)
        .map_err(|e| e.to_string())?;
    txn.commit().map_err(|e| e.to_string())?;
    Ok(d)
}

/// Put the guard on ASP 1. The temporary ASP has none: `QTEMP` is one
/// job's.
pub fn guard(system: &mut System) {
    system.qsecurity = sysval_num(system, "QSECURITY", 40);
    level_change(system);
    audit_force(system);
    // What the IPL itself did is not audited.
    let _ = sec::audit::take();
    let _ = sec::guard::take_failures();
    let Some(s) = system.storage.as_mut() else {
        return;
    };
    let g = SecGuard::new(s.qsys);
    s.store.set_guard(Some(g.clone()));
    system.guard = Some(g);
    refresh(system);
}

/// Give the guard the current system values.
pub fn refresh(system: &System) {
    if let Some(g) = &system.guard {
        for n in GUARD_SYSVALS {
            g.set_sysval(n, &sysval_text(system, n));
        }
    }
}

/// `user`'s principal from its profile; a user with no profile has no
/// authority of its own.
pub fn principal(system: &mut System, user: &str) -> Principal {
    let qsecurity = system.qsecurity;
    let loaded = system.storage.as_mut().and_then(|s| {
        let qsys = s.qsys?;
        let mut txn = s.store.begin().ok()?;
        Principal::load(&mut txn, qsys, user, qsecurity).ok()
    });
    loaded.unwrap_or_else(|| Principal {
        user: user.into(),
        qsecurity,
        ..Principal::default()
    })
}

/// A new session for `user`.
pub fn session(system: &mut System, user: &str) -> Rc<RefCell<Session>> {
    Rc::new(RefCell::new(Session::new(principal(system, user))))
}

/// Reload the session's principal when a profile changed since it was
/// loaded; adoption is kept.
pub fn freshen(system: &mut System, s: &Rc<RefCell<Session>>) {
    if !s.borrow().stale() {
        return;
    }
    let user = s.borrow().principal.user.clone();
    let mut p = principal(system, &user);
    let mut sess = s.borrow_mut();
    p.adopted = std::mem::take(&mut sess.principal.adopted);
    *sess = Session::new(p);
}

/// Whether a request is the job acting on objects (checked as the job)
/// rather than the system keeping its books for the job (not checked).
pub fn as_user(req: &Request) -> bool {
    use Request as R;
    matches!(
        req,
        R::Exec { .. }
            | R::Chgjob { .. }
            | R::Program { .. }
            | R::Resolve { .. }
            | R::UsLoad { .. }
            | R::UsStore { .. }
            | R::UsCreate { .. }
            | R::UsDelete { .. }
            | R::UsLibraryOf { .. }
            | R::Describe { .. }
            | R::DaLock { .. }
            | R::DaUnlock { .. }
            | R::DaRead { .. }
            | R::DaWrite { .. }
            | R::DqSend { .. }
            | R::DqReceive { .. }
            | R::Open { .. }
            | R::Io { .. }
            | R::CloseFile { .. }
            | R::ListObjects { .. }
            | R::Object { .. }
            | R::ChangeObject { .. }
            | R::RenameObject { .. }
            | R::UsridxCreate { .. }
            | R::UsridxDelete { .. }
            | R::UsridxAdd { .. }
            | R::UsridxFind { .. }
            | R::UsridxAttrs { .. }
            | R::ApiDtaqSend { .. }
            | R::ApiDtaqReceive { .. }
            | R::ApiDtaqClear { .. }
            | R::ApiDtaqDescribe { .. }
            | R::ApiDtaqEntries { .. }
            | R::ApiDtaaraRead { .. }
            | R::ApiDtaaraWrite { .. }
            | R::File { .. }
            | R::Members { .. }
            | R::Formats { .. }
            | R::Dependents { .. }
            | R::UserInfoReq { .. }
            | R::MsgqSend { .. }
            | R::MsgqReceive { .. }
            | R::MsgqReply { .. }
            | R::MsgqList { .. }
            | R::SbsCheck { .. }
            | R::SbsEnd { .. }
            | R::Sbmjob { .. }
            | R::PrtOpen { .. }
    )
}

/// The principal of the request being served; `None` is the system.
pub fn actor() -> Option<Principal> {
    sec::guard::current().map(|s| s.borrow().principal.clone())
}

/// Require special authority `a` of the actor; the system has all.
pub fn require(a: sec::profile::SpecialAuth, what: &str) -> Result<(), String> {
    match actor() {
        Some(p) => p
            .require(a, what)
            .map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

/// Controlling another job (`ENDJOB`, `HLDJOB`, `RLSJOB`, `CHGJOB`):
/// its own user, or `*JOBCTL`.
pub fn may_control(target_user: &str, job: &str) -> Result<(), String> {
    match actor() {
        Some(p) if p.user != target_user && !p.has(sec::profile::SpecialAuth::JOBCTL) => Err(format!(
            "CPF1343 {} is not authorized to control job {job}; *JOBCTL special authority is needed",
            p.user
        )),
        _ => Ok(()),
    }
}

/// A scheduling priority within the actor's `PTYLMT`.
pub fn clamp_priority(kw: &str, value: &str) -> String {
    let Some(p) = actor() else {
        return value.to_string();
    };
    match (kw, value.trim().parse::<u8>()) {
        ("JOBPTY" | "OUTPTY" | "RUNPTY", Ok(n)) if n < p.ptylmt => p.ptylmt.to_string(),
        _ => value.to_string(),
    }
}

/// DST's "allow system value security changes", as the IPL left the LIC
/// record; on when the record has no flags (as shipped).
pub fn dst_allows_sysval_changes(record: Option<&licpart::Record>) -> bool {
    record
        .and_then(|r| r.tlv(svctools::dst::security::TAG_SECURITY))
        .and_then(|t| t.first().copied())
        .is_none_or(|b| svctools::dst::security::decode(b).allow_sysval_security_changes)
}

/// The owners `programs` (`LIB/NAME`, nearest first) adopt: each must be
/// a `*PGM` or `*SRVPGM` created with `USRPRF(*OWNER)`; one that is not
/// adopts nothing.
pub fn adopted(system: &mut System, programs: &[String]) -> Vec<sec::auth::principal::Adopted> {
    let mut out = Vec::new();
    let Some(storage) = system.storage.as_mut() else {
        return out;
    };
    let Some(qsys) = storage.qsys else {
        return out;
    };
    let Ok(mut txn) = storage.store.begin() else {
        return out;
    };
    for key in programs {
        let Some((lib, name)) = key.split_once('/') else {
            continue;
        };
        let (Ok(lib_name), Ok(pgm)) = (qsname::Name::parse(lib), qsname::Name::parse(name)) else {
            continue;
        };
        let Ok(lid) = store::obj::ops::find_library(&mut txn, qsys, lib_name) else {
            continue;
        };
        let Ok(lib_obj) = store::obj::library::Library::load(&mut txn, lid.id) else {
            continue;
        };
        let id = [store::obj::types::PGM, store::obj::types::SRVPGM]
            .into_iter()
            .find_map(|ty| lib_obj.lookup(&mut txn, ty, pgm).ok().flatten());
        let Some(id) = id else {
            continue;
        };
        let Ok(p) = ile::store::load_program(&mut txn, id) else {
            continue;
        };
        if p.attr.usrprf != ile::UserProfile::Owner {
            continue;
        }
        let Ok((_, h)) = store::obj::ops::read_header(&mut txn, id) else {
            continue;
        };
        let owner = store::obj::header::text_of(&h.owner);
        if let Ok(a) = Principal::adopt(&mut txn, qsys, &owner, key) {
            out.push(a);
        }
    }
    out
}

/// Note the level this IPL runs at; going from 20 or below to 30 or
/// above takes `*ALLOBJ` from every profile not of class `*SECOFR`.
fn level_change(system: &mut System) {
    let level = system.qsecurity;
    let Some(s) = system.storage.as_mut() else {
        return;
    };
    let Some(qsys) = s.qsys else {
        return;
    };
    let Ok(mut txn) = s.store.begin() else {
        return;
    };
    match sec::level::ipl(&mut txn, qsys, level, crate::ipl::system::now_unix()) {
        Ok(c) => {
            if txn.commit().is_ok() && c.revoked > 0 {
                eprintln!(
                    "qsstg: security level {} to {}: *ALLOBJ taken from {} profile(s)",
                    c.from, c.to, c.revoked
                );
            }
        }
        Err(e) => eprintln!("qsstg: security level: {e}"),
    }
}

/// Who a request's audit entries are charged to: the job and the
/// user's `AUDLVL`.
pub struct Charged {
    /// `name`, `user`, `number`.
    pub job: (String, String, u32),
    /// The user's `AUDLVL`.
    pub audlvl: Vec<String>,
}

/// Write what the request left to audit: its authority failures as `AF`
/// entries and the events it recorded, as the auditing values select.
pub fn flush(system: &mut System, who: Option<Charged>) {
    use sec::audit::{Event, Kind};
    let failures = sec::guard::take_failures();
    let mut events = sec::audit::take();
    if failures.is_empty() && events.is_empty() {
        return;
    }
    let control = sec::audit::Control::from_sysvals(&|n: &str| sysval_text(system, n));
    if !control.on() {
        return;
    }
    for f in failures {
        events.push(
            Event::new(Kind::Af)
                .set("VIOL", "A")
                .object(&f.name, &f.library, &f.type_name)
                .set("OPER", f.op)
                .set("STEP", f.step.name())
                .set("HAVE", format!("{:08X}", f.have.0))
                .by(&f.user),
        );
    }
    let (job, lvl) = who.map_or_else(
        || (("QSYSARB".to_string(), "QSYS".to_string(), 0), Vec::new()),
        |c| (c.job, c.audlvl),
    );
    let selected: Vec<(Event, jrn::JobInfo)> = events
        .into_iter()
        .filter(|e| control.wanted(e, &lvl))
        .map(|e| {
            let user = e.user.clone().unwrap_or_else(|| job.1.clone());
            let info = jrn::JobInfo {
                name: job.0.clone(),
                user,
                number: job.2,
            };
            (e, info)
        })
        .collect();
    if selected.is_empty() {
        return;
    }
    let outcome = sec::guard::as_system(|| {
        let s = system.storage.as_mut()?;
        let qsys = s.qsys?;
        let mut txn = s.store.begin().ok()?;
        let r = sec::audit::write::write(&mut txn, qsys, &selected).ok()?;
        txn.commit().ok()?;
        Some(r)
    });
    if let Some(sec::audit::write::Outcome::Full) = outcome {
        audit_end_action(system);
    }
}

/// The audit journal's receiver is full: `QAUDENDACN`. `*NOTIFY` tells
/// `QSYSOPR` once; `*PWRDWNSYS` marks the next IPL to start with
/// `QAUDCTL(*NONE)` and powers down now (`docs/SECURITY.md` §7).
fn audit_end_action(system: &mut System) {
    let action = sysval_text(system, "QAUDENDACN");
    let power = action.trim() == "*PWRDWNSYS";
    if !power && system.audit_full_told {
        return;
    }
    system.audit_full_told = true;
    let msgid = if power { "CPI2284" } else { "CPI2283" };
    let who = ("QSYSARB".to_string(), "QSYS".to_string(), 0);
    super::hist::system_message(system, msgid, b"QAUDJRN   QSYS      ", "04", who.clone());
    let m = wm::obj::msgq::QMsg {
        msgid: msgid.into(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        severity: 90,
        text: if power {
            "The audit journal cannot be written; the system is powering down.".into()
        } else {
            "The audit journal cannot be written; audit entries are being lost.".into()
        },
        job: who,
        from_pgm: "QSYSARB".into(),
        ..wm::obj::msgq::QMsg::default()
    };
    let _ = super::msgs::send(system, "QSYS", "QSYSOPR", m.clone());
    let _ = super::msgs::send(system, "QSYS", "QSYSMSG", m);
    if power {
        force_audit_off_next_ipl(system);
        // slic powers down on SIGUSR1.
        if let Some(init) = rustix::process::Pid::from_raw(1) {
            let _ = rustix::process::kill_process(init, rustix::process::Signal::USR1);
        }
    }
}

/// Remember, in `QSYS/QAUDFORCE`, that the next IPL sets `QAUDCTL(*NONE)`.
fn force_audit_off_next_ipl(system: &mut System) {
    let Some(s) = system.storage.as_mut() else {
        return;
    };
    let Some(qsys) = s.qsys else {
        return;
    };
    let Ok(mut txn) = s.store.begin() else {
        return;
    };
    if sec::audit::force::set(&mut txn, qsys, crate::ipl::system::now_unix()).is_ok() {
        let _ = txn.commit();
    }
}

/// A `JS` entry: `etyp` `S` start, `E` end, `B` submit, `H` hold, `R`
/// release, `C` change, `N` ended by `ENDJOB`.
pub fn js(etyp: &str, job: (&str, &str, u32), kind: char, jobd: &str) {
    let by = actor().map_or_else(|| "QSYS".to_string(), |p| p.user);
    sec::audit::record(
        sec::audit::Event::new(sec::audit::Kind::Js)
            .set("ETYP", etyp)
            .set("JTYP", kind.to_string())
            .set("JNAM", job.0)
            .set("JUSR", job.1)
            .set("JNBR", format!("{:06}", job.2))
            .set("ORUS", by)
            .set("JOBD", jobd),
    );
}

/// Apply a power down the audit journal forced: `QAUDCTL(*NONE)`.
fn audit_force(system: &mut System) {
    let forced = (|| {
        let s = system.storage.as_mut()?;
        let qsys = s.qsys?;
        let mut txn = s.store.begin().ok()?;
        let on = sec::audit::force::take(&mut txn, qsys).ok()?;
        txn.commit().ok()?;
        Some(on)
    })();
    if forced != Some(true) {
        return;
    }
    if let Some(v) = system.sysvals.as_mut() {
        let _ = v.set_text("QAUDCTL", "*NONE");
    }
    let _ = crate::ipl::sysvals::save(system);
    let who = ("QSYSARB".to_string(), "QSYS".to_string(), 0);
    super::hist::system_message(system, "CPI2284", b"QAUDJRN   QSYS      ", "04", who);
}

/// Whether `QSYS/QAUDJRN` exists.
pub fn audit_journal_exists(system: &mut System) -> bool {
    let Some(s) = system.storage.as_mut() else {
        return false;
    };
    let Some(qsys) = s.qsys else {
        return false;
    };
    let Ok(mut txn) = s.store.begin() else {
        return false;
    };
    matches!(sec::audit::write::journal(&mut txn, qsys), Ok(Some(_)))
}
