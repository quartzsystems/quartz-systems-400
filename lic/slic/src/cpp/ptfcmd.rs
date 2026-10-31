// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PTF commands on save files: `LODPTF`, `APYPTF`, `RMVPTF`, and the
//! text forms of `DSPPTF` and `DSPPTFCVR`. The work is the `ptf` crate;
//! the PTF database lives in user space `QSYS/QSPTFDB`.

use std::fmt::Write as _;

use cmd::block::Parms;
use ptf::apply::{self, Ctx};
use ptf::package::{self, Package};
use ptf::{Action, PtfDb, Status};
use stg::proto::{CmdMsg, CmdReply};
use store::obj::savf;
use store::obj::{ObjectId, ops, types, usrspc};
use store::txn::Txn;

use super::direct::{failed, in_store};
use super::savcmd::{SvcHooks, trusted_keys};
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "LODPTF" => lodptf(system, parms),
        "APYPTF" => apyptf(system, parms),
        "RMVPTF" => rmvptf(system, parms),
        "DSPPTF" => dspptf(system, parms),
        "DSPPTFCVR" => dspptfcvr(system, parms),
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| failed(name, &e)))
}

/// The PTF database's user space.
const DB: &str = "QSPTFDB";
/// The PTF groups' user space.
const GROUPS: &str = "QSPTFGRP";
/// PTF loaded.
const LOADED: &str = "CPC3604";
/// PTF applied (or marked for the next IPL).
const APPLIED: &str = "CPC3606";
/// PTF removed.
const REMOVED: &str = "CPC3608";

pub(super) fn release_text() -> String {
    let r = release::RELEASE;
    format!("V{}R{}M{}", r.v, r.r, r.m)
}

fn space(txn: &mut Txn<'_>, qsys: ObjectId, name: &str, text: &str, create: bool) -> Result<Option<ObjectId>, String> {
    let name = qsname::Name::parse(name).expect("name");
    let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
    if let Some(id) = lib.lookup(txn, types::USRSPC, name).map_err(|e| e.to_string())? {
        return Ok(Some(id));
    }
    if !create {
        return Ok(None);
    }
    let params = ops::CreateParams {
        library: qsys,
        name,
        type_code: types::USRSPC,
        attribute: String::new(),
        text: text.into(),
        owner: "QSYS".into(),
        system: String::new(),
        now: now_unix(),
        by_system: true,
    };
    usrspc::create(txn, &params, 16, true, 0)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The text a user space in `QSYS` keeps (a length, then the bytes);
/// empty when there is none.
pub(super) fn space_text(system: &mut System, name: &str) -> Result<String, String> {
    in_store(system, |txn, qsys, _| {
        let Some(id) = space(txn, qsys, name, "", false)? else {
            return Ok(String::new());
        };
        let len = usrspc::read(txn, id, 0, 8).map_err(|e| e.to_string())?;
        let len = u64::from_be_bytes(len.try_into().expect("8"));
        let text = usrspc::read(txn, id, 8, usize::try_from(len).unwrap_or(0))
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&text).into_owned())
    })
}

/// Keep `text` in user space `QSYS/name`, made when missing.
pub(super) fn store_space_text(system: &mut System, name: &str, desc: &str, text: &str) -> Result<(), String> {
    in_store(system, |txn, qsys, _| {
        let id = space(txn, qsys, name, desc, true)?.expect("created");
        usrspc::write(txn, id, 0, &(text.len() as u64).to_be_bytes()).map_err(|e| e.to_string())?;
        usrspc::write(txn, id, 8, text.as_bytes()).map_err(|e| e.to_string())
    })
}

/// The PTF database.
pub fn load_db(system: &mut System) -> Result<PtfDb, String> {
    space_text(system, DB).map(|t| PtfDb::decode(&t))
}

/// Keep the PTF database.
pub fn store_db(system: &mut System, db: &PtfDb) -> Result<(), String> {
    store_space_text(system, DB, "PTF database", &db.encode())
}

/// The PTF groups the system knows (user space `QSYS/QSPTFGRP`).
pub(super) fn load_groups(system: &mut System) -> Result<ptf::grpdb::GroupDb, String> {
    space_text(system, GROUPS).map(|t| ptf::grpdb::GroupDb::decode(&t))
}

/// Keep the PTF groups.
pub(super) fn store_groups(system: &mut System, g: &ptf::grpdb::GroupDb) -> Result<(), String> {
    store_space_text(system, GROUPS, "PTF groups", &g.encode())
}

pub(super) fn list(parms: &Parms, kwd: &str) -> Vec<String> {
    parms
        .list(kwd)
        .iter()
        .map(|v| v.source().trim().to_ascii_uppercase())
        .filter(|v| !v.is_empty() && !v.starts_with('*'))
        .collect()
}

/// The stream of a PTF's save file, `QGPL/QSnnnnn` unless named.
pub(super) fn read_ptf(system: &mut System, savf_q: &str) -> Result<Vec<u8>, String> {
    let (lib, name) = savf_q.split_once('/').unwrap_or(("QGPL", savf_q));
    let (lib, name) = (lib.to_string(), name.to_string());
    in_store(system, |txn, qsys, names| {
        let id = wm::obj::find(txn, qsys, names, &lib, &name, types::SAVF)
            .map_err(|_| format!("CPF9812 save file {lib}/{name} not found"))?;
        savf::read(txn, id).map_err(|e| e.to_string())
    })
}

/// Every PTF save file in `QGPL` (`SELECT(*ALL)`).
pub(super) fn ptf_savfs(system: &mut System) -> Result<Vec<String>, String> {
    in_store(system, |txn, qsys, _| {
        let l = ops::find_library(txn, qsys, qsname::Name::parse("QGPL").expect("name"))
            .map_err(|e| e.to_string())?;
        Ok(ops::list(txn, l.id, Some(types::SAVF))
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(k, _)| k.name.to_string())
            .filter(|n| n.parse::<release::Ptf>().is_ok())
            .collect())
    })
}

pub(super) fn require() -> Result<(), String> {
    crate::stgsvc::security::require(sec::profile::SpecialAuth::ALLOBJ, "work with PTFs")
}

pub(super) fn msg(id: &str, kind: u8, fields: &[(&str, usize)]) -> CmdMsg {
    CmdMsg {
        msgid: id.into(),
        kind,
        data: cmd::error::data(fields),
    }
}

fn lodptf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require()?;
    let product = parms.text("LICPGM").to_ascii_uppercase();
    let dev = parms.text("DEV").to_ascii_uppercase();
    if dev != "*SAVF" {
        return Err(format!("CPF9814 device {dev} not found"));
    }
    let omit = list(parms, "OMIT");
    let mut select = list(parms, "SELECT");
    if select.is_empty() {
        select = ptf_savfs(system)?;
    }
    let mut db = load_db(system)?;
    let keys = trusted_keys();
    let mut reply = CmdReply::default();
    let mut loaded = 0;
    for p in select.iter().filter(|p| !omit.contains(p)) {
        let bytes = read_ptf(system, p)?;
        let pkg = package::open(&bytes, &keys).map_err(|e| format!("{} {p}: {e}", e.msgid()))?;
        if pkg.header.product != product {
            continue;
        }
        apply::load(&mut db, &pkg, &format!("QGPL/{p}"), &release_text(), now_unix())
            .map_err(|e| format!("{} {e}", e.msgid()))?;
        reply.messages.push(msg(LOADED, 4, &[(p, 7), (&product, 7)]));
        let _ = writeln!(reply.text, "{LOADED} PTF {p} {product} loaded");
        loaded += 1;
    }
    store_db(system, &db)?;
    if loaded == 0 {
        return Err(format!("CPF3606 no PTFs of {product} to load"));
    }
    Ok(reply)
}

/// The PTFs a command selects: the list, or every one of the product
/// in the states `want` allows.
fn chosen(db: &PtfDb, parms: &Parms, product: &str, want: &[Status]) -> Vec<String> {
    let omit = list(parms, "OMIT");
    let mut sel = list(parms, "SELECT");
    if sel.is_empty() {
        sel = db
            .of_product(product)
            .filter(|r| want.contains(&r.status))
            .map(|r| r.ptf.clone())
            .collect();
    }
    sel.retain(|p| !omit.contains(p));
    sel
}

pub(super) fn ctx_run<T>(
    system: &mut System,
    f: impl FnOnce(&mut Ctx<'_>) -> T,
) -> Result<T, String> {
    let job = store::obj::JobHandle(u64::from(system.current_job));
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    let hooks = SvcHooks::new(qsys);
    let mut cx = Ctx {
        store: &mut s.store,
        job,
        qsys,
        hooks: &hooks,
        now: now_unix(),
    };
    Ok(sec::guard::as_system(|| f(&mut cx)))
}

pub(super) fn package_of(system: &mut System, db: &PtfDb, ptf: &str) -> Result<(Package, Vec<u8>), String> {
    let r = db.get(ptf).ok_or_else(|| format!("CPF3606 PTF {ptf} is not loaded"))?;
    let bytes = read_ptf(system, &r.savf)?;
    let pkg = package::open(&bytes, &trusted_keys()).map_err(|e| format!("{} {e}", e.msgid()))?;
    Ok((pkg, bytes))
}

fn apyptf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require()?;
    let product = parms.text("LICPGM").to_ascii_uppercase();
    let perm = parms.text("APY") == "*PERM";
    let delayed = parms.text("DELAYED") == "*YES";
    let mut db = load_db(system)?;
    let mut want = vec![Status::Loaded, Status::TempRemoved];
    if perm {
        want.push(Status::TempApplied);
    }
    let batch = chosen(&db, parms, &product, &want);
    let mut reply = CmdReply::default();
    for p in &batch {
        let status = db.get(p).map(|r| r.status);
        if status != Some(Status::TempApplied) {
            if delayed {
                apply::check_apply(&db, p, &batch).map_err(|e| format!("{} {e}", e.msgid()))?;
                let r = db.ptfs.get_mut(p).expect("checked");
                r.action = Action::ApplyAtIpl;
            } else {
                let (pkg, bytes) = package_of(system, &db, p)?;
                ctx_run(system, |cx| apply::apply_temp(cx, &mut db, &pkg, &bytes, &batch, false))?
                    .map_err(|e| format!("{} {e}", e.msgid()))?;
            }
        }
        if perm && db.get(p).is_some_and(|r| r.status == Status::TempApplied) {
            ctx_run(system, |cx| apply::apply_perm(cx, &mut db, p))?
                .map_err(|e| format!("{} {e}", e.msgid()))?;
        }
        let state = db.get(p).map_or("", |r| {
            if r.action == Action::None {
                r.status.display()
            } else {
                r.action.display()
            }
        });
        let _ = writeln!(reply.text, "{APPLIED} PTF {p} {product}: {state}");
        reply.messages.push(msg(APPLIED, 4, &[(p, 7), (&product, 7)]));
    }
    store_db(system, &db)?;
    Ok(reply)
}

fn rmvptf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require()?;
    let product = parms.text("LICPGM").to_ascii_uppercase();
    let perm = parms.text("RMV") == "*PERM";
    let mut db = load_db(system)?;
    let batch = chosen(&db, parms, &product, &[Status::TempApplied]);
    let mut reply = CmdReply::default();
    for p in &batch {
        ctx_run(system, |cx| apply::remove(cx, &mut db, p, perm))?
            .map_err(|e| format!("{} {e}", e.msgid()))?;
        if perm && let Some(r) = db.get(p) {
            let savf_q = r.savf.clone();
            let (lib, name) = savf_q.split_once('/').unwrap_or(("QGPL", &savf_q));
            let (lib, name) = (lib.to_string(), name.to_string());
            let _ = in_store(system, |txn, qsys, names| {
                let id = wm::obj::find(txn, qsys, names, &lib, &name, types::SAVF)
                    .map_err(|e| e.to_string())?;
                ops::delete(txn, store::obj::JobHandle(0), id).map_err(|e| e.to_string())
            });
        }
        let _ = writeln!(reply.text, "{REMOVED} PTF {p} {product} removed");
        reply.messages.push(msg(REMOVED, 4, &[(p, 7), (&product, 7)]));
    }
    store_db(system, &db)?;
    Ok(reply)
}

fn dspptf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let product = parms.text("LICPGM").to_ascii_uppercase();
    let db = load_db(system)?;
    let sel = list(parms, "SELECT");
    let mut text = format!(
        "Product {} release {}  IPL source ##MACH#A\n",
        if product.is_empty() || product == "*ALL" {
            release::PRODUCT_ID.to_string()
        } else {
            product.clone()
        },
        release_text()
    );
    let rows: Vec<_> = db
        .ptfs
        .values()
        .filter(|r| product.is_empty() || product == "*ALL" || r.product == product)
        .filter(|r| sel.is_empty() || sel.contains(&r.ptf))
        .collect();
    if rows.is_empty() {
        text.push_str("No PTFs.\n");
    }
    for r in rows {
        let _ = writeln!(
            text,
            "  {}  {:<7}  {:<24} {}",
            r.ptf,
            r.product,
            r.status.display(),
            r.action.display()
        );
    }
    Ok(CmdReply {
        text,
        ..CmdReply::default()
    })
}

fn dspptfcvr(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let product = parms.text("LICPGM").to_ascii_uppercase();
    let db = load_db(system)?;
    let sel = list(parms, "SELECT");
    let mut text = String::new();
    let ids: Vec<String> = db
        .ptfs
        .values()
        .filter(|r| product.is_empty() || product == "*ALL" || r.product == product)
        .filter(|r| sel.is_empty() || sel.contains(&r.ptf))
        .map(|r| r.ptf.clone())
        .collect();
    for p in ids {
        if let Ok((pkg, _)) = package_of(system, &db, &p) {
            let _ = writeln!(text, "PTF {p} {}\n{}\n", pkg.header.product, pkg.cover.trim_end());
        }
    }
    if text.is_empty() {
        text.push_str("No cover letters.\n");
    }
    Ok(CmdReply {
        text,
        ..CmdReply::default()
    })
}

/// At IPL: apply the delayed PTFs and remove those marked for the next
/// IPL (`docs/PTF.md` §3, §5). LIC PTFs are the IPL's slot switch and
/// are left to it. What was done, for the IPL's message.
pub fn ipl_pending(system: &mut System) -> String {
    let Ok(mut db) = load_db(system) else {
        return String::new();
    };
    let pending: Vec<(String, Action)> = apply::pending(&db)
        .into_iter()
        .filter(|(p, _)| db.get(p).is_some_and(|r| !r.lic))
        .collect();
    if pending.is_empty() {
        return String::new();
    }
    let (mut applied, mut removed, mut failed_n) = (0, 0, 0);
    for (p, action) in &pending {
        let ok = match action {
            Action::ApplyAtIpl => {
                system.emit(&crate::src::PTF_APPLY);
                package_of(system, &db, p)
                    .and_then(|(pkg, bytes)| {
                        ctx_run(system, |cx| apply::apply_temp(cx, &mut db, &pkg, &bytes, &[], true))?
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    })
                    .inspect(|()| applied += 1)
            }
            Action::RemoveAtIpl => {
                system.emit(&crate::src::PTF_REMOVE);
                if let Some(r) = db.ptfs.get_mut(p) {
                    // The remove itself, now that no job runs.
                    r.delayed = false;
                }
                ctx_run(system, |cx| apply::remove(cx, &mut db, p, false))
                    .and_then(|r| r.map_err(|e| e.to_string()))
                    .inspect(|()| removed += 1)
            }
            Action::None => Ok(()),
        };
        if ok.is_err() {
            failed_n += 1;
            system.emit(&crate::src::PTF_FAILED);
            if let Some(r) = db.ptfs.get_mut(p) {
                r.status = Status::Damaged;
                r.action = Action::None;
            }
        }
    }
    let _ = store_db(system, &db);
    format!("{applied} PTF(s) applied, {removed} removed, {failed_n} failed at IPL")
}
