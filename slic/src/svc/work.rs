// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management commands of the service console: job descriptions,
//! classes and the job's attributes. They run in the storage service
//! for the job that typed them.

use std::fmt::Write as _;

use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;
use wm::cmd::Params;
use wm::obj::cls::Cls;
use wm::obj::jobd::JobD;
use wm::obj::params::{self, qualified};

use crate::ipl::system::{System, now_unix};

/// Help for these commands.
pub const HELP: &str = "\
crtjobd LIB/JOBD [USER() JOBQ() JOBPTY() OUTQ() RTGDTA() RQSDTA() INLLIBL() LOG(l s t) INQMSGRPY() HOLD() TEXT() ...]
chgjobd LIB/JOBD keywords | dltjobd LIB/JOBD | dspjobd LIB/JOBD
crtcls LIB/CLS [RUNPTY() TIMESLICE() PURGE() DFTWAIT() CPUTIME() MAXTMPSTG() MAXTHD() TEXT()]
chgcls LIB/CLS keywords | dltcls LIB/CLS | dspcls LIB/CLS
chgjob [JOB(nnnnnn/USER/NAME)] keywords | dspjob [JOB(nnnnnn/USER/NAME)]
wrkactjob            the active jobs
dsplibl | chglibl LIBL(..) [CURLIB()] | addlible LIB [POSITION(*LAST)] | rmvlible LIB | chgcurlib LIB | chgsyslibl LIB [OPTION(*REMOVE)]
";

/// Dispatch a command of this module; `None` when `cmd` is not one.
pub fn command(system: &mut System, cmd: &str, line: &str) -> Option<String> {
    let p = Params::parse(line);
    let r = match cmd {
        "crtjobd" => create(system, &p, types::JOBD),
        "crtcls" => create(system, &p, types::CLS),
        "chgjobd" => change(system, &p, types::JOBD),
        "chgcls" => change(system, &p, types::CLS),
        "dltjobd" => delete(system, &p, types::JOBD),
        "dltcls" => delete(system, &p, types::CLS),
        "dspjobd" => display(system, &p, types::JOBD),
        "dspcls" => display(system, &p, types::CLS),
        "chgjob" => chgjob(system, &p),
        "dspjob" => dspjob(system, &p),
        _ => return None,
    };
    Some(match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    })
}

fn kind(type_code: u16) -> &'static str {
    if type_code == types::JOBD {
        "Job description"
    } else {
        "Class"
    }
}

/// Run `f` on ASP 1 in a transaction.
fn in_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    super::db::with_txn(&mut s.store, |txn| f(txn, qsys))
}

fn err(e: &wm::Error) -> String {
    match e.msgid() {
        Some(id) => format!("{id} {e}"),
        None => e.to_string(),
    }
}

/// The payload of `type_code` built from parameters over `base`.
fn build(type_code: u16, base: Option<&[u8]>, p: &Params) -> Result<(Vec<u8>, String), String> {
    if type_code == types::JOBD {
        let d = match base {
            Some(b) => JobD::decode(b).map_err(|e| err(&e))?,
            None => JobD::default(),
        };
        let d = params::jobd(d, p).map_err(|e| err(&e))?;
        Ok((d.encode(), d.text))
    } else {
        let c = match base {
            Some(b) => Cls::decode(b).map_err(|e| err(&e))?,
            None => Cls::default(),
        };
        let c = params::cls(c, p).map_err(|e| err(&e))?;
        Ok((c.encode(), c.text))
    }
}

fn target(p: &Params, type_code: u16) -> Result<(String, String), String> {
    let kw = if type_code == types::JOBD {
        "JOBD"
    } else {
        "CLS"
    };
    Ok(qualified(&p.pos_or(0, kw)?))
}

/// `CRTJOBD`, `CRTCLS`.
fn create(system: &mut System, p: &Params, type_code: u16) -> Result<String, String> {
    let (lib, name) = target(p, type_code)?;
    let lib = if lib == "*LIBL" {
        "*CURLIB".into()
    } else {
        lib
    };
    let (payload, text) = build(type_code, None, p)?;
    in_store(system, |txn, qsys| {
        let names = super::ile::name_context(txn, qsys);
        let l = match lib.as_str() {
            "*CURLIB" => names.curlib.ok_or("CPF2110 no current library")?,
            l => wm::obj::library(txn, qsys, l).map_err(|e| err(&e))?,
        };
        let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
        let stamp = wm::obj::Stamp {
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: now_unix(),
        };
        wm::obj::create(txn, l, n, type_code, &text, &stamp, &payload).map_err(|e| err(&e))?;
        Ok(())
    })?;
    Ok(format!(
        "{} {name} created in library {lib}.\n",
        kind(type_code)
    ))
}

fn find(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    lib: &str,
    name: &str,
    type_code: u16,
) -> Result<ObjectId, String> {
    let names = super::ile::name_context(txn, qsys);
    wm::obj::find(txn, qsys, &names, lib, name, type_code).map_err(|e| err(&e))
}

/// `CHGJOBD`, `CHGCLS`.
fn change(system: &mut System, p: &Params, type_code: u16) -> Result<String, String> {
    let (lib, name) = target(p, type_code)?;
    in_store(system, |txn, qsys| {
        let id = find(txn, qsys, &lib, &name, type_code)?;
        let old = wm::obj::read(txn, id).map_err(|e| err(&e))?;
        let (payload, text) = build(type_code, Some(&old), p)?;
        wm::obj::write(txn, id, &payload).map_err(|e| err(&e))?;
        if p.kw("TEXT").is_some() {
            ops::change_text(txn, id, &text, now_unix()).map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    Ok(format!("{} {name} changed.\n", kind(type_code)))
}

/// `DLTJOBD`, `DLTCLS`.
fn delete(system: &mut System, p: &Params, type_code: u16) -> Result<String, String> {
    let (lib, name) = target(p, type_code)?;
    let job = JobHandle(u64::from(system.current_job));
    in_store(system, |txn, qsys| {
        let id = find(txn, qsys, &lib, &name, type_code)?;
        ops::delete(txn, job, id).map_err(|e| e.to_string())
    })?;
    Ok(format!("{} {name} deleted.\n", kind(type_code)))
}

/// `DSPJOBD`, `DSPCLS`.
fn display(system: &mut System, p: &Params, type_code: u16) -> Result<String, String> {
    let (lib, name) = target(p, type_code)?;
    in_store(system, |txn, qsys| {
        let id = find(txn, qsys, &lib, &name, type_code)?;
        let lib_name = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
        let b = wm::obj::read(txn, id).map_err(|e| err(&e))?;
        if type_code == types::JOBD {
            let d = JobD::decode(&b).map_err(|e| err(&e))?;
            Ok(params::show_jobd(&lib_name, &name, &d))
        } else {
            let c = Cls::decode(&b).map_err(|e| err(&e))?;
            Ok(params::show_cls(&lib_name, &name, &c))
        }
    })
}

/// `CHGJOB [JOB(...)] keywords`.
fn chgjob(system: &mut System, p: &Params) -> Result<String, String> {
    let job = p.upper("JOB").unwrap_or_default();
    let changes: Vec<(String, String)> = p.kw.iter().filter(|(k, _)| k != "JOB").cloned().collect();
    if changes.is_empty() {
        return Err("CPD0042 chgjob needs at least one attribute to change".into());
    }
    let me = system.current_job;
    crate::stgsvc::chgjob(system, me, &job, &changes)?;
    Ok("Job attributes changed.\n".into())
}

/// `DSPJOB [JOB(...)]`: who the job is and its definition attributes.
fn dspjob(system: &mut System, p: &Params) -> Result<String, String> {
    let number = match p.upper("JOB") {
        None => system.current_job,
        Some(q) => wm::job::JobId::parse(&q)
            .map(|id| id.number)
            .ok_or_else(|| format!("CPF1070 job {q} not found"))?,
    };
    let j = system
        .jobs
        .get(number)
        .ok_or_else(|| format!("CPF1070 job {number:06} not found"))?;
    let a = system.attrs.get(&number).cloned().unwrap_or_default();
    let mut out = format!(
        "Job:   {}   User:   {}   Number:   {:06}\n\n",
        j.name, j.user, j.number
    );
    let rows: Vec<(&str, String)> = vec![
        ("Status of job", "ACTIVE".into()),
        ("Current status", j.job_status().code().into()),
        ("Type of job", j.kind.to_string()),
        ("Subsystem", j.subsystem.clone()),
        ("Job description", a.jobd.clone()),
        ("Job queue", a.jobq.clone()),
        ("Job priority (on job queue)", a.jobpty.to_string()),
        ("Output priority (on output queue)", a.outpty.to_string()),
        ("Run priority", a.runpty.to_string()),
        ("Time slice in milliseconds", a.timeslice.to_string()),
        ("Print device", a.prtdev.clone()),
        ("Output queue", a.outq.clone()),
        (
            "Message logging",
            format!("{} {} {}", a.log_level, a.log_sev, a.log_text),
        ),
        ("Log CL program commands", a.logclpgm.clone()),
        ("Job log output", a.logoutput.clone()),
        ("Inquiry message reply", a.inqmsgrpy.clone()),
        ("Break message handling", a.brkmsg.clone()),
        ("Status message", a.stsmsg.clone()),
        ("Date format", a.datfmt.clone()),
        ("Date separator", a.datsep.clone()),
        ("Time separator", a.timsep.clone()),
        ("Job switches", a.sws.clone()),
        ("Coded character set identifier", a.ccsid.to_string()),
        ("Library list", a.libl.search().join(" ")),
        ("Routing data", a.rtgdta.clone()),
    ];
    for (k, v) in rows {
        let _ = writeln!(out, "  {k:<38} {v}");
    }
    Ok(out)
}

/// The library list commands; `None` when `cmd` is not one.
pub fn libl_command(system: &mut System, cmd: &str, line: &str) -> Option<String> {
    let p = Params::parse(line);
    let r = match cmd {
        "dsplibl" => Ok(dsplibl(system)),
        "chglibl" => change_libl(system, &p),
        "addlible" => addlible(system, &p),
        "rmvlible" => rmvlible(system, &p),
        "chgcurlib" => chgcurlib(system, &p),
        "chgsyslibl" => chgsyslibl(system, &p),
        _ => return None,
    };
    Some(match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    })
}

/// Run `f` on the job's library list; the change holds for the rest of
/// this request too.
fn with_libl<T>(
    system: &mut System,
    f: impl FnOnce(&mut wm::job::LibList) -> Result<T, String>,
) -> Result<T, String> {
    let me = system.current_job;
    let a = system
        .attrs
        .get_mut(&me)
        .ok_or("CPF1070 the job has no attributes")?;
    let mut l = a.libl.clone();
    let r = f(&mut l)?;
    a.libl = l.clone();
    super::ile::set_libl(Some(l));
    Ok(r)
}

/// A library must exist (`QTEMP` always does).
fn check_lib(system: &mut System, lib: &str) -> Result<String, String> {
    let lib = lib.to_ascii_uppercase();
    if lib == "QTEMP" {
        return Ok(lib);
    }
    in_store(system, |txn, qsys| {
        wm::obj::library(txn, qsys, &lib).map_err(|_| format!("CPF2110 library {lib} not found"))
    })?;
    Ok(lib)
}

fn libl_err(e: &wm::Error) -> String {
    err(e)
}

/// `DSPLIBL`.
fn dsplibl(system: &System) -> String {
    let me = system.current_job;
    let l = system
        .attrs
        .get(&me)
        .map(|a| a.libl.clone())
        .unwrap_or_default();
    let mut out = String::from("Library     Type\n");
    for (part, libs) in [
        ("SYS", l.sys.clone()),
        ("PRD", l.prd.clone()),
        ("CUR", l.cur.iter().cloned().collect()),
        ("USR", l.usr.clone()),
    ] {
        for lib in libs {
            let _ = writeln!(out, "{lib:<10}  {part}");
        }
    }
    out
}

/// `CHGLIBL LIBL(...) [CURLIB(...)]`.
fn change_libl(system: &mut System, p: &Params) -> Result<String, String> {
    let raw = p
        .kw("LIBL")
        .map(str::to_string)
        .or_else(|| (!p.pos.is_empty()).then(|| p.pos.join(" ")))
        .ok_or("CPD0071 LIBL is needed")?;
    let libs: Vec<String> = if raw.eq_ignore_ascii_case("*NONE") {
        Vec::new()
    } else {
        raw.split_whitespace()
            .map(|l| check_lib(system, l))
            .collect::<Result<_, _>>()?
    };
    let cur = match p.upper("CURLIB") {
        Some(c) if c == "*CRTDFT" => Some(None),
        Some(c) if c == "*SAME" => None,
        Some(c) => Some(Some(check_lib(system, &c)?)),
        None => None,
    };
    with_libl(system, |l| {
        if let Some(c) = cur {
            l.cur = c;
        }
        l.change(libs).map_err(|e| libl_err(&e))
    })?;
    Ok("Library list changed.\n".into())
}

/// `ADDLIBLE LIB [POSITION(*FIRST|*LAST)]`.
fn addlible(system: &mut System, p: &Params) -> Result<String, String> {
    let lib = check_lib(system, &p.pos_or(0, "LIB")?)?;
    let first = !matches!(p.upper("POSITION").as_deref(), Some("*LAST"));
    with_libl(system, |l| l.add(&lib, first).map_err(|e| libl_err(&e)))?;
    Ok(format!("Library {lib} added to library list.\n"))
}

/// `RMVLIBLE LIB`.
fn rmvlible(system: &mut System, p: &Params) -> Result<String, String> {
    let lib = p.pos_or(0, "LIB")?.to_ascii_uppercase();
    with_libl(system, |l| l.remove(&lib).map_err(|e| libl_err(&e)))?;
    Ok(format!("Library {lib} removed from library list.\n"))
}

/// `CHGCURLIB CURLIB(LIB|*CRTDFT)`.
fn chgcurlib(system: &mut System, p: &Params) -> Result<String, String> {
    let lib = p.pos_or(0, "CURLIB")?.to_ascii_uppercase();
    if lib == "*CRTDFT" {
        set_curlib(system, None)?;
        return Ok("Current library changed to *CRTDFT.\n".into());
    }
    let lib = check_lib(system, &lib)?;
    set_curlib(system, Some(lib.clone()))?;
    Ok(format!("Current library changed to {lib}.\n"))
}

/// The job's current library; `None` is `*CRTDFT`.
pub fn set_curlib(system: &mut System, lib: Option<String>) -> Result<(), String> {
    with_libl(system, |l| {
        if let Some(c) = &lib
            && (l.sys.contains(c) || l.usr.contains(c))
        {
            return Err(format!(
                "CPF2103 library {c} is already in the library list"
            ));
        }
        l.cur = lib;
        Ok(())
    })
}

/// `CHGSYSLIBL LIB [OPTION(*ADD|*REMOVE)]`.
fn chgsyslibl(system: &mut System, p: &Params) -> Result<String, String> {
    let lib = check_lib(system, &p.pos_or(0, "LIB")?)?;
    let remove = matches!(p.upper("OPTION").as_deref(), Some("*REMOVE"));
    with_libl(system, |l| {
        let mut sys = l.sys.clone();
        if remove {
            if !sys.contains(&lib) {
                return Err(format!("CPF2104 library {lib} is not in the system part"));
            }
            sys.retain(|s| *s != lib);
        } else {
            if l.contains(&lib) {
                return Err(format!(
                    "CPF2103 library {lib} is already in the library list"
                ));
            }
            sys.insert(0, lib.clone());
        }
        l.change_sys(sys).map_err(|e| libl_err(&e))
    })?;
    Ok(format!("System library list changed for {lib}.\n"))
}
