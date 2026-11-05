// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The physical file trigger commands on `db::trg::ops`: `ADDPFTRG`,
//! `RMVPFTRG` and `CHGPFTRG`, with the primary's messages
//! (`tools/conformance/sql/oracle/trigger/primary-facts.md`). The file is
//! held `*EXCL` while its triggers change: a member loads them as it
//! opens.

use cmd::block::Parms;
use db::trg::ops::{self, NewTrg, Pick, op};
use db::trg::{Event, MtAction, Threadsafe, Time};
use qsname::Name;
use stg::proto::{CmdMsg, CmdReply};
use store::obj::{JobHandle, LockState, ObjectId, ops as objops, types};
use store::txn::Txn;

use super::direct::{in_store, lib_or};
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    Some(match name {
        "ADDPFTRG" => add(system, parms),
        "RMVPFTRG" => remove(system, parms),
        "CHGPFTRG" => change(system, parms),
        _ => return None,
    })
}

fn msg(id: &str, kind: u8, fields: &[(&str, usize)]) -> CmdMsg {
    CmdMsg { msgid: id.into(), kind, data: cmd::error::data(fields) }
}

/// What failed: a diagnostic then `CPF32C6` naming the file and the
/// operation.
struct Refused {
    diag: Option<CmdMsg>,
    text: String,
}

impl From<String> for Refused {
    fn from(text: String) -> Self {
        let id = super::leading_msgid(&text).unwrap_or("CPF9898").to_string();
        Self { diag: Some(msg(&id, 5, &[(&text, 256)])), text }
    }
}

fn refused(r: Refused, file: &(String, String), code: u8) -> CmdReply {
    let mut data = cmd::error::data(&[(&file.0, 10), (&file.1, 10)]);
    data.extend_from_slice(&u32::from(code).to_be_bytes());
    CmdReply {
        text: format!("CPF32C6 {}\n", r.text),
        messages: r.diag.into_iter().collect(),
        escape: Some(CmdMsg { msgid: "CPF32C6".into(), kind: 7, data }),
        ..CmdReply::default()
    }
}

/// The database's refusal as its diagnostic.
fn diag(e: &db::DbError, trg: &str, file: &str) -> Refused {
    let d = match e {
        db::DbError::Trigger { reason, .. } => match reason {
            ops::Refusal::NotPhysical => msg("CPF3210", 5, &[(file, 10), ("", 10)]),
            r => msg(r.msgid(), 5, &[(trg, 258), (file, 10)]),
        },
        other => msg(other.msgid().unwrap_or("CPF9898"), 5, &[(&other.to_string(), 256)]),
    };
    Refused { diag: Some(d), text: e.to_string() }
}

fn job(system: &System) -> JobHandle {
    JobHandle(system.jobs.get(system.current_job).map_or(u64::from(system.current_job), |j| j.handle))
}

/// The file, held `*EXCL` for `f`, with its library's name. A refusal
/// leaves nothing changed: the operations check before they store.
fn with_file<T>(
    system: &mut System,
    q: &(String, String),
    f: impl FnOnce(&mut Txn<'_>, ObjectId, &objops::NameContext, ObjectId) -> Result<T, Refused>,
) -> Result<(T, String), Refused> {
    let job = job(system);
    let r = in_store(system, |txn, qsys, nm| {
        let id = wm::obj::find(txn, qsys, nm, &lib_or(&q.1), &q.0, types::FILE)
            .map_err(|_| format!("CPF9812 {} {}", q.0, lib_or(&q.1)))?;
        let lib = db::file::describe(txn, id).map_err(|e| e.to_string())?.library_name.to_string();
        db::odp::allocate(txn, job, id, LockState::Exclusive, &q.0).map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF9898")))?;
        let r = f(txn, qsys, nm, id);
        db::odp::deallocate(txn, job, id, LockState::Exclusive);
        Ok(r.map(|v| (v, lib)))
    });
    r.map_err(Refused::from)?
}

fn name10(s: &str, kw: &str) -> Result<Name, Refused> {
    Name::parse(&s.to_ascii_uppercase()).map_err(|e| Refused::from(format!("CPD0078 {kw}: {e}")))
}

/// A trigger name as given: upper case unless quoted.
fn trg_name(raw: &str) -> String {
    let t = raw.trim();
    match t.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(q) => q.to_string(),
        None => t.to_ascii_uppercase(),
    }
}

fn time(w: &str) -> Option<Time> {
    match w {
        "*BEFORE" => Some(Time::Before),
        "*AFTER" => Some(Time::After),
        _ => None,
    }
}

fn event(w: &str) -> Option<Event> {
    Some(match w {
        "*INSERT" => Event::Insert,
        "*DELETE" => Event::Delete,
        "*UPDATE" => Event::Update,
        "*READ" => Event::Read,
        _ => return None,
    })
}

/// A library name or `*CURLIB` resolved.
fn lib_name(txn: &mut Txn<'_>, qsys: ObjectId, nm: &objops::NameContext, raw: &str) -> Result<Name, Refused> {
    if raw == "*CURLIB" {
        let id = super::direct::library(txn, qsys, nm, "*CURLIB")?;
        let (_, h) = objops::read_header(txn, id).map_err(|e| e.to_string())?;
        return Ok(h.name);
    }
    name10(raw, "TRGLIB")
}

/// `ADDPFTRG`.
fn add(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("FILE");
    let p = parms.qual("PGM");
    let definer = crate::stgsvc::security::actor().map_or_else(|| "QSYS".to_string(), |a| a.user.clone());
    let raw = parms.text("TRG");
    let given = (!raw.eq_ignore_ascii_case("*GEN")).then(|| trg_name(&raw));
    let shown = given.clone().unwrap_or_else(|| "*GEN".into());
    let r = with_file(system, &q, |txn, qsys, nm, id| {
        let pgm = wm::obj::find(txn, qsys, nm, &lib_or(&p.1), &p.0, types::PGM).map_err(|_| Refused {
            diag: Some(msg("CPF9811", 5, &[(&p.0, 10), (&lib_or(&p.1), 10)])),
            text: format!("CPF9811 {} {}", p.0, lib_or(&p.1)),
        })?;
        let (_, h) = objops::read_header(txn, pgm).map_err(|e| e.to_string())?;
        let (_, lh) = objops::read_header(txn, h.library).map_err(|e| e.to_string())?;
        let mut n = NewTrg::new(
            time(&parms.name("TRGTIME")).unwrap_or(Time::After),
            event(&parms.name("TRGEVENT")).unwrap_or(Event::Insert),
            (lh.name, h.name),
        );
        n.name.clone_from(&given);
        let tl = parms.name("TRGLIB");
        if tl != "*FILE" && !tl.is_empty() {
            n.library = Some(lib_name(txn, qsys, nm, &tl)?);
        }
        n.replace = parms.name("RPLTRG") == "*YES";
        n.alwrepchg = parms.name("ALWREPCHG") == "*YES";
        n.on_change = parms.name("TRGUPDCND") == "*CHANGE";
        n.threadsafe = match parms.name("THDSAFE").as_str() {
            "*YES" => Threadsafe::Yes,
            "*NO" => Threadsafe::No,
            _ => Threadsafe::Unknown,
        };
        n.mt_action = match parms.name("MLTTHDACN").as_str() {
            "*MSG" => MtAction::Msg,
            "*NORUN" => MtAction::NoRun,
            "*RUN" => MtAction::Run,
            _ => MtAction::Sysval,
        };
        ops::authorize_add(txn, id, pgm, n.alwrepchg).map_err(|e| diag(&e, &shown, &q.0))?;
        n.definer.clone_from(&definer);
        let name = ops::add(txn, id, &n, now_unix()).map_err(|e| diag(&e, &shown, &q.0))?;
        let lib = n.library.map_or_else(|| db::file::describe(txn, id).map(|d| d.library_name.to_string()).unwrap_or_default(), |l| l.to_string());
        Ok((name, lib))
    });
    match r {
        Ok(((name, tlib), flib)) => CmdReply {
            text: crate::stgsvc::msgs::text(system, "CPC3203", &[]) + "\n",
            messages: vec![
                msg("CPI321A", 4, &[(&name, 258), (&tlib, 10), (&q.0, 10), (&flib, 10)]),
                msg("CPC3203", 4, &[]),
            ],
            ..CmdReply::default()
        },
        Err(e) => refused(e, &(q.0.clone(), lib_or(&q.1)), op::ADD),
    }
}

/// `TRG` and `TRGLIB` as a pick, or `None` for every trigger.
fn named(txn: &mut Txn<'_>, qsys: ObjectId, nm: &objops::NameContext, parms: &Parms) -> Result<Option<Pick>, Refused> {
    let raw = parms.text("TRG");
    if raw.is_empty() || raw.eq_ignore_ascii_case("*ALL") {
        return Ok(None);
    }
    let tl = parms.name("TRGLIB");
    let lib = if tl == "*ALL" || tl.is_empty() { None } else { Some(lib_name(txn, qsys, nm, &tl)?) };
    Ok(Some(Pick::Named(trg_name(&raw), lib)))
}

/// `RMVPFTRG`.
fn remove(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("FILE");
    let shown = trg_name(&parms.text("TRG"));
    let r = with_file(system, &q, |txn, qsys, nm, id| {
        ops::authorize_change(txn, id).map_err(|e| diag(&e, &shown, &q.0))?;
        let pick = match named(txn, qsys, nm, parms)? {
            Some(p) => p,
            None => Pick::When(time(&parms.name("TRGTIME")), event(&parms.name("TRGEVENT"))),
        };
        let lib_of: Vec<(String, String)> = db::trg::list(txn, id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|t| (t.name, t.library.to_string()))
            .collect();
        let gone = ops::remove(txn, id, &pick, now_unix()).map_err(|e| diag(&e, &shown, &q.0))?;
        Ok(gone.into_iter().map(|n| {
            let l = lib_of.iter().find(|(a, _)| *a == n).map(|(_, l)| l.clone()).unwrap_or_default();
            (n, l)
        }).collect::<Vec<_>>())
    });
    match r {
        Ok((gone, flib)) => {
            let mut messages: Vec<CmdMsg> = gone.iter().map(|(n, l)| msg("CPI321B", 4, &[(n, 258), (l, 10), (&q.0, 10), (&flib, 10)])).collect();
            messages.push(msg("CPC3205", 4, &[]));
            let text = crate::stgsvc::msgs::text(system, "CPC3205", &[]) + "\n";
            CmdReply { text, messages, ..CmdReply::default() }
        }
        Err(e) => refused(e, &(q.0.clone(), lib_or(&q.1)), op::REMOVE),
    }
}

/// `CHGPFTRG`.
fn change(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("FILE");
    let shown = trg_name(&parms.text("TRG"));
    let state = parms.name("STATE");
    let r = with_file(system, &q, |txn, qsys, nm, id| {
        ops::authorize_change(txn, id).map_err(|e| diag(&e, &shown, &q.0))?;
        let pick = named(txn, qsys, nm, parms)?.unwrap_or(Pick::When(None, None));
        if state == "*SAME" {
            return Ok(0);
        }
        ops::set_state(txn, id, &pick, state == "*ENABLED", now_unix()).map_err(|e| diag(&e, &shown, &q.0))
    });
    match r {
        Ok((n, _)) => {
            let mut messages: Vec<CmdMsg> = (0..n).map(|_| msg("CPI32E8", 4, &[])).collect();
            messages.push(msg("CPC32E8", 4, &[(&q.0, 10)]));
            let text = crate::stgsvc::msgs::text(system, "CPC32E8", &[&q.0]) + "\n";
            CmdReply { text, messages, ..CmdReply::default() }
        }
        Err(e) => refused(e, &(q.0.clone(), lib_or(&q.1)), op::CHANGE),
    }
}
