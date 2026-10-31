// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGIPLA` and `DSPIPLA`: the IPL attributes in the LIC record
//! (`licpart::ipla`), and what the operating system's start does with
//! them: the queues cleared, the writers and TCP/IP started or not, the
//! restricted state.

use cmd::block::Parms;
use licpart::ipla::{ATTRS, IplAttrs};
use stg::proto::CmdReply;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::{ops, types};

use super::direct::{failed, in_store};
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CHGIPLA" => chgipla(system, parms),
        "DSPIPLA" => Ok(dspipla(system)),
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| failed(name, &e)))
}

/// IPL attributes changed.
const CHANGED: &str = "CPC3E08";
/// The data area `QSTRUP` reads: `STRTCP` then `STRRSTD`, `Y` or `N`.
const QSTRUP_FLAGS: &str = "QSIPLA";

/// The attributes the LIC record holds now.
pub fn current(system: &mut System) -> IplAttrs {
    match system.lic.as_mut() {
        Some((part, record)) => {
            part.refresh(record);
            IplAttrs::of(record)
        }
        None => IplAttrs::default(),
    }
}

fn chgipla(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(sec::profile::SpecialAuth::ALLOBJ, "change the IPL attributes")?;
    let mut a = current(system);
    for attr in ATTRS {
        let v = parms.text(attr.kwd);
        let v = v.trim();
        if !v.is_empty() {
            a.set(attr.kwd, v)
                .map_err(|_| format!("CPD0084 {}({v}) is not valid", attr.kwd))?;
        }
    }
    let (part, record) = system.lic.as_mut().ok_or("CPF3700 the LIC partition is not attached")?;
    part.refresh(record);
    a.store(record).map_err(|e| e.to_string())?;
    part.write(record).map_err(|e| e.to_string())?;
    Ok(super::done(format!("{CHANGED} IPL attributes changed\n"), CHANGED, &[]))
}

fn dspipla(system: &mut System) -> CmdReply {
    CmdReply {
        text: current(system).text(),
        ..CmdReply::default()
    }
}

fn set_flags(system: &mut System, flags: &str) -> Result<(), String> {
    let flags = flags.to_string();
    in_store(system, |txn, qsys, _| {
        let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let n = qsname::Name::parse(QSTRUP_FLAGS).expect("name");
        let v = DataAreaValue::Char(flags);
        match lib.lookup(txn, types::DTAARA, n).map_err(|e| e.to_string())? {
            Some(id) => dtaara::write(txn, id, &v).map_err(|e| e.to_string()),
            None => dtaara::create(
                txn,
                &ops::CreateParams {
                    library: qsys,
                    name: n,
                    type_code: types::DTAARA,
                    attribute: String::new(),
                    text: "IPL attributes the startup program reads".into(),
                    owner: "QSYS".into(),
                    system: String::new(),
                    now: now_unix(),
                    by_system: true,
                },
                DataAreaType::Char { len: 2 },
                &v,
            )
            .map(|_| ())
            .map_err(|e| e.to_string()),
        }
    })
}

/// Every output queue, `lib/name`.
fn outqs(system: &mut System) -> Vec<String> {
    in_store(system, |txn, qsys, _| {
        let mut libs = vec![("QSYS".to_string(), qsys)];
        for (k, id) in ops::list(txn, qsys, Some(types::LIB)).map_err(|e| e.to_string())? {
            libs.push((k.name.to_string(), id));
        }
        let mut out = Vec::new();
        for (l, id) in libs {
            for (k, _) in ops::list(txn, id, Some(types::OUTQ)).map_err(|e| e.to_string())? {
                out.push(format!("{l}/{}", k.name));
            }
        }
        Ok(out)
    })
    .unwrap_or_default()
}

/// At the operating system's start, after the queues are indexed and
/// the system values loaded: what the attributes ask for. A note for
/// the IPL's message, empty when nothing differs from the defaults.
pub fn at_ipl(system: &mut System) -> String {
    let a = current(system);
    let mut notes = Vec::new();
    if a.yes("CLRJOBQ") {
        let qs = crate::stgsvc::jobqs::all_queues(system).unwrap_or_default();
        for (q, _) in &qs {
            let _ = crate::stgsvc::jobqs::queue_op(system, q, "clear");
        }
        notes.push(format!("{} job queue(s) cleared", qs.len()));
    }
    if a.yes("CLROUTQ") {
        let qs = outqs(system);
        for q in &qs {
            let _ = crate::stgsvc::exec::run(system, &format!("clroutq OUTQ({q})"));
        }
        notes.push(format!("{} output queue(s) cleared", qs.len()));
    }
    // CLRINCJOB: job logs waiting to print do not outlive an IPL here.
    if !a.yes("STRPRTWTR")
        && let Some(v) = system.sysvals.as_mut()
    {
        let _ = v.set_system("QSTRPRTWTR", sysval::Value::Char("0".into()));
        notes.push("print writers not started".into());
    }
    let tcp = if a.yes("STRTCP") { 'Y' } else { 'N' };
    let rstd = if a.yes("STRRSTD") { 'Y' } else { 'N' };
    if let Err(e) = set_flags(system, &format!("{tcp}{rstd}")) {
        notes.push(format!("startup flags: {e}"));
    }
    if tcp == 'N' {
        notes.push("TCP/IP not started".into());
    }
    if rstd == 'Y' {
        notes.push("restricted state".into());
    }
    if a.get("HDWDIAG") == Some("*ALL") {
        notes.push("hardware diagnostics *ALL: the resources were enumerated".into());
    }
    notes.join(", ")
}
