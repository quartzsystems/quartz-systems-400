// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Licensed program commands (`docs/LICPGM.md`): `RSTLICPGM`,
//! `DLTLICPGM`, `SAVLICPGM`, `CHKPRDOPT`, `DSPSFWRSC`, the license
//! commands (`WRKLICINF`, `ADDLICKEY`, `RMVLICKEY`, `DSPLICKEY`,
//! `CHGLICINF`), `WRKSFWAGR` and Quartz's `QSPKGPRD`. The model is the
//! `licpgm` crate; the store side `lpstore`.

use std::fmt::Write as _;

use cmd::block::Parms;
use licpgm::directory::check::{Finding, chkprdopt};
use licpgm::directory::{installed_programs, software_resources};
use licpgm::license::key::{KeyFields, LicenseKey};
use licpgm::license::keyfile::{self, KeyRecord};
use licpgm::license::repo::{AddLicKey, ChgLicInf};
use licpgm::model::load::ObjEntry;
use licpgm::plan::delete::{DltLicPgm, Remover, execute_delete, plan_delete};
use licpgm::plan::restore::{RstLicPgm, execute_restore, plan_restore};
use licpgm::plan::save::{SavLicPgm, plan_save};
use licpgm::plan::{NoExits, Objects};
use licpgm::{Date, LicTerm, LoadId, OptionNo, ProductId, ProductStore, QualName, catalog, id};
use lpstore::{Cell, Products, SaveRestorer, SavfMedia, save_product};
use qsname::Name;
use savfmt::{Compression, MediaHeader, SaveKind};
use saverst::AllowAll;
use sec::profile::SpecialAuth;
use stg::proto::CmdReply;
use store::obj::{JobHandle, ObjectId, ops, savf, types};

use super::direct::{failed, in_store, lib_or};
use super::done;
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::licpgm::{self as lp, system_info};
use crate::stgsvc::security::{actor, require};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "RSTLICPGM" => rstlicpgm(system, parms),
        "DLTLICPGM" => dltlicpgm(system, parms),
        "SAVLICPGM" => savlicpgm(system, parms),
        "CHKPRDOPT" => chkprdopt_cmd(system, parms),
        "DSPSFWRSC" => Ok(CmdReply {
            text: dspsfwrsc_text(system),
            ..CmdReply::default()
        }),
        "WRKLICINF" => wrklicinf(system, parms),
        "ADDLICKEY" => addlickey(system, parms),
        "RMVLICKEY" => rmvlickey(system, parms),
        "DSPLICKEY" => dsplickey(system, parms),
        "CHGLICINF" => chglicinf(system, parms),
        "WRKSFWAGR" => Ok(CmdReply {
            text: wrksfwagr_text(system),
            ..CmdReply::default()
        }),
        "QSPKGPRD" => qspkgprd(system, parms),
        _ => return None,
    };
    Some(match r {
        Ok(reply) => reply,
        Err(e) => failed(name, &e),
    })
}

/// A licensed program was installed.
const RESTORED: &str = "CPC3D81";
/// A licensed program was deleted.
const DELETED: &str = "CPC3D82";
/// A licensed program was saved.
const SAVED: &str = "CPC3D85";
/// RSTLICPGM could not install.
const NOT_RESTORED: &str = "CPF3D94";
/// DLTLICPGM could not delete.
const NOT_DELETED: &str = "CPF3D95";
/// SAVLICPGM could not save.
const NOT_SAVED: &str = "CPF3D97";
/// CHKPRDOPT found nothing wrong.
const CHECK_OK: &str = "CPC0C20";
/// CHKPRDOPT found errors.
const CHECK_ERRORS: &str = "CPF0C20";
/// A product or option that is not on the system.
pub const NOT_INSTALLED: &str = "CPF0C1F";
/// A license key added.
const KEY_ADDED: &str = "CPC9E25";
/// License keys removed.
const KEY_REMOVED: &str = "CPC9E26";
/// License information changed.
const INFO_CHANGED: &str = "CPC9E27";
/// A product packaged from a manifest.
const PACKAGED: &str = "CPC0C2A";
/// A manifest that does not check out.
const BAD_MANIFEST: &str = "CPF0C2B";

fn job(system: &System) -> JobHandle {
    JobHandle(
        system
            .jobs
            .get(system.current_job)
            .map_or(u64::from(system.current_job), |j| j.handle),
    )
}

fn user(system: &System) -> String {
    actor()
        .map(|p| p.user.clone())
        .or_else(|| system.svc_user.clone())
        .unwrap_or_else(|| "QSYS".into())
}

fn today() -> Date {
    Date::from_days(i64::try_from(now_unix() / 86_400).unwrap_or(0))
}

/// A product ID as typed, IBM's aliases mapped to Quartz's.
fn product(s: &str) -> Result<ProductId, String> {
    let s = s.trim().to_ascii_uppercase();
    if let Some(p) = catalog::shipped().product(&s) {
        return Ok(p.definition.product.clone());
    }
    s.parse()
        .map_err(|_| format!("{NOT_INSTALLED} product {s} is not valid"))
}

fn option_of(s: &str) -> Result<Option<OptionNo>, String> {
    match s.to_ascii_uppercase().as_str() {
        "*ALL" => Ok(None),
        "" => Ok(Some(OptionNo::BASE)),
        o => o
            .parse()
            .map(Some)
            .map_err(|_| format!("{NOT_INSTALLED} option {o} is not valid")),
    }
}

fn release_of(s: &str) -> Result<Option<release::Release>, String> {
    match s.to_ascii_uppercase().as_str() {
        "" | "*ONLY" | "*FIRST" | "*CUR" => Ok(None),
        r => id::parse_release(r)
            .map(Some)
            .map_err(|_| format!("{NOT_INSTALLED} release {r} is not valid")),
    }
}

fn lng_of(s: &str) -> Result<Option<LoadId>, String> {
    match s.to_ascii_uppercase().as_str() {
        "" | "*PRIMARY" | "*ALL" => Ok(None),
        l => LoadId::nlv(l)
            .map(Some)
            .map_err(|_| format!("{NOT_INSTALLED} language {l} is not valid")),
    }
}

fn objects_of(s: &str) -> Objects {
    match s.to_ascii_uppercase().as_str() {
        "*PGM" => Objects::Pgm,
        "*LNG" => Objects::Lng,
        _ => Objects::All,
    }
}

/// The first `DEV` value; only save files until the media layer comes.
fn device(parms: &Parms) -> Result<(), String> {
    let dev = parms
        .list("DEV")
        .first()
        .map_or_else(|| parms.text("DEV").to_ascii_uppercase(), |v| v.source().to_ascii_uppercase());
    if dev == "*SAVF" {
        Ok(())
    } else {
        // MIGRATE(phase11): optical and tape devices: open the medium
        // and read its product loads as a `licpgm::plan::Media`.
        Err(format!("CPF9814 device {dev} not found"))
    }
}

fn find_savf(system: &mut System, (name, lib): &(String, String)) -> Result<ObjectId, String> {
    in_store(system, |txn, qsys, names| {
        wm::obj::find(txn, qsys, names, &lib_or(lib), name, types::SAVF)
            .map_err(|_| format!("CPF9812 save file {}/{name} not found", lib_or(lib)))
    })
}

fn rstlicpgm(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require(SpecialAuth::ALLOBJ, "install licensed programs")?;
    device(parms)?;
    let pid = product(&parms.text("LICPGM"))?;
    let option = option_of(&parms.text("OPTION"))?.unwrap_or(OptionNo::BASE);
    let req = RstLicPgm {
        product: pid.clone(),
        option,
        release: release_of(&parms.text("RLS"))?,
        lng: lng_of(&parms.text("LNG"))?,
        objects: objects_of(&parms.text("RSTOBJ")),
    };
    let savf_id = find_savf(system, &parms.qual("SAVF"))?;
    let stream = in_store(system, |txn, _, _| savf::read(txn, savf_id).map_err(|e| e.to_string()))?;
    if stream.is_empty() {
        return Err(format!("{NOT_RESTORED} the save file holds no licensed program"));
    }
    let media = SavfMedia::read(stream).map_err(|e| format!("{NOT_RESTORED} {e}"))?;
    let sys = system_info(system);
    let (jobh, who, now) = (job(system), user(system), now_unix());
    let done_keys = lp::with(system, |p, cell, qsys| {
        let plan = plan_restore(p, &media, &sys, std::slice::from_ref(&req))
            .map_err(|e| format!("{NOT_RESTORED} {e}"))?;
        // Running RSTLICPGM is accepting the agreement; GO LICPGM shows
        // it first.
        if !plan.agreements.is_empty() {
            let mut book = p.agreements();
            for (prd, rel) in &plan.agreements {
                book.accept(prd, *rel, &who, today());
            }
            p.save_agreements(&book).map_err(|e| e.to_string())?;
        }
        let mut r = SaveRestorer::new(cell, qsys, jobh, &AllowAll, now);
        let done = execute_restore(&plan, p, &media, &mut r, &mut NoExits, now)
            .map_err(|e| format!("{NOT_RESTORED} {e}"))?;
        Ok(done.into_iter().map(|d| d.key).collect::<Vec<_>>())
    })?;
    let mut text = String::new();
    for k in &done_keys {
        let _ = writeln!(text, "{RESTORED} {k} installed");
    }
    let rel = done_keys
        .first()
        .map(|k| id::release_text(k.release))
        .unwrap_or_default();
    Ok(done(
        text,
        RESTORED,
        &[(pid.as_str(), 7), (&option.to_string(), 5), (&rel, 6)],
    ))
}

/// Deletes for DLTLICPGM in the store; IFS paths are left to the IFS
/// commands (`MIGRATE(phase11)`: remove product directories).
struct StoreRemover<'c, 's> {
    cell: &'c Cell<'s>,
    qsys: ObjectId,
}

impl StoreRemover<'_, '_> {
    fn txn(&self, f: impl FnOnce(&mut store::Txn<'_>) -> store::Result<()>) -> Result<(), String> {
        let mut s = self.cell.borrow_mut();
        let mut t = s.begin().map_err(|e| e.to_string())?;
        match f(&mut t) {
            Ok(()) => t.commit().map_err(|e| e.to_string()),
            Err(e) => {
                t.abort();
                Err(e.to_string())
            }
        }
    }
}

impl Remover for StoreRemover<'_, '_> {
    fn delete_library(&mut self, lib: &str) -> Result<(), String> {
        let qsys = self.qsys;
        let Ok(n) = Name::parse(lib) else {
            return Ok(());
        };
        self.txn(|t| {
            let Ok(found) = ops::find_library(t, qsys, n) else {
                return Ok(());
            };
            for (_, oid) in ops::list(t, found.id, None)? {
                ops::delete(t, JobHandle(0), oid)?;
            }
            ops::delete(t, JobHandle(0), found.id)
        })
    }

    fn delete_object(&mut self, o: &ObjEntry) -> Result<(), String> {
        let qsys = self.qsys;
        let (Ok(l), Ok(n), Some(ty)) = (Name::parse(&o.lib), Name::parse(&o.name), types::type_by_name(&o.typ))
        else {
            return Ok(());
        };
        self.txn(|t| {
            let lib = if o.lib == "QSYS" {
                qsys
            } else {
                match ops::find_library(t, qsys, l) {
                    Ok(f) => f.id,
                    Err(_) => return Ok(()),
                }
            };
            let hit = ops::list(t, lib, Some(ty.code))?
                .into_iter()
                .find(|(k, _)| k.name == n);
            match hit {
                Some((_, id)) => ops::delete(t, JobHandle(0), id),
                None => Ok(()),
            }
        })
    }

    fn delete_path(&mut self, _path: &str) -> Result<(), String> {
        Ok(())
    }
}

fn dltlicpgm(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require(SpecialAuth::ALLOBJ, "delete licensed programs")?;
    let pid = product(&parms.text("LICPGM"))?;
    let opt = parms.text("OPTION");
    let req = DltLicPgm {
        product: pid.clone(),
        option: option_of(if opt.is_empty() { "*ALL" } else { &opt })?,
        release: release_of(&parms.text("RLS"))?,
        lng: lng_of(&parms.text("LNG"))?,
    };
    let n = lp::with(system, |p, cell, qsys| {
        let plan = plan_delete(p, &req).map_err(|e| format!("{NOT_DELETED} {e}"))?;
        let mut rm = StoreRemover { cell, qsys };
        execute_delete(&plan, p, &mut rm, &mut NoExits).map_err(|e| format!("{NOT_DELETED} {e}"))?;
        Ok(plan.loads.len())
    })?;
    Ok(done(
        format!("{DELETED} {n} product load(s) of {pid} deleted\n"),
        DELETED,
        &[(pid.as_str(), 7), (&opt, 5)],
    ))
}

fn header(system: &System, kind: SaveKind) -> MediaHeader {
    let now = now_unix();
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(&now.to_be_bytes());
    id[8..12].copy_from_slice(&system.current_job.to_be_bytes());
    id[12..].copy_from_slice(&std::process::id().to_be_bytes());
    let r = release::RELEASE;
    MediaHeader {
        release: (r.v, r.r, r.m),
        target: (r.v, r.r, r.m),
        compression: Compression::None,
        kind,
        created: now,
        stream_id: id,
        system: "QS400".into(),
        serial: crate::stgsvc::security::sysval_text(system, "QSRLNBR"),
        command: "SAVLICPGM".into(),
        user: user(system),
    }
}

/// Write a product stream into a save file, `CLEAR` honoured.
fn to_savf(system: &mut System, file: &(String, String), clear: &str, bytes: &[u8]) -> Result<(), String> {
    let id = find_savf(system, file)?;
    let clear = clear.to_ascii_uppercase();
    in_store(system, |txn, _, _| {
        let used = savf::len(txn, id).map_err(|e| e.to_string())?;
        if used > 0 && !matches!(clear.as_str(), "*ALL" | "*REPLACE" | "*AFTER") {
            return Err(format!("CPA4067 save file {} already holds data", file.0));
        }
        savf::write(txn, id, bytes).map_err(|e| e.to_string())
    })
}

fn savlicpgm(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require(SpecialAuth::SAVSYS, "save licensed programs").or_else(|_| require(SpecialAuth::ALLOBJ, "save licensed programs"))?;
    device(parms)?;
    let pid = product(&parms.text("LICPGM"))?;
    let tgt = parms.text("TGTRLS");
    if !matches!(tgt.as_str(), "" | "*CURRENT") && tgt != release::RELEASE.to_string() {
        return Err(format!("CPF3743 target release {tgt} is not valid"));
    }
    let req = SavLicPgm {
        product: pid.clone(),
        option: option_of(&parms.text("OPTION"))?,
        release: release_of(&parms.text("RLS"))?,
        lng: lng_of(&parms.text("LNG"))?,
        objects: objects_of(&parms.text("OBJTYPE")),
    };
    let head = header(system, SaveKind::LicPgm);
    let jobh = job(system);
    let line = parms.to_line("SAVLICPGM");
    let (bytes, n) = lp::with(system, |p, cell, qsys| {
        let plan = plan_save(p, &req).map_err(|e| format!("{NOT_SAVED} {e}"))?;
        let b = save_product(cell, qsys, jobh, &AllowAll, &plan, &head, &line)?;
        Ok((b, plan.loads.len()))
    })?;
    to_savf(system, &parms.qual("SAVF"), &parms.text("CLEAR"), &bytes)?;
    Ok(done(
        format!("{SAVED} {n} product load(s) of {pid} saved\n"),
        SAVED,
        &[(pid.as_str(), 7), (&parms.text("OPTION"), 5)],
    ))
}

fn chkprdopt_cmd(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let pid = product(&parms.text("PRDID"))?;
    let option = option_of(&parms.text("OPTION"))?;
    let rel = release_of(&parms.text("RLS"))?;
    let full = parms.text("DETAIL") == "*FULL";
    let report = lp::with(system, |p, cell, qsys| {
        let objs = Products::new(cell, qsys, lpstore::stamp(now_unix()));
        chkprdopt(p, &objs, &pid, option, rel).map_err(|e| format!("{NOT_INSTALLED} {e}"))
    })?;
    let mut text = String::new();
    let mut errors = 0usize;
    for (k, state, findings) in &report.loads {
        errors += findings.len();
        if full || !findings.is_empty() {
            let _ = writeln!(text, "{k}: load state {}", state.code());
        }
        for f in findings {
            let line = match f {
                Finding::Missing(o) => format!("  object {}/{} {} is missing", o.lib, o.name, o.typ),
                Finding::Damaged(o) => format!("  object {}/{} {} is damaged", o.lib, o.name, o.typ),
                Finding::WrongRelease(o, r) => {
                    format!("  object {}/{} {} is at release {r}", o.lib, o.name, o.typ)
                }
                Finding::PathMissing(p) => format!("  {p} is missing"),
                Finding::PathDamaged(p) => format!("  {p} is damaged"),
                Finding::Prereq(r) => format!("  needs {} option {}", r.product, r.option),
            };
            let _ = writeln!(text, "{line}");
        }
    }
    if errors > 0 {
        return Err(format!("{CHECK_ERRORS} {errors} error(s) found\n{text}"));
    }
    Ok(done(text, CHECK_OK, &[(pid.as_str(), 7)]))
}

/// DSPSFWRSC rows: resource ID, option, feature, description, type,
/// library, release; the Licensed Internal Code first.
pub fn sfwrsc_rows(system: &mut System) -> Vec<Vec<String>> {
    let rls = release::RELEASE.to_string();
    let mut rows = vec![
        ["5770999", "*BASE", "5050", "Licensed Internal Code", "*CODE", "", &rls]
            .iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<String>>(),
    ];
    let res = lp::with(system, |p, _, _| {
        let loads = p.loads();
        Ok(software_resources(p)
            .into_iter()
            .map(|r| {
                let lib = loads
                    .iter()
                    .find(|(_, l)| {
                        l.key.product == r.product && l.key.option == r.option && l.key.load_id == r.feature
                    })
                    .and_then(|(_, l)| l.principal().map(|e| e.installed.clone()))
                    .unwrap_or_default();
                vec![
                    r.product.to_string(),
                    r.option.to_string(),
                    r.feature.to_string(),
                    r.text,
                    r.load_type.name().to_string(),
                    lib,
                    r.release.to_string(),
                ]
            })
            .collect::<Vec<_>>())
    })
    .unwrap_or_default();
    rows.extend(res);
    rows
}

fn dspsfwrsc_text(system: &mut System) -> String {
    let mut out = String::from("Resource ID  Option  Feature  Release  Description\n");
    for r in sfwrsc_rows(system) {
        let _ = writeln!(out, "{:<11}  {:<6}  {:<7}  {:<7}  {}", r[0], r[1], r[2], r[6], r[3]);
    }
    out
}

/// GO LICPGM option 10 rows: product, option, installed status,
/// description, release.
pub fn installed_rows(system: &mut System) -> Vec<Vec<String>> {
    let sys = system_info(system);
    lp::with(system, |p, _, _| {
        Ok(installed_programs(p, &sys)
            .into_iter()
            .map(|r| {
                vec![
                    r.product.to_string(),
                    r.option.to_string(),
                    r.status.name().to_string(),
                    r.text,
                    r.release.to_string(),
                ]
            })
            .collect())
    })
    .unwrap_or_default()
}

/// WRKLICINF rows: product, term, feature, compliance, usage limit,
/// usage count, peak, threshold, grace days, expiration.
pub fn licinf_rows(system: &mut System, product: Option<&str>) -> Vec<Vec<String>> {
    let sys = system_info(system);
    lp::with(system, |p, _, _| {
        Ok(p.licenses()
            .info_rows(&sys, today())
            .into_iter()
            .filter(|r| product.is_none_or(|x| r.product == x))
            .map(|r| {
                vec![
                    r.product,
                    r.term,
                    r.feature,
                    r.compliance.to_string(),
                    r.usage_limit,
                    r.usage_count.to_string(),
                    r.peak.to_string(),
                    r.threshold,
                    r.grace_days.to_string(),
                    r.expiration.map(|d| d.to_string()).unwrap_or_default(),
                ]
            })
            .collect())
    })
    .unwrap_or_default()
}

fn wrklicinf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let p = parms.text("PRDID");
    let filter = match p.as_str() {
        "" | "*INSTALLED" | "*ALL" => None,
        x => Some(product(x)?.to_string()),
    };
    let mut out = String::from("Product  Term    Feature  Compliance  Limit    Count  Peak   Threshold\n");
    for r in licinf_rows(system, filter.as_deref()) {
        let _ = writeln!(
            out,
            "{:<7}  {:<6}  {:<7}  {:<10}  {:<7}  {:<5}  {:<5}  {}",
            r[0], r[1], r[2], r[3], r[4], r[5], r[6], r[7]
        );
    }
    Ok(CmdReply {
        text: out,
        ..CmdReply::default()
    })
}

fn special_none(s: &str) -> String {
    if s.eq_ignore_ascii_case("*NONE") {
        String::new()
    } else {
        s.to_string()
    }
}

fn addlickey(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require(SpecialAuth::ALLOBJ, "add license keys")?;
    let records: Vec<KeyRecord> = if parms.text("LICKEYINP") == "*LICKEYFILE" {
        let path = parms.text("LICKEYFILE");
        let text = super::stmf::read_as_job(system, &path)?;
        keyfile::parse(&text).map_err(|e| format!("CPF9E78 {e}"))?
    } else {
        let groups: Vec<String> = parms.list("LICKEY").iter().map(|v| v.source()).collect();
        let key = LicenseKey::parse(&groups.join(" ")).map_err(|e| format!("CPF9E78 {e}"))?;
        let usage_limit = match parms.text("USGLMT").to_ascii_uppercase().as_str() {
            "" | "*NOMAX" => None,
            n => Some(n.parse().map_err(|_| format!("CPF9E15 usage limit {n} is not valid"))?),
        };
        let expiration = match parms.text("EXPDATE").to_ascii_uppercase().as_str() {
            "" | "*NONE" => None,
            d => Some(Date::parse(d).ok_or_else(|| format!("CPF9E15 expiration date {d} is not valid"))?),
        };
        let fields = KeyFields {
            product: product(&parms.text("PRDID"))?,
            term: parms
                .text("LICTRM")
                .parse()
                .map_err(|_| "CPF9E15 the license term is not valid".to_string())?,
            feature: LoadId::feature(&parms.text("FEATURE"))
                .map_err(|_| "CPF9E15 the feature is not valid".to_string())?,
            serial: parms.text("SERIAL").to_ascii_uppercase(),
            processor_group: parms.text("PRCGRP").to_ascii_uppercase(),
            usage_limit,
            expiration,
            vendor_data: special_none(&parms.text("VNDDTA")),
        };
        vec![KeyRecord { fields, key }]
    };
    let sys = system_info(system);
    let qkey = lp::quartz_key();
    let n = records.len();
    let first = records.first().map(|r| r.fields.product.to_string()).unwrap_or_default();
    lp::with(system, |p, _, _| {
        let defs: Vec<_> = p.definitions().into_iter().map(|(_, d)| d).collect();
        let mut repo = p.licenses();
        for r in records {
            repo.add_key(&defs, qkey.as_ref(), &sys, AddLicKey { fields: r.fields, key: r.key }, today())
                .map_err(|e| format!("{} {e}", e.msg_id()))?;
        }
        p.save_licenses(&repo).map_err(|e| e.to_string())
    })?;
    Ok(done(
        format!("{KEY_ADDED} {n} license key(s) added\n"),
        KEY_ADDED,
        &[(&first, 7)],
    ))
}

fn all_or<T>(s: &str, f: impl FnOnce(&str) -> Result<T, String>) -> Result<Option<T>, String> {
    match s.to_ascii_uppercase().as_str() {
        "" | "*ALL" => Ok(None),
        x => f(x).map(Some),
    }
}

fn rmvlickey(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require(SpecialAuth::ALLOBJ, "remove license keys")?;
    let pid = product(&parms.text("PRDID"))?;
    let term: Option<LicTerm> = all_or(&parms.text("LICTRM"), |t| {
        t.parse().map_err(|_| format!("CPF9E15 license term {t} is not valid"))
    })?;
    let feature = all_or(&parms.text("FEATURE"), |f| {
        LoadId::feature(f).map_err(|_| format!("CPF9E15 feature {f} is not valid"))
    })?;
    let sys = system_info(system);
    let serial = match parms.text("SERIAL").to_ascii_uppercase().as_str() {
        "*ALL" => None,
        "" | "*LOCAL" => Some(sys.serial.clone()),
        s => Some(s.to_string()),
    };
    let grp = all_or(&parms.text("PRCGRP"), |g| Ok(g.to_string()))?;
    let n = lp::with(system, |p, _, _| {
        let mut repo = p.licenses();
        let n = repo
            .remove_keys(&pid, term.as_ref(), feature.as_ref(), serial.as_deref(), grp.as_deref())
            .map_err(|e| format!("{} {e}", e.msg_id()))?;
        p.save_licenses(&repo).map_err(|e| e.to_string())?;
        Ok(n)
    })?;
    Ok(done(
        format!("{KEY_REMOVED} {n} license key(s) removed\n"),
        KEY_REMOVED,
        &[(pid.as_str(), 7)],
    ))
}

fn dsplickey(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let pid = all_or(&parms.text("PRDID"), product)?;
    let term = all_or(&parms.text("LICTRM"), |t| Ok(t.to_string()))?;
    let feature = all_or(&parms.text("FEATURE"), |f| Ok(f.to_string()))?;
    let repo = lp::with(system, |p, _, _| Ok(p.licenses()))?;
    let keys: Vec<_> = repo
        .keys
        .iter()
        .filter(|k| {
            pid.as_ref().is_none_or(|p| &k.fields.product == p)
                && term.as_ref().is_none_or(|t| k.fields.term.as_str() == t)
                && feature.as_ref().is_none_or(|f| k.fields.feature.as_str() == f)
        })
        .collect();
    if parms.text("OUTPUT") == "*LICKEYFILE" {
        let recs: Vec<KeyRecord> = keys
            .iter()
            .map(|k| KeyRecord {
                fields: k.fields.clone(),
                key: k.key.clone(),
            })
            .collect();
        let path = parms.text("LICKEYFILE");
        let text = keyfile::write(&recs);
        super::stmf::with_job(system, |c| super::stmf::write_text(c, &path, &text))?;
        return Ok(CmdReply {
            text: format!("{} license key(s) written to {path}\n", recs.len()),
            ..CmdReply::default()
        });
    }
    let mut out = String::new();
    for row in repo.key_rows(None).into_iter().filter(|r| {
        pid.as_ref().is_none_or(|p| r.product == p.as_str())
            && term.as_ref().is_none_or(|t| &r.term == t)
            && feature.as_ref().is_none_or(|f| &r.feature == f)
    }) {
        let _ = writeln!(
            out,
            "Product {}  term {}  feature {}  serial {}  processor group {}",
            row.product, row.term, row.feature, row.serial, row.processor_group
        );
        let _ = writeln!(
            out,
            "  usage limit {}  expiration {}  vendor data {}",
            row.usage_limit, row.expiration, row.vendor_data
        );
        for l in row.key {
            let _ = writeln!(out, "  {l}");
        }
    }
    if out.is_empty() {
        out = "No license keys match.\n".into();
    }
    Ok(CmdReply {
        text: out,
        ..CmdReply::default()
    })
}

fn chglicinf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require(SpecialAuth::ALLOBJ, "change license information")?;
    let pid = product(&parms.text("PRDID"))?;
    let term: LicTerm = parms
        .text("LICTRM")
        .parse()
        .map_err(|_| "CPF9E15 the license term is not valid".to_string())?;
    let feature =
        LoadId::feature(&parms.text("FEATURE")).map_err(|_| "CPF9E15 the feature is not valid".to_string())?;
    let num = |k: &str, none: &[&str]| -> Result<Option<Option<u32>>, String> {
        let v = parms.text(k).to_ascii_uppercase();
        match v.as_str() {
            "" | "*SAME" => Ok(None),
            x if none.contains(&x) => Ok(Some(None)),
            x => x
                .parse()
                .map(|n| Some(Some(n)))
                .map_err(|_| format!("CPF9E15 {k} {x} is not valid")),
        }
    };
    let (mq, ml) = parms.qual("MSGQ");
    let chg = ChgLicInf {
        usage_limit: num("USGLMT", &["*NOMAX"])?,
        threshold: num("THRESHOLD", &["*USGLMT", "*CALC"])?,
        msgq: (!mq.is_empty() && mq != "*SAME").then(|| QualName::new(&mq, &lib_or(&ml))),
        log: match parms.text("LOG").as_str() {
            "*YES" => Some(true),
            "*NO" => Some(false),
            _ => None,
        },
    };
    lp::with(system, |p, _, _| {
        let mut repo = p.licenses();
        repo.change_info(&pid, &term, &feature, &chg)
            .map_err(|e| format!("{} {e}", e.msg_id()))?;
        p.save_licenses(&repo).map_err(|e| e.to_string())
    })?;
    Ok(done(
        format!("{INFO_CHANGED} license information of {pid} changed\n"),
        INFO_CHANGED,
        &[(pid.as_str(), 7)],
    ))
}

/// WRKSFWAGR rows: product, release, description, status.
pub fn agreement_rows(system: &mut System) -> Vec<Vec<String>> {
    lp::with(system, |p, _, _| {
        let loads = p.loads();
        let products: Vec<(ProductId, release::Release, String)> = p
            .definitions()
            .into_iter()
            .map(|(_, d)| d)
            .filter(|d| {
                loads.iter().any(|(_, l)| {
                    l.key.product == d.product && l.key.release == d.release && l.state.is_installed()
                })
            })
            .map(|d| (d.product.clone(), d.release, d.text.clone()))
            .collect();
        Ok(p.agreements()
            .rows(&products)
            .into_iter()
            .map(|r| {
                vec![
                    r.product.to_string(),
                    r.release.to_string(),
                    r.text,
                    r.status.to_string(),
                ]
            })
            .collect())
    })
    .unwrap_or_default()
}

fn wrksfwagr_text(system: &mut System) -> String {
    let mut out = String::from("Product  Release  Status      Description\n");
    for r in agreement_rows(system) {
        let _ = writeln!(out, "{:<7}  {:<7}  {:<10}  {}", r[0], r[1], r[3], r[2]);
    }
    out
}

fn qspkgprd(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    require(SpecialAuth::ALLOBJ, "package products")?;
    let path = parms.text("MANIFEST");
    let text = super::stmf::read_as_job(system, &path)?;
    let pkg = licpgm::pkg::parse(&text, licpgm::pkg::Options::default()).map_err(|errs| {
        let mut s = format!("{BAD_MANIFEST} {path} has {} error(s)", errs.len());
        for e in errs {
            let _ = write!(s, "\n  {e}");
        }
        s
    })?;
    if let Some(a) = &pkg.agreement
        && let Some(doc) = pkg
            .loads
            .iter()
            .find_map(|(_, l)| l.agreements.first().cloned())
    {
        let _ = super::stmf::with_job(system, |c| super::stmf::write_text(c, &doc, a));
    }
    let pid = pkg.definition.product.clone();
    let rel = pkg.definition.release;
    let options: Vec<OptionNo> = pkg.definition.options.iter().map(|o| o.option).collect();
    lp::with(system, |p, cell, qsys| {
        licpgm::api::crtpd::create_definition(p, &pkg.definition_name, &pkg.definition)
            .map_err(|e| format!("{} {}", e.id, pkg.definition_name))?;
        for (q, l) in &pkg.loads {
            licpgm::api::crtpl::create_load(p, q, l).map_err(|e| format!("{} {q}", e.id))?;
        }
        let objs = Products::new(cell, qsys, lpstore::stamp(now_unix()));
        for o in &options {
            licpgm::api::pkgpo::package(p, &objs, &pid, Some(rel), *o, &licpgm::api::pkgpo::LoadSel::All, true)
                .map_err(|e| format!("{} option {o}", e.id))?;
        }
        Ok(())
    })?;
    let (sname, slib) = parms.qual("SAVF");
    if !sname.is_empty() && sname != "*NONE" {
        let head = header(system, SaveKind::LicPgm);
        let jobh = job(system);
        let req = SavLicPgm {
            product: pid.clone(),
            option: None,
            release: Some(rel),
            lng: None,
            objects: Objects::All,
        };
        let bytes = lp::with(system, |p, cell, qsys| {
            let plan = plan_save(p, &req).map_err(|e| format!("{NOT_SAVED} {e}"))?;
            save_product(cell, qsys, jobh, &AllowAll, &plan, &head, "QSPKGPRD")
        })?;
        to_savf(system, &(sname, slib), "*REPLACE", &bytes)?;
    }
    Ok(done(
        format!("{PACKAGED} product {pid} packaged from {path}\n"),
        PACKAGED,
        &[(pid.as_str(), 7), (&path, 50)],
    ))
}
