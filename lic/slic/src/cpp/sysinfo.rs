// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System information commands: `DSPSFWRSC`, `DSPPTF` from the release
//! crate; `DSPNETA`, `CHGNETA` on the network attributes; `DSPDEVD`,
//! `CHGDEVD`, `VRYCFG`, `WRKCFGSTS` on the `*DEVD` objects.

use std::fmt::Write as _;

use cmd::block::Parms;
use stg::proto::CmdReply;
use store::obj::library::Library;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;
use wm::obj::devd::Devd;

use super::direct::{failed, in_store, old, plain};
use crate::ipl::system::System;
use crate::svc::neta;

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "DSPSFWRSC" => Ok(dspsfwrsc()),
        "DSPPTF" => Ok(dspptf()),
        "DSPNETA" => dspneta(system),
        "CHGNETA" => chgneta(system, parms),
        "DSPDEVD" => dspdevd(system, parms),
        "CHGDEVD" => chgdevd(system, parms),
        "VRYCFG" => vrycfg(system, parms),
        "WRKCFGSTS" => wrkcfgsts(system, parms),
        _ => return None,
    };
    Some(match r {
        Ok(text) => plain(text),
        Err(e) => failed(name, &e),
    })
}

/// `DSPSFWRSC`: the one licensed program, the operating system.
fn dspsfwrsc() -> String {
    format!(
        "Resource ID  Option  Feature  Release  Description\n{:<11}  *BASE   5050     {:<7}  {}\n{:<11}  *BASE   2924     {:<7}  {} (English)\n",
        release::PRODUCT_ID,
        release::RELEASE.to_string(),
        release::PRODUCT_TEXT,
        release::PRODUCT_ID,
        release::RELEASE.to_string(),
        release::PRODUCT_TEXT,
    )
}

/// `DSPPTF`: no PTF has been applied to this release.
fn dspptf() -> String {
    format!(
        "Product {} release {}: no PTFs applied (cumulative package *NONE, IPL source ##MACH#A).\n",
        release::PRODUCT_ID,
        release::RELEASE
    )
}

fn dspneta(system: &mut System) -> Result<String, String> {
    let sysname = system.system_name();
    in_store(system, |txn, qsys, _| {
        let map = neta::read(txn, qsys, &sysname)?;
        let mut out = String::from("Network attributes:\n");
        for (k, _) in neta::DEFAULTS {
            let _ = writeln!(out, "  {k:<12} {}", map.get(*k).map_or("", String::as_str));
        }
        Ok(out)
    })
}

/// `CHGNETA`: every keyword given replaces its attribute; `SYSNAME`
/// takes effect at once (the machine waits for the next IPL) and sets
/// `QSYSNAME` with it.
fn chgneta(system: &mut System, parms: &Parms) -> Result<String, String> {
    let sysname = system.system_name();
    let mut changes: Vec<(String, String)> = Vec::new();
    for (k, _) in neta::DEFAULTS {
        if !parms.specified(k) {
            continue;
        }
        let v = match *k {
            "MSGQ" | "OUTQ" => {
                let (n, l) = parms.qual(k);
                if n.starts_with('*') {
                    n.to_ascii_uppercase()
                } else {
                    format!("{}/{}", if l.is_empty() { "*LIBL" } else { &l }, n).to_ascii_uppercase()
                }
            }
            _ => parms.text(k).to_ascii_uppercase(),
        };
        if v == "*SAME" {
            continue;
        }
        changes.push(((*k).to_string(), v));
    }
    if changes.is_empty() {
        return Err("CPD0084 CHGNETA: no attribute given".into());
    }
    let new_sysname = changes
        .iter()
        .find(|(k, _)| k == "SYSNAME")
        .map(|(_, v)| v.clone());
    let n = changes.len();
    in_store(system, |txn, qsys, _| {
        let mut map = neta::read(txn, qsys, &sysname)?;
        for (k, v) in changes {
            map.insert(k, v);
        }
        neta::write(txn, qsys, &map)
    })?;
    if let Some(s) = new_sysname {
        let r = old(system, &format!("sysval QSYSNAME {s}"));
        if let Some(id) = super::leading_msgid(r.trim()) {
            return Err(format!("{id} QSYSNAME could not be set: {r}"));
        }
    }
    Ok(format!("{n} network attribute(s) changed.\n"))
}

/// A device description in `QSYS`.
fn device(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Result<(ObjectId, Devd), String> {
    let id = wm::obj::find(
        txn,
        qsys,
        &ops::NameContext::default(),
        "QSYS",
        name,
        types::DEVD,
    )
    .map_err(|_| format!("CPF2702 device description {name} not found"))?;
    let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
    let d = Devd::decode(&b).map_err(|e| e.to_string())?;
    Ok((id, d))
}

fn describe(name: &str, d: &Devd) -> String {
    format!(
        "Device description {name}:\n  Device class . . . : {}\n  Device type  . . . : {}\n  Device model . . . : {}\n  Online at IPL  . . : {}\n  Status . . . . . . : {}\n  Output queue . . . : {}\n  Text . . . . . . . : {}\n",
        d.devcls,
        d.devtype,
        d.model,
        if d.online { "*YES" } else { "*NO" },
        if d.online { "VARIED ON" } else { "VARIED OFF" },
        if d.outq.is_empty() { "*NONE" } else { &d.outq },
        d.text
    )
}

fn dspdevd(system: &mut System, parms: &Parms) -> Result<String, String> {
    let name = parms.name("DEVD");
    in_store(system, |txn, qsys, _| {
        let (_, d) = device(txn, qsys, &name)?;
        Ok(describe(&name, &d))
    })
}

fn chgdevd(system: &mut System, parms: &Parms) -> Result<String, String> {
    let name = parms.name("DEVD");
    let text = parms.specified("TEXT").then(|| parms.text("TEXT"));
    let outq = parms.specified("OUTQ").then(|| {
        let (n, l) = parms.qual("OUTQ");
        if n.starts_with('*') {
            String::new()
        } else {
            format!("{}/{n}", if l.is_empty() { "*LIBL" } else { &l })
        }
    });
    let online = parms.specified("ONLINE").then(|| parms.yes("ONLINE"));
    in_store(system, |txn, qsys, _| {
        let (id, mut d) = device(txn, qsys, &name)?;
        if let Some(t) = text {
            d.text = if t == "*BLANK" { String::new() } else { t };
        }
        if let Some(q) = outq {
            d.outq = q;
        }
        if let Some(o) = online {
            d.online = o;
        }
        wm::obj::write(txn, id, &d.encode()).map_err(|e| e.to_string())?;
        Ok(format!("Device description {name} changed.\n"))
    })
}

/// `VRYCFG CFGOBJ(name) CFGTYPE(*DEV) STATUS(*ON|*OFF)`.
fn vrycfg(system: &mut System, parms: &Parms) -> Result<String, String> {
    let ty = parms.name("CFGTYPE");
    if ty != "*DEV" {
        return Err(format!("CPD0084 CFGTYPE({ty}): only *DEV objects exist here"));
    }
    let on = match parms.name("STATUS").as_str() {
        "*ON" => true,
        "*OFF" => false,
        s => return Err(format!("CPD0084 STATUS({s}) is not *ON or *OFF")),
    };
    let names: Vec<String> = parms
        .list("CFGOBJ")
        .iter()
        .map(|v| v.source().to_ascii_uppercase())
        .collect();
    in_store(system, |txn, qsys, _| {
        for name in &names {
            let (id, mut d) = device(txn, qsys, name)?;
            d.online = on;
            wm::obj::write(txn, id, &d.encode()).map_err(|e| e.to_string())?;
        }
        Ok(format!(
            "{} device(s) varied {}.\n",
            names.len(),
            if on { "on" } else { "off" }
        ))
    })
}

/// `WRKCFGSTS CFGTYPE(*DEV)`: every device description and its status.
fn wrkcfgsts(system: &mut System, parms: &Parms) -> Result<String, String> {
    let ty = parms.name("CFGTYPE");
    if !matches!(ty.as_str(), "*DEV" | "") {
        return Err(format!("CPD0084 CFGTYPE({ty}): only *DEV objects exist here"));
    }
    let want = parms.name("CFGD");
    in_store(system, |txn, qsys, _| {
        let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let mut out = String::from("Description  Class  Type   Model  Status\n");
        for (key, id) in lib.list(txn, Some(types::DEVD)).map_err(|e| e.to_string())? {
            let n = key.name.to_string();
            if !want.is_empty() && want != "*ALL" && want != n && !matches_generic(&want, &n) {
                continue;
            }
            let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
            let d = Devd::decode(&b).map_err(|e| e.to_string())?;
            let _ = writeln!(
                out,
                "{n:<11}  {:<5}  {:<5}  {:<5}  {}",
                d.devcls,
                d.devtype,
                d.model,
                if d.online { "VARIED ON" } else { "VARIED OFF" }
            );
        }
        Ok(out)
    })
}

/// `ABC*` matches names starting with `ABC`.
fn matches_generic(pattern: &str, name: &str) -> bool {
    pattern
        .strip_suffix('*')
        .is_some_and(|p| name.starts_with(p))
}
