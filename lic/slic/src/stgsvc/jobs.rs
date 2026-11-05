// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the service keeps per job beyond its files: the job number
//! counter in `QSYS/QJOBNBR`, each job's `QTEMP` (a temporary root
//! library in ASP 1), and the active job list.

use qsname::Name;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use wm::job::{Status, next_number};

use crate::ipl::system::{System, now_unix};

/// The counter's data area in `QSYS`.
const COUNTER: &str = "QJOBNBR";

fn params(library: ObjectId, name: Name, type_code: u16, text: &str) -> CreateParams {
    CreateParams {
        library,
        name,
        type_code,
        attribute: String::new(),
        text: text.into(),
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: now_unix(),
        by_system: true,
    }
}

/// ASP 1's store and `QSYS`.
fn asp1(system: &mut System) -> Result<(&mut store::Store, ObjectId), String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    Ok((&mut s.store, qsys))
}

/// The data area holding the next IPL's `QIPLSTS`.
const NEXT_IPLSTS: &str = "QWCIPLSTS";

/// Keep `QIPLSTS` for the next IPL (`2` after `PWRDWNSYS RESTART(*YES)`).
pub fn set_next_iplsts(system: &mut System, value: u8) -> Result<(), String> {
    let (store, qsys) = asp1(system)?;
    crate::svc::db::with_txn(store, |txn| {
        let name = Name::parse(NEXT_IPLSTS).expect("valid");
        let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let v = DataAreaValue::Dec(i128::from(value));
        match lib
            .lookup(txn, types::DTAARA, name)
            .map_err(|e| e.to_string())?
        {
            Some(id) => dtaara::write(txn, id, &v).map_err(|e| e.to_string()),
            None => dtaara::create(
                txn,
                &params(qsys, name, types::DTAARA, "QIPLSTS of the next IPL"),
                DataAreaType::Dec {
                    digits: 1,
                    decimals: 0,
                },
                &v,
            )
            .map(|_| ())
            .map_err(|e| e.to_string()),
        }
    })
}

/// The `QIPLSTS` the last power down left for this IPL, reset to 0.
pub fn take_next_iplsts(system: &mut System) -> u8 {
    let Some(s) = system.storage.as_mut() else {
        return 0;
    };
    let Some(qsys) = s.qsys else {
        return 0;
    };
    let r = crate::svc::db::with_txn(&mut s.store, |txn| {
        let name = Name::parse(NEXT_IPLSTS).expect("valid");
        let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let Some(id) = lib
            .lookup(txn, types::DTAARA, name)
            .map_err(|e| e.to_string())?
        else {
            return Ok(0);
        };
        let v = match dtaara::read(txn, id).map_err(|e| e.to_string())? {
            DataAreaValue::Dec(n) => u8::try_from(n).unwrap_or(0),
            _ => 0,
        };
        dtaara::write(txn, id, &DataAreaValue::Dec(0)).map_err(|e| e.to_string())?;
        Ok(v)
    });
    r.unwrap_or(0)
}

/// The next job number, skipping numbers of active jobs and of jobs
/// that still own spooled files; the last one
/// handed out is kept in `QSYS/QJOBNBR`.
pub fn next(system: &mut System) -> Result<u32, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    let jobs = &system.jobs;
    let mut txn = s.store.begin().map_err(|e| e.to_string())?;
    let name = Name::parse(COUNTER).expect("valid");
    let lib = store::obj::library::Library::load(&mut txn, qsys).map_err(|e| e.to_string())?;
    let id = match lib
        .lookup(&mut txn, types::DTAARA, name)
        .map_err(|e| e.to_string())?
    {
        Some(id) => id,
        None => dtaara::create(
            &mut txn,
            &params(qsys, name, types::DTAARA, "Last job number"),
            DataAreaType::Dec {
                digits: 6,
                decimals: 0,
            },
            &DataAreaValue::Dec(0),
        )
        .map_err(|e| e.to_string())?,
    };
    let last = match dtaara::read(&mut txn, id).map_err(|e| e.to_string())? {
        DataAreaValue::Dec(n) => u32::try_from(n).unwrap_or(0),
        _ => 0,
    };
    let queued = &system.queued;
    let pending = &system.pending;
    let spool = &system.spool.files;
    let n = next_number(last, |n| {
        jobs.in_use(n)
            || queued.contains_key(&n)
            || pending.contains_key(&n)
            || spool.values().any(|e| e.attrs.job.2 == n)
    })
    .ok_or("every job number is in use")?;
    dtaara::write(&mut txn, id, &DataAreaValue::Dec(i128::from(n))).map_err(|e| e.to_string())?;
    txn.commit().map_err(|e| e.to_string())?;
    Ok(n)
}

/// A job's `QTEMP`: a temporary root library in ASP 1, owned by its
/// user, which CL, RPG, record I/O and SQL all reach (one `QTEMP` per
/// job). The library and when it was made.
pub fn make_qtemp(system: &mut System, user: &str) -> Result<(ObjectId, u64), String> {
    let (store, _) = asp1(system)?;
    let owner = if user.is_empty() { "QSYS" } else { user };
    crate::svc::db::with_txn(store, |txn| {
        let id = ops::create_temporary_root(txn, Name::parse("QTEMP").expect("valid"), owner, now_unix())
            .map_err(|e| e.to_string())?;
        let made = ops::read_header(txn, id).map_err(|e| e.to_string())?.1.created;
        Ok((id, made))
    })
}

/// Delete a job's `QTEMP` and everything in it.
pub fn drop_qtemp(system: &mut System, job: JobHandle, lib: ObjectId) -> Result<(), String> {
    let (store, qsys) = asp1(system)?;
    crate::svc::db::with_txn(store, |txn| {
        sql::exec::qtemp::drop_library(txn, qsys, job, lib).map_err(|e| e.to_string())
    })
}

/// The status of a job that has just said hello.
pub fn initial_status() -> u8 {
    Status::Run.byte()
}

/// What `user`'s profile supplies for `*USRPRF`; the shipped defaults
/// when the profile cannot be read.
pub fn user_dft(system: &mut System, user: &str) -> wm::job::UserDft {
    super::spool::with_store(system, |txn, qsys| {
        sec::profile::objects::read(txn, qsys, user).map_err(|e| e.to_string())
    })
    .map(|(_, p)| wm::job::UserDft {
        langid: p.langid.clone(),
        cntryid: p.cntryid.clone(),
        ccsid: p.ccsid,
        outq: if p.outq.0.starts_with('*') {
            p.outq.0
        } else {
            format!("{}/{}", p.outq.1, p.outq.0)
        },
        prtdev: p.prtdev,
    })
    .unwrap_or_default()
}

/// A job's attributes from its job description and class, `lib/name`
/// each, for `user`; a missing object falls back to the command
/// defaults.
pub fn resolve_attrs(
    system: &mut System,
    jobd: &str,
    class: &str,
    user: &str,
) -> wm::job::JobAttrs {
    use wm::obj::{cls::Cls, jobd::JobD, params::qualified};
    let prf = user_dft(system, user);
    let load = |system: &mut System, qual: &str, type_code: u16| -> Option<Vec<u8>> {
        let s = system.storage.as_mut()?;
        let qsys = s.qsys?;
        let (lib, name) = qualified(qual);
        let mut txn = s.store.begin().ok()?;
        let names = crate::svc::ile::name_context(&mut txn, qsys);
        let id = wm::obj::find(&mut txn, qsys, &names, &lib, &name, type_code).ok()?;
        let b = wm::obj::read(&mut txn, id).ok();
        txn.abort();
        b
    };
    let d = load(system, jobd, types::JOBD)
        .and_then(|b| JobD::decode(&b).ok())
        .unwrap_or_default();
    let c = load(system, class, types::CLS)
        .and_then(|b| Cls::decode(&b).ok())
        .unwrap_or_default();
    let vals = system.sysvals.as_ref();
    let sv = |name: &str| -> String {
        vals.and_then(|v| v.get(name).ok())
            .map(|v| v.text())
            .unwrap_or_default()
    };
    wm::job::JobAttrs::resolve_for(jobd, &d, &c, &sv, &prf)
}

/// `CHGJOB`: `job` blank for `self_number`'s own job.
pub fn chgjob(
    system: &mut System,
    self_number: u32,
    job: &str,
    changes: &[(String, String)],
) -> Result<(), String> {
    let number = if job.trim().is_empty() || job == "*" {
        self_number
    } else {
        let id =
            wm::job::JobId::parse(job).ok_or_else(|| format!("CPF1070 job {job} not found"))?;
        let found = system
            .jobs
            .get(id.number)
            .filter(|j| j.name == id.name.to_string() && j.user == id.user.to_string())
            .ok_or_else(|| format!("CPF1070 job {job} not found"))?;
        super::security::may_control(&found.user, &found.qualified())?;
        found.number
    };
    let user = system.jobs.get(number).map(|j| j.user.clone()).unwrap_or_default();
    let resolved: Vec<(String, String)> = changes
        .iter()
        .map(|(kw, v)| (kw.clone(), national(system, &user, kw, v)))
        .collect();
    // A new language identifier brings its language library.
    let lang_lib = resolved
        .iter()
        .find(|(k, _)| k == "LANGID")
        .map(|(_, v)| language_library(system, v));
    let attrs = system
        .attrs
        .get_mut(&number)
        .ok_or_else(|| format!("CPF1070 job {number:06} not found"))?;
    // All or nothing: change a copy, then keep it.
    let mut a = attrs.clone();
    for (kw, v) in &resolved {
        super::security::check_priority(kw, v)?;
        a.change(kw, v)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPD9D07")))?;
    }
    if let Some(l) = lang_lib {
        a.libl.set_language(l.as_deref());
    }
    *attrs = a;
    if let Some(j) = system.jobs.get(number) {
        let (name, user, kind, jobd) = (j.name.clone(), j.user.clone(), j.kind, j.jobd.clone());
        super::security::js("C", (&name, &user, number), kind, &jobd);
    }
    Ok(())
}

/// The run attributes of a class over a job's attributes.
pub fn apply_class(system: &mut System, attrs: &mut wm::job::JobAttrs, class: &str) {
    let Ok(b) = super::sbs::wm_object(system, class, "*CLS") else {
        return;
    };
    let Ok(c) = wm::obj::cls::Cls::decode(&b) else {
        return;
    };
    attrs.runpty = c.runpty;
    attrs.timeslice = c.timeslice;
    attrs.purge = c.purge;
    attrs.dftwait = c.dftwait.unwrap_or(u32::MAX);
    attrs.cputime = c.cputime.unwrap_or(u32::MAX);
    attrs.maxtmpstg = c.maxtmpstg.unwrap_or(u32::MAX);
    attrs.maxthd = c.maxthd.unwrap_or(u32::MAX);
}

/// `CHGJOB`'s special values of the national attributes as values:
/// `*SYSVAL` the system value's, `*USRPRF` the job user's profile's
/// (`CCSID` -2 and -1).
fn national(system: &mut System, user: &str, kw: &str, v: &str) -> String {
    let t = v.trim().to_ascii_uppercase();
    let sysval = |system: &System, name: &str| super::security::sysval_text(system, name).trim().to_string();
    match (kw, t.as_str()) {
        ("CCSID", "-2" | "*SYSVAL") => sysval(system, "QCCSID"),
        ("CCSID", "-1" | "*USRPRF") => match user_dft(system, user).ccsid {
            0 => sysval(system, "QCCSID"),
            n => n.to_string(),
        },
        ("CCSID", "*HEX") => "65535".into(),
        ("LANGID", "*SYSVAL") => sysval(system, "QLANGID"),
        ("CNTRYID", "*SYSVAL") => sysval(system, "QCNTRYID"),
        ("LANGID" | "CNTRYID", "*USRPRF") => {
            let p = user_dft(system, user);
            let mine = if kw == "LANGID" { p.langid } else { p.cntryid };
            if mine.trim().is_empty() || mine.starts_with('*') {
                sysval(system, if kw == "LANGID" { "QLANGID" } else { "QCNTRYID" })
            } else {
                mine.trim().to_string()
            }
        }
        _ => v.to_string(),
    }
}

/// The installed secondary language library of `langid`: `QSYS29xx`
/// when the store has it, `None` for the primary language.
pub fn language_library(system: &mut System, langid: &str) -> Option<String> {
    let lang = ccsid::nls::by_langid(langid)?;
    if lang.feature == ccsid::nls::PRIMARY {
        return None;
    }
    let lib = lang.library();
    let name = qsname::Name::parse(&lib).ok()?;
    super::spool::with_store(system, |txn, qsys| Ok(store::obj::ops::find_library(txn, qsys, name).is_ok()))
        .ok()
        .filter(|found| *found)
        .map(|_| lib)
}

