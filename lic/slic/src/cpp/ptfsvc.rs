// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PTF commands over packages: `WRKPTFGRP` (text), `INSPTF`,
//! `CPYPTF` and Quartz's `QSCRTPTF`. The work is the `ptf` crate
//! (`install`, `grpdb`, `manifest`, `package`).

use std::fmt::Write as _;

use cmd::block::Parms;
use ifs::Errno;
use ifs::client::{IfsClient, read_all};
use ifs::ops::OpenArgs;
use ifs::ops::flags::O_RDONLY;
use ptf::install::{self, InsType, Item};
use ptf::manifest::{self, Manifest};
use ptf::package::{self, Build};
use ptf::{Action, Status};
use stg::proto::CmdReply;
use store::obj::savf::{self, SavfAttr};
use store::obj::{ops, types};

use super::direct::{failed, in_store};
use super::ptfcmd::{self, list, msg, release_text, require};
use super::savcmd::trusted_keys;
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "WRKPTFGRP" => wrkptfgrp(system, parms),
        "INSPTF" => insptf(system, parms),
        "CPYPTF" => cpyptf(system, parms),
        "QSCRTPTF" => qscrtptf(system, parms),
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| failed(name, &e)))
}

/// `INSPTF` done.
const INSTALLED: &str = "CPC3E05";
/// `CPYPTF` done.
const COPIED: &str = "CPC3E06";
/// `QSCRTPTF` done.
const CREATED: &str = "CPC3E07";
/// A save file that exists and `REPLACE(*NO)`.
const EXISTS: &str = "CPF7302";

/// Whether `name` matches a generic name (`QS99*`) or `*ALL`.
fn matches(name: &str, pattern: &str) -> bool {
    match pattern {
        "" | "*ALL" => true,
        p => p.strip_suffix('*').map_or(name == p, |s| name.starts_with(s)),
    }
}

fn wrkptfgrp(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let pattern = parms.text("PTFGRP").to_ascii_uppercase();
    let db = ptfcmd::load_db(system)?;
    let groups = ptfcmd::load_groups(system)?;
    let rows: Vec<_> = groups
        .rows(&db, &trusted_keys())
        .into_iter()
        .filter(|r| matches(&r.name, &pattern))
        .collect();
    let mut text = format!("Work with PTF Groups\n {:<12} {:>5}  {:<20} {}\n", "PTF group", "Level", "Status", "Text");
    if rows.is_empty() {
        text.push_str(" (No PTF groups)\n");
    }
    for r in rows {
        let _ = write!(text, " {:<12} {:>5}  {:<20} {}", r.name, r.level, r.status.display(), r.text);
        if let Some(n) = r.newer {
            let _ = write!(text, "  (level {n} available)");
        }
        text.push('\n');
    }
    Ok(CmdReply {
        text,
        ..CmdReply::default()
    })
}

/// Make or replace save file `lib/name` with `bytes`.
pub(super) fn put_savf(
    system: &mut System,
    lib: &str,
    name: &str,
    text: &str,
    bytes: &[u8],
    replace: bool,
) -> Result<(), String> {
    let (lib, name) = (lib.to_string(), name.to_string());
    in_store(system, |txn, qsys, _| {
        let l = ops::find_library(txn, qsys, qsname::Name::parse(&lib).map_err(|e| format!("CPD0074 {e}"))?)
            .map_err(|e| format!("CPF2110 {lib}: {e}"))?
            .id;
        let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0074 {e}"))?;
        let found = store::obj::library::Library::load(txn, l)
            .map_err(|e| e.to_string())?
            .lookup(txn, types::SAVF, n)
            .map_err(|e| e.to_string())?;
        let id = match found {
            Some(_) if !replace => return Err(format!("{EXISTS} save file {lib}/{name} exists; REPLACE(*YES) replaces it")),
            Some(id) => id,
            None => savf::create(
                txn,
                &ops::CreateParams {
                    library: l,
                    name: n,
                    type_code: types::SAVF,
                    attribute: String::new(),
                    text: text.into(),
                    owner: "QSYS".into(),
                    system: String::new(),
                    now: now_unix(),
                    by_system: true,
                },
                SavfAttr::default(),
            )
            .map_err(|e| e.to_string())?,
        };
        savf::write(txn, id, bytes).map_err(|e| e.to_string())
    })
}

/// `PWRDWNSYS RESTART(*YES)` on the system's behalf, as a job's
/// request reaches `slic`.
fn ipl_now() -> Result<(), String> {
    std::fs::write(crate::ipl::power::REQUEST_FILE, "0 600 1\n").map_err(|e| e.to_string())?;
    rustix::process::kill_process(rustix::process::Pid::INIT, rustix::process::Signal::USR1).map_err(|e| e.to_string())
}

/// The PTF save files in `QGPL` an `INSPTF` takes: of the group's
/// members when one is named, of the products, not omitted, not
/// applied or waiting already.
fn items(
    system: &mut System,
    db: &ptf::PtfDb,
    members: Option<&[String]>,
    omit: &[String],
    products: &[String],
    lic_only: bool,
) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    for p in ptfcmd::ptf_savfs(system)? {
        if members.is_some_and(|m| !m.contains(&p)) || omit.contains(&p) {
            continue;
        }
        let done = db.get(&p).is_some_and(|r| {
            r.status.applied() || r.status == Status::Superseded || r.action != Action::None
        });
        if done {
            continue;
        }
        let savf = db
            .get(&p)
            .map(|r| r.savf.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("QGPL/{p}"));
        let bytes = ptfcmd::read_ptf(system, &savf)?;
        let Ok((h, _)) = package::peek(&bytes) else { continue };
        if (!products.is_empty() && !products.contains(&h.product)) || (lic_only && !h.lic) {
            continue;
        }
        items.push(Item { savf, bytes });
    }
    if items.is_empty() {
        return Err("CPF3606 no PTFs to install from the save files in QGPL".into());
    }
    Ok(items)
}

fn insptf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require()?;
    let dev = parms.text("DEV").to_ascii_uppercase();
    if dev != "*SERVICE" && dev != "*SAVF" {
        return Err(format!("CPF9814 device {dev}: PTFs install from save files (*SERVICE, *SAVF) in this release"));
    }
    let ins = InsType::parse(&parms.text("INSTYPE")).unwrap_or(InsType::DelayAll);
    let lic_only = parms.text("PTFTYPE") == "*HIPERLIC";
    let products = list(parms, "LICPGM");
    let omit = list(parms, "OMIT");
    let group = parms.text("PTFGRP").to_ascii_uppercase();
    let mut db = ptfcmd::load_db(system)?;
    let keys = trusted_keys();
    // The PTFs of the group named, from its signed manifest.
    let members: Option<Vec<String>> = if group.is_empty() || group == "*ALL" {
        None
    } else {
        let groups = ptfcmd::load_groups(system)?;
        let (g, ok) = groups
            .groups
            .get(&group)
            .and_then(|k| k.group(&keys))
            .ok_or_else(|| format!("CPF3606 PTF group {group} is not on this system"))?;
        if !ok {
            return Err(format!("CPF3586 PTF group {group}: its manifest does not verify"));
        }
        Some(g.ptfs)
    };
    let items = items(system, &db, members.as_deref(), &omit, &products, lic_only)?;
    let rel = release_text();
    let rep = ptfcmd::ctx_run(system, |cx| install::install(cx, &mut db, &items, &keys, &rel, ins, &omit))?
        .map_err(|e| format!("{} {e}", e.msgid()))?;
    let mut reply = CmdReply::default();
    // One LIC image per IPL: the newest goes to the other slot.
    if let Some(newest) = rep.lic.iter().max().cloned() {
        for p in rep.lic.iter().filter(|p| **p != newest) {
            if let Some(r) = db.ptfs.get_mut(p) {
                r.action = Action::None;
            }
            let _ = writeln!(reply.text, "PTF {p} left loaded: {newest} brings the newer LIC image");
        }
        match super::ptflic::stage(system, &mut db, &newest) {
            Ok(s) => {
                let _ = writeln!(reply.text, "PTF {newest}: its LIC image is in slot {s:?}; the next IPL boots it");
            }
            Err(e) => {
                let _ = writeln!(reply.text, "{e}");
            }
        }
    }
    ptfcmd::store_db(system, &db)?;
    for p in &rep.loaded {
        let _ = writeln!(reply.text, "PTF {p} loaded");
    }
    for p in &rep.applied {
        let _ = writeln!(reply.text, "PTF {p} temporarily applied");
    }
    for p in &rep.at_ipl {
        let _ = writeln!(reply.text, "PTF {p} applied at the next IPL");
    }
    for p in &rep.left {
        let _ = writeln!(reply.text, "PTF {p} loaded; delayed, not applied (*IMMONLY)");
    }
    for (p, id, why) in &rep.failed {
        let _ = writeln!(reply.text, "{id} PTF {p} not installed: {why}");
    }
    let counts = (
        rep.loaded.len().to_string(),
        rep.applied.len().to_string(),
        rep.at_ipl.len().to_string(),
    );
    let _ = writeln!(
        reply.text,
        "{INSTALLED} {} loaded, {} applied, {} at the next IPL",
        counts.0, counts.1, counts.2
    );
    reply
        .messages
        .push(msg(INSTALLED, 4, &[(&counts.0, 5), (&counts.1, 5), (&counts.2, 5)]));
    if ins == InsType::DelayIpl && rep.ipl_needed() {
        ipl_now()?;
        reply.text.push_str("The system is IPLed to apply them.\n");
    }
    Ok(reply)
}

fn cpyptf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require()?;
    let product = parms.text("LICPGM").to_ascii_uppercase();
    let from = parms.text("FROMDEV").to_ascii_uppercase();
    if !from.is_empty() && from != "*SERVICE" && from != "*SAVF" {
        return Err(format!("CPF9814 device {from}: PTFs are copied from their save files in this release"));
    }
    let to = parms.text("TODEV").to_ascii_uppercase();
    if to != "*SAVF" {
        return Err(format!("CPF9814 device {to}: tape and optical devices are not wired yet; use TODEV(*SAVF)"));
    }
    let db = ptfcmd::load_db(system)?;
    let mut sel = list(parms, "SELECT");
    if sel.is_empty() {
        sel = db
            .of_product(&product)
            .filter(|r| !r.savf.is_empty() && r.status != Status::PermRemoved)
            .map(|r| r.ptf.clone())
            .collect();
    }
    let (name, lib) = parms.qual("TOSAVF");
    let lib = if lib.is_empty() { "QGPL".to_string() } else { lib };
    let name = if name.is_empty() { "*PTFID".to_string() } else { name.to_ascii_uppercase() };
    if name != "*PTFID" && sel.len() != 1 {
        return Err(format!(
            "CPD0084 a save file holds one PTF; TOSAVF({lib}/*PTFID) copies {} into save files named after them",
            sel.len()
        ));
    }
    let replace = parms.text("REPLACE") == "*YES";
    let mut reply = CmdReply::default();
    for p in &sel {
        let r = db.get(p).ok_or_else(|| format!("CPF3606 PTF {p} is not on this system"))?;
        if r.product != product {
            return Err(format!("CPF3598 PTF {p} is for {}", r.product));
        }
        let bytes = ptfcmd::read_ptf(system, &r.savf)?;
        let target = if name == "*PTFID" { p.clone() } else { name.clone() };
        if format!("{lib}/{target}") == r.savf {
            return Err(format!("CPD0084 {lib}/{target} is where PTF {p} already is"));
        }
        put_savf(system, &lib, &target, &format!("Copy of PTF {p}"), &bytes, replace)?;
        let _ = writeln!(reply.text, "PTF {p} copied to save file {lib}/{target}");
    }
    let n = sel.len().to_string();
    let _ = writeln!(reply.text, "{COPIED} {n} PTF(s) copied");
    reply.messages.push(msg(COPIED, 4, &[(&n, 5)]));
    Ok(reply)
}

/// A stream file's bytes, read as the job.
fn read_bytes(system: &mut System, path: &str) -> Result<Vec<u8>, String> {
    super::stmf::with_job(system, |c: &dyn IfsClient| {
        let fd = c.open(path, OpenArgs::new(O_RDONLY))?;
        let d = read_all(c, fd);
        let _ = c.close(fd);
        d
    })
    .map_err(|e: Errno| format!("{} {path}: {}", e.command_message(), e.text()))
}

fn qscrtptf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require()?;
    let m = Manifest::parse(&super::stmf::read_as_job(system, &parms.text("MANIFEST"))?)
        .map_err(|e| format!("{} {e}", e.msgid()))?;
    let key = manifest::signing_key(&read_bytes(system, &parms.text("KEYFILE"))?)
        .map_err(|e| format!("{} {e}", e.msgid()))?;
    let cover = if m.cover_file.is_empty() {
        m.cover.clone()
    } else {
        super::stmf::read_as_job(system, &m.cover_file)?
    };
    let lic = if m.lic.is_empty() { None } else { Some(read_bytes(system, &m.lic)?) };
    let lic_b = if m.lic_b.is_empty() { None } else { Some(read_bytes(system, &m.lic_b)?) };
    let now = now_unix();
    let header = m.header(&release_text(), now).map_err(|e| format!("{} {e}", e.msgid()))?;
    let objects = m.objects().map_err(|e| format!("{} {e}", e.msgid()))?;
    let ptf_id = header.ptf.clone();
    let sysname = system.system_name();
    let b = Build {
        header,
        cover,
        objects,
        lic: lic.as_deref(),
        lic_b: lic_b.as_deref(),
        key: &key,
        system: sysname,
    };
    let bytes = {
        let s = system.storage.as_mut().ok_or("object store not open")?;
        let qsys = s.qsys.ok_or("object store not initialized")?;
        sec::guard::as_system(|| package::build(&mut s.store, qsys, &b, now))
            .map_err(|e| format!("{} {e}", e.msgid()))?
    };
    let (name, lib) = parms.qual("TOSAVF");
    let lib = if lib.is_empty() { "QGPL".to_string() } else { lib };
    let name = if name.is_empty() || name == "*PTFID" { ptf_id.clone() } else { name };
    put_savf(system, &lib, &name, &format!("PTF {ptf_id}"), &bytes, parms.text("REPLACE") == "*YES")?;
    let mut reply = CmdReply::default();
    let public = key.verifying_key().to_bytes();
    if !trusted_keys().contains(&public) {
        let _ = writeln!(
            reply.text,
            "The signing key is not one this system trusts; LODPTF refuses the PTF here until its public key is added (docs/PTF.md section 6)"
        );
    }
    let _ = writeln!(reply.text, "{CREATED} PTF {ptf_id} created in save file {lib}/{name}, {} bytes", bytes.len());
    reply.messages.push(msg(CREATED, 4, &[(&ptf_id, 7), (&format!("{lib}/{name}"), 21)]));
    Ok(reply)
}
