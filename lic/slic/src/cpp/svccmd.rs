// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The service commands that run in the storage service
//! (`docs/SERVICE.md`): `ANZPRB` (refused away from a display),
//! the contact information (`WRKCNTINF`, `CHGCNTINF`) and
//! service providers (`WRKSRVPVD`) as text, `DSPSRVSTS`, the dumps
//! `DMPOBJ` and `DMPSYSOBJ`, and the traces `STRTRC`, `ENDTRC`,
//! `PRTTRC`, `TRCJOB` and `TRCINT`.

use std::fmt::Write as _;

use cmd::Value;
use cmd::block::Parms;
use prb::PrbType;
use stg::proto::CmdReply;
use store::obj::{ObjectId, ops, types};

use super::direct::failed;
use super::done;
use crate::ipl::system::System;
use crate::stgsvc::{dumps, prblog, trace};

type R = Result<CmdReply, String>;

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r: R = match name {
        "ANZPRB" => Err("CPF39DA ANZPRB needs a display.".into()),
        "WRKCNTINF" => contact_text(system),
        "CHGCNTINF" => chgcntinf(system, parms),
        "WRKSRVPVD" => providers_text(system),
        "DSPSRVSTS" => dspsrvsts(system, parms),
        "DMPOBJ" => dmpobj(system, parms),
        "DMPSYSOBJ" => dmpsysobj(system, parms),
        "STRTRC" => strtrc(system, parms),
        "ENDTRC" => endtrc(system, parms),
        "PRTTRC" => prttrc(system, parms),
        "TRCJOB" => trcjob(system, parms),
        "TRCINT" => trcint(system, parms),
        "RCLDBXREF" => rcldbxref(system, parms),
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| failed(name, &e)))
}

fn text(t: String) -> CmdReply {
    CmdReply {
        text: t,
        ..CmdReply::default()
    }
}

fn lines_text(lines: &[String]) -> String {
    lines.iter().fold(String::new(), |mut acc, l| {
        let _ = writeln!(acc, "{l}");
        acc
    })
}

fn contact_text(system: &mut System) -> R {
    let c = prblog::contact(system)?;
    let mut out = String::from("Support contact information\n");
    for (k, v) in c.lines() {
        let _ = writeln!(out, "  {:<24} {v}", format!("{k}:"));
    }
    Ok(text(out))
}

/// A value unless it is `*SAME` or absent.
fn given(v: Option<&Value>) -> Option<String> {
    let s = v.map(Value::source)?.trim().trim_matches('\'').to_string();
    (s != "*SAME").then_some(s)
}

fn chgcntinf(system: &mut System, parms: &Parms) -> R {
    let mut c = prblog::contact(system)?;
    let set = |dst: &mut String, kwd: &str| {
        let v = parms.text(kwd);
        if !v.is_empty() && v != "*SAME" {
            *dst = v;
        }
    };
    set(&mut c.company, "CMPNY");
    set(&mut c.contact, "CONTACT");
    set(&mut c.language, "LNGVER");
    set(&mut c.media, "MEDPTF");
    set(&mut c.email, "EMAIL");
    for (kwd, dst) in [("TELNBR", &mut c.phone), ("FAXTELNBR", &mut c.fax)] {
        for (i, e) in parms.elems(kwd).into_iter().take(2).enumerate() {
            if let Some(v) = given(e) {
                dst[i] = v;
            }
        }
    }
    let addr = parms.elems("MAILADDR");
    for (i, dst) in [&mut c.street, &mut c.city, &mut c.country, &mut c.postal]
        .into_iter()
        .enumerate()
    {
        if let Some(v) = given(addr.get(i).copied().flatten()) {
            *dst = v;
        }
    }
    prblog::set_contact(system, &c)?;
    Ok(done("Contact information changed.\n".into(), "CPC39D4", &[]))
}

fn providers_text(system: &mut System) -> R {
    let c = prblog::contact(system)?;
    let mut out = String::from("Control point  Network ID  Connection  Description\n");
    for p in &c.providers {
        let _ = writeln!(out, "{:<14} {:<11} {:<11} {}", p.cpname, p.netid, p.connection, p.text);
    }
    Ok(text(out))
}

/// The job a `JOB` parameter names: its number, `None` for `*ALL`.
pub fn job_number(system: &System, parms: &Parms, kwd: &str) -> Result<Option<u32>, String> {
    let me = system.current_job;
    let quals: Vec<String> = match parms.get(kwd) {
        Some(Value::Qual(p)) => p.iter().map(|v| v.source().trim().to_ascii_uppercase()).collect(),
        Some(v) => vec![v.source().trim().to_ascii_uppercase()],
        None => vec!["*".into()],
    };
    let first = quals.first().map_or("*", String::as_str);
    match first {
        "*" | "" => return Ok(Some(me)),
        "*ALL" => return Ok(None),
        _ => {}
    }
    let user = quals.get(1).map_or("", String::as_str);
    let number = quals.get(2).and_then(|n| n.parse::<u32>().ok());
    system
        .jobs
        .all()
        .into_iter()
        .filter(|j| j.name == first && (user.is_empty() || j.user == user))
        .find(|j| number.is_none_or(|n| n == j.number))
        .map(|j| Some(j.number))
        .ok_or_else(|| format!("CPF1070 Job {first} not found."))
}

/// What `DSPSRVSTS` shows of a job: label and value.
pub fn service_status(system: &System, number: u32) -> Vec<(String, String)> {
    let traces = trace::tracing(number);
    vec![
        ("Job".into(), dumps::job_name(system, number)),
        ("Serviced by job".into(), "*NONE".into()),
        (
            "Trace active".into(),
            if traces.is_empty() { "*NO".into() } else { "*YES".into() },
        ),
        (
            "Trace sessions".into(),
            if traces.is_empty() { "*NONE".into() } else { traces.join(" ") },
        ),
    ]
}

fn dspsrvsts(system: &mut System, parms: &Parms) -> R {
    let n = job_number(system, parms, "JOB")?.unwrap_or(system.current_job);
    let lines: Vec<String> = std::iter::once("Display Service Status".to_string())
        .chain(service_status(system, n).into_iter().map(|(k, v)| format!("  {:<18} {v}", format!("{k}:"))))
        .collect();
    if parms.name("OUTPUT") == "*PRINT" {
        return crate::svc::security::print(system, "QPSRVSTS", &lines).map(text);
    }
    Ok(text(lines_text(&lines)))
}

/// Print a dump, log its problem, answer `CPC39D8`.
fn printed(system: &mut System, what: &str, lines: &[String]) -> R {
    crate::svc::security::print(system, "QPSRVDMP", lines)?;
    crate::stgsvc::liclog::write(0x0500, 0x0003, system.current_job, what);
    let d = prb::analyze::from_dump(what, &format!("Dump of {what} requested"));
    let id = prblog::log(system, PrbType::Machine, &d)?;
    Ok(done(
        format!("Dump of {what} printed; problem {id} logged.\n"),
        "CPC39D8",
        &[(what, 30)],
    ))
}

fn type_code(t: &str) -> Result<u16, String> {
    types::type_by_name(t)
        .map(|t| t.code)
        .or_else(|| u16::from_str_radix(t.trim_start_matches("X'").trim_end_matches('\''), 16).ok())
        .ok_or_else(|| format!("CPD0084 Object type {t} is not valid."))
}

fn dmpobj(system: &mut System, parms: &Parms) -> R {
    let (name, lib) = parms.qual("OBJ");
    let (name, lib) = (name.to_ascii_uppercase(), lib.to_ascii_uppercase());
    let t = parms.name("OBJTYPE");
    let code = type_code(&t)?;
    let what = format!("{}/{name} {t}", if lib.is_empty() { "*LIBL" } else { &lib });
    let lines = dumps::with_store(system, |txn, qsys| {
        let names = crate::svc::ile::name_context(txn, qsys);
        let id = wm::obj::find(txn, qsys, &names, &lib, &name, code)
            .map_err(|e| format!("CPF9801 Object {name} type {t} not found: {e}"))?;
        dumps::object_lines(txn, id, &format!("Dump Object  {what}"))
    })?;
    printed(system, &what, &lines)
}

fn dmpsysobj(system: &mut System, parms: &Parms) -> R {
    let obj = parms.text("OBJ").trim().to_ascii_uppercase();
    let ctx = parms.name("CONTEXT");
    let t = parms.name("TYPE");
    let by_id: Option<ObjectId> = obj.parse().ok();
    let what = by_id.map_or_else(|| format!("{ctx}/{obj}"), |id| id.to_string());
    let lines = dumps::with_store(system, |txn, qsys| {
        if let Some(id) = by_id {
            return dumps::object_lines(txn, id, &format!("Dump System Object  {id}"));
        }
        let lib = if ctx.is_empty() || ctx == "QSYS" {
            qsys
        } else {
            let n = qsname::Name::parse(&ctx).map_err(|e| e.to_string())?;
            ops::find_library(txn, qsys, n).map_err(|e| format!("CPF2110 Library {ctx} not found: {e}"))?.id
        };
        let only = if t.is_empty() || t == "*ALL" { None } else { Some(type_code(&t)?) };
        let mut out = Vec::new();
        for (key, id) in ops::list(txn, lib, only).map_err(|e| e.to_string())? {
            if key.name.to_string().trim() == obj {
                out.extend(dumps::object_lines(txn, id, &format!("Dump System Object  {ctx}/{obj}"))?);
                out.push(String::new());
            }
        }
        if out.is_empty() {
            return Err(format!("CPF9801 Object {obj} not found in {ctx}."));
        }
        Ok(out)
    })?;
    printed(system, &what, &lines)
}

/// `RCLDBXREF`: the cross-reference is the files' own dependents
/// tables, so checking walks every `*FILE` of the libraries chosen and
/// reads its description; `*FIX` has nothing more to do than that.
fn rcldbxref(system: &mut System, parms: &Parms) -> R {
    let lib = parms.name("LIB");
    let n = dumps::with_store(system, |txn, qsys| {
        let libs: Vec<ObjectId> = match lib.as_str() {
            "" | "*ALL" | "*ERR" => ops::libraries(txn, qsys)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|(_, id)| id)
                .collect(),
            l => {
                let n = qsname::Name::parse(l).map_err(|e| e.to_string())?;
                vec![ops::find_library(txn, qsys, n).map_err(|e| format!("CPF2110 Library {l} not found: {e}"))?.id]
            }
        };
        let mut n = 0u64;
        for l in libs {
            for (_, id) in ops::list(txn, l, Some(types::FILE)).map_err(|e| e.to_string())? {
                ops::describe(txn, id).map_err(|e| format!("CPF3294 File {id} is damaged: {e}"))?;
                n += 1;
            }
        }
        Ok(n)
    })?;
    let n = n.to_string();
    Ok(done(
        format!("Database cross-reference checked: {n} files.\n"),
        "CPC39DF",
        &[(&n, 10)],
    ))
}

fn kib(parms: &Parms, kwd: &str, dft: u64) -> u64 {
    parms.int(kwd).and_then(|n| u64::try_from(n).ok()).unwrap_or(dft)
}

fn strtrc(system: &mut System, parms: &Parms) -> R {
    let ssnid = parms.name("SSNID");
    let jobs = job_number(system, parms, "JOB")?.map_or_else(Vec::new, |n| vec![n]);
    let t = trace::Trace::new(
        &ssnid,
        trace::Kind::Job,
        jobs,
        trace::Detail::parse(&parms.name("JOBTRCTYPE")),
        kib(parms, "MAXSTG", 10_000),
        parms.name("TRCFULL") != "*STOPTRC",
    );
    trace::start(t)?;
    Ok(done(format!("Trace {ssnid} started.\n"), "CPC39D5", &[(&ssnid, 10)]))
}

/// The trace report as `QPSRVTRCJ`.
fn print_trace(system: &mut System, t: &trace::Trace) -> Result<(), String> {
    let lines = trace::report(t, &|n| dumps::job_name(system, n));
    crate::svc::security::print(system, "QPSRVTRCJ", &lines).map(|_| ())
}

fn endtrc(system: &mut System, parms: &Parms) -> R {
    let ssnid = parms.name("SSNID");
    let n = trace::end(&ssnid)?;
    let t = trace::get(&ssnid).expect("ended above");
    if parms.name("PRTTRC") == "*YES" {
        print_trace(system, &t)?;
    }
    if parms.name("DTAOPT") == "*DLT" {
        trace::delete(&ssnid);
    } else {
        let lib = parms.name("DTALIB");
        let lib = if lib.is_empty() { "QGPL".to_string() } else { lib };
        if parms.name("REPLACE") == "*NO" && prblog::load_text(system, &lib, &ssnid)?.is_some() {
            return Err(format!("CPF39D0 Trace data {ssnid} already exists in {lib}."));
        }
        prblog::store_text(system, &lib, &ssnid, &t.encode(), "Trace data")?;
        trace::delete(&ssnid);
    }
    Ok(done(
        format!("Trace {ssnid} ended with {n} records.\n"),
        "CPC39D6",
        &[(&ssnid, 10), (&n.to_string(), 10)],
    ))
}

fn prttrc(system: &mut System, parms: &Parms) -> R {
    let mbr = parms.name("DTAMBR");
    let lib = parms.name("DTALIB");
    let lib = if lib.is_empty() { "QGPL".to_string() } else { lib };
    let t = match prblog::load_text(system, &lib, &mbr)?.and_then(|s| trace::Trace::decode(&s)) {
        Some(t) => t,
        None => trace::get(&mbr)
            .ok_or_else(|| format!("CPF39DB Trace data {mbr} in library {lib} not found."))?,
    };
    print_trace(system, &t)?;
    if parms.name("DLTTRC") == "*YES" {
        trace::delete(&mbr);
    }
    Ok(done(format!("Trace data {mbr} printed.\n"), "CPC39D7", &[(&mbr, 10)]))
}

fn trcjob(system: &mut System, parms: &Parms) -> R {
    let me = system.current_job;
    let name = format!("TRC{me:06}");
    let set = parms.name("SET");
    match set.as_str() {
        "*OFF" | "*END" => {
            trace::end(&name).map_err(|_| "CPF39D1 No job trace is on in this job.".to_string())?;
            if set == "*OFF"
                && let Some(t) = trace::get(&name)
            {
                print_trace(system, &t)?;
            }
            trace::delete(&name);
        }
        _ => {
            let t = trace::Trace::new(
                &name,
                trace::Kind::Job,
                vec![me],
                trace::Detail::parse(&parms.name("TRCTYPE")),
                kib(parms, "MAXSTG", 16_000),
                parms.name("TRCFULL") != "*STOPTRC",
            );
            trace::start(t)?;
        }
    }
    let shown = if set.is_empty() { "*ON".to_string() } else { set };
    Ok(done(format!("Job trace set {shown}.\n"), "CPC39DD", &[(&shown, 4)]))
}

fn trcint(system: &mut System, parms: &Parms) -> R {
    let tbl = parms.name("TRCTBL");
    let tbl = if tbl.is_empty() { "QSTRCTBL".to_string() } else { tbl };
    match parms.name("SET").as_str() {
        "*OFF" => {
            trace::end(&tbl)?;
            let t = trace::get(&tbl).expect("ended above");
            let lines = trace::report(&t, &|n| dumps::job_name(system, n));
            crate::svc::security::print(system, "QPCSMPRT", &lines)?;
            trace::delete(&tbl);
            Ok(text(format!("Internal trace {tbl} printed and ended.\n")))
        }
        "*HOLD" => {
            let n = trace::end(&tbl)?;
            Ok(text(format!("Internal trace {tbl} held with {n} records.\n")))
        }
        "*END" => {
            trace::delete(&tbl).ok_or_else(|| format!("CPF39D1 Trace {tbl} not found."))?;
            Ok(text(format!("Internal trace {tbl} ended.\n")))
        }
        _ => {
            let job = parms.text("JOBNBR");
            let jobs = match job.trim() {
                "" | "*ALL" => Vec::new(),
                n => vec![n.parse().map_err(|_| format!("CPD0084 Job number {n} is not valid."))?],
            };
            let mut t = trace::Trace::new(
                &tbl,
                trace::Kind::Internal,
                jobs,
                trace::Detail::All,
                kib(parms, "SIZE", 1024),
                parms.name("TRCFULL") != "*STOPTRC",
            );
            for v in parms.list("TRCTYPE") {
                let c = v.source().trim().trim_start_matches('*').to_ascii_uppercase();
                if c != "ALL" && !c.is_empty() {
                    t.categories.push(c);
                }
            }
            trace::start(t)?;
            let _ = system;
            Ok(done(format!("Trace {tbl} started.\n"), "CPC39D5", &[(&tbl, 10)]))
        }
    }
}
