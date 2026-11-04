// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Printer files in the service: resolving one (a `*FILE` whose
//! attribute is `PRTF`) with the `OVRPRTF`s that reach the opener,
//! opening its spooled file, and the output queue the file goes to.

use store::obj::{ops, types};
use wm::cmd::Params;
use wm::obj::prtf::Prtf;
use wm::obj::splf::SplfAttrs;
use wm::table::JobInfo;

use crate::ipl::system::System;

/// One `OVRPRTF`: the file it names, the activation group that issued
/// it (or `*JOB`), and its keywords as typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrtOverride {
    /// `FILE`.
    pub file: String,
    /// `OVRSCOPE`: `*JOB`, or the issuing activation group.
    pub scope: String,
    /// The call level that issued it; 0 at a command line.
    pub level: u32,
    /// `SECURE(*YES)`: lower call levels' overrides are not merged.
    pub secure: bool,
    /// The command line, for its keywords.
    pub line: String,
}

/// Whether `lib/name` is a printer file; its real library when it is.
fn find(
    txn: &mut store::txn::Txn<'_>,
    qsys: store::obj::ObjectId,
    qual: &str,
) -> Result<(store::obj::ObjectId, String), String> {
    let (lib, name) = wm::obj::params::qualified(qual);
    let names = crate::svc::ile::name_context(txn, qsys);
    let id = wm::obj::find(txn, qsys, &names, &lib, &name, types::FILE)
        .map_err(|_| format!("CPF4101 file {name} in library {lib} not found"))?;
    let (_, h) = ops::read_header(txn, id).map_err(|e| e.to_string())?;
    if ccsid::decode_trimmed(&h.attribute) != "PRTF" {
        return Err(format!(
            "CPF4123 file {name} in library {lib} is not a printer file"
        ));
    }
    let real = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
    Ok((id, format!("{real}/{name}")))
}

/// A printer file, `lib/name`, with `overrides` applied in order, and
/// its real `lib/name`.
pub fn resolve_real(
    system: &mut System,
    qual: &str,
    overrides: &[Params],
) -> Result<(Prtf, String), String> {
    let (mut f, real) = super::spool::with_store(system, |txn, qsys| {
        let (id, real) = find(txn, qsys, qual)?;
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        Ok((Prtf::decode(&b).map_err(|e| e.to_string())?, real))
    })?;
    for o in overrides {
        f.apply(o)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPD0084")))?;
    }
    Ok((f, real))
}

/// [`resolve_real`] without the name.
pub fn resolve(system: &mut System, qual: &str, overrides: &[Params]) -> Result<Prtf, String> {
    resolve_real(system, qual, overrides).map(|r| r.0)
}

/// Whether an output queue exists.
fn exists(system: &mut System, q: &str) -> bool {
    super::spool::with_store(system, |txn, qsys| super::spool::outq_object(txn, qsys, q)).is_ok()
}

/// The output queue a file of job `job` opened through `f` goes to:
/// the file's `OUTQ`, else the job's, else the printer device's
/// (`QUSRSYS/<device>`), else `QGPL/QPRINT`.
pub fn outq_of(system: &mut System, f: &Prtf, job: u32) -> String {
    outq_checked(system, f, job).0
}

/// [`outq_of`] and the queues named that were not found.
pub fn outq_checked(system: &mut System, f: &Prtf, job: u32) -> (String, Vec<String>) {
    let (job_q, job_dev) = system
        .attrs
        .get(&job)
        .map(|a| (a.outq.clone(), a.prtdev.clone()))
        .unwrap_or_default();
    let qprtdev = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QPRTDEV").ok())
        .map(|v| v.text())
        .unwrap_or_default();
    let dev = wm::obj::prtf::device_of(&f.dev, &job_dev, &qprtdev);
    let dev_q = if dev.is_empty() { String::new() } else { format!("QUSRSYS/{dev}") };
    let mut known = std::collections::BTreeMap::new();
    for q in [f.outq.as_str(), job_q.as_str(), dev_q.as_str()] {
        if !q.is_empty() && !q.starts_with('*') && !known.contains_key(q) {
            known.insert(q.to_string(), exists(system, q));
        }
    }
    wm::obj::prtf::pick_outq(&f.outq, &job_q, &dev_q, |q| known.get(q).copied().unwrap_or(false))
}

/// `CPF3357` for each queue a printer file or job named that was not
/// found, as diagnostics for the job log: the file went to
/// `QGPL/QPRINT` instead.
pub fn missing_outq_entries(missing: &[String]) -> Vec<wm::joblog::Entry> {
    missing
        .iter()
        .map(|q| {
            let (lib, name) = wm::obj::params::qualified(q);
            let data = wm::joblog::data(&[(&name, 10), (&lib, 10)]);
            wm::joblog::Entry {
                msgid: MISSING_OUTQ.into(),
                kind: "Diagnostic".into(),
                severity: 40,
                time: super::msgs::now_micros(),
                from: ("QSPOPNSP".into(), "QSYS".into()),
                from_inst: "*N".into(),
                to: ("*EXT".into(), String::new()),
                to_inst: "*N".into(),
                text: super::hist::message_text(MISSING_OUTQ, &data),
                external: true,
                ..wm::joblog::Entry::default()
            }
        })
        .collect()
}

/// An output queue named for a spooled file was not found.
pub const MISSING_OUTQ: &str = "CPF3357";

/// Open printer file `(file, lib)` for `job` from activation group
/// `actgrp`: the overrides that reach it applied (the last `TOFILE`
/// redirects), a new spooled file. Its identifier, the attributes and
/// the output queues named that were not found.
pub fn open(
    system: &mut System,
    overrides: &[PrtOverride],
    job: &JobInfo,
    (file, lib): (&str, &str),
    actgrp: &str,
) -> Result<(u32, Prtf, Vec<String>), String> {
    let file = file.to_ascii_uppercase();
    // Outer call levels first, the deepest last; a SECURE(*YES) one
    // hides those below it.
    let mut mine: Vec<&PrtOverride> = overrides
        .iter()
        .filter(|o| o.file == file && db::odp::ovrdbf::reaches(&o.scope, actgrp))
        .collect();
    mine.sort_by_key(|o| o.level);
    if let Some(i) = mine.iter().rposition(|o| o.secure) {
        mine.drain(..i);
    }
    // The override's own keywords are not the printer file's.
    let mine: Vec<Params> = mine
        .iter()
        .map(|o| {
            let mut p = Params::parse(&o.line);
            p.kw
                .retain(|(k, _)| !matches!(k.as_str(), "CALLLVL" | "SECURE" | "OVRSCOPE"));
            p
        })
        .collect();
    let target = mine
        .iter()
        .rev()
        .find_map(|p| p.upper("TOFILE"))
        .unwrap_or_else(|| format!("{}/{file}", if lib.is_empty() { "*LIBL" } else { lib }));
    let (prtf, real) = resolve_real(system, &target, &mine)?;
    let (outq, missing) = outq_checked(system, &prtf, job.number);
    let a = system.attrs.get(&job.number);
    let outpty = if prtf.outpty == 0 {
        a.map_or(5, |a| a.outpty)
    } else {
        prtf.outpty
    };
    let attrs = SplfAttrs {
        name: if prtf.splfname == "*FILE" || prtf.splfname.is_empty() {
            file.clone()
        } else {
            prtf.splfname.clone()
        },
        job: (job.name.clone(), job.user.clone(), job.number),
        usrdta: prtf.usrdta.clone(),
        schedule: prtf.schedule,
        copies: prtf.copies.max(1),
        outpty,
        save: prtf.save,
        page_len: prtf.page_len,
        page_width: prtf.page_width,
        lpi: prtf.lpi,
        cpi: prtf.cpi,
        ovrflw: prtf.ovrflw,
        drawer: prtf.drawer,
        duplex: prtf.duplex,
        prtqlty: prtf.prtqlty,
        formtype: prtf.formtype.clone(),
        outq,
        devtype: prtf.devtype.clone(),
        font: prtf.font.clone(),
        prtf: real,
        ..SplfAttrs::default()
    };
    let id = super::spool::create(system, attrs)?;
    Ok((id, prtf, missing))
}
