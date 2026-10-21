// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Printer files at the console: `CRTPRTF` (DDS from a source member, or
//! program-described), `CHGPRTF`, `OVRPRTF`, and `DLTF` and `DSPFD` of a
//! printer file.

use std::fmt::Write as _;

use dds::{CompileOpts, compile_prtf};
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;
use wm::obj::prtf::Prtf;

use super::files::{refs_for, source};
use super::{Params, db, file_id, now, object_name, with_txn};
use crate::ipl::system::System;
use crate::stgsvc::prtf::PrtOverride;
use crate::svc::db::SqlContext;

/// Whether `id` is a printer file.
pub fn is_prtf(txn: &mut Txn<'_>, id: ObjectId) -> bool {
    ops::read_header(txn, id).is_ok_and(|(_, h)| {
        h.id.type_code == types::FILE && ccsid::decode_trimmed(&h.attribute) == "PRTF"
    })
}

fn err(e: &wm::Error) -> String {
    format!("{} {e}", e.msgid().unwrap_or("CPD0084"))
}

/// `crtprtf LIB/FILE [SRCFILE(LIB/SRC) SRCMBR(M)] [PAGESIZE() LPI() CPI() OVRFLW() CTLCHAR() OUTQ() ...]`.
pub fn crtprtf(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let name = object_name(&qual)?;
    let mut f = Prtf::default();
    f.apply(p).map_err(|e| err(&e))?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let lib = ctx.library_for(txn, &name).map_err(|e| e.to_string())?;
        let fname = SqlContext::sys_name(&name.name).map_err(|e| e.to_string())?;
        let mut note = String::from("program-described");
        if p.kw("SRCFILE").is_some() {
            let (src, text) = source(txn, ctx, p, fname)?;
            let (refs, _) = refs_for(txn, ctx, &text);
            let comp = compile_prtf(
                &src,
                &refs,
                &CompileOpts {
                    file: Some(fname.to_string()),
                    ..CompileOpts::default()
                },
            );
            let Some(def) = comp.definition else {
                return Err(format!("File {qual} not created.\n{}", comp.listing));
            };
            note = format!(
                "{} record format(s), {} diagnostic(s)",
                def.formats.len(),
                comp.diagnostics.len()
            );
            if f.text.is_empty() {
                f.text = def.text.clone().unwrap_or_default();
            }
            f.formats = dds::prtf::codec::encode(&def);
        }
        let stamp = wm::obj::Stamp {
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: now(),
        };
        wm::obj::create_attr(
            txn,
            lib,
            (fname, types::FILE, "PRTF"),
            &f.text.clone(),
            &stamp,
            &f.encode(),
        )
        .map_err(|e| err(&e))?;
        Ok(format!("Printer file {qual} created: {note}."))
    })
}

/// A printer file's object, or why not.
fn prtf_id(txn: &mut Txn<'_>, ctx: &SqlContext, qual: &str) -> Result<ObjectId, String> {
    let id = file_id(txn, ctx, qual)?;
    if is_prtf(txn, id) {
        Ok(id)
    } else {
        Err(format!("CPF4123 {qual} is not a printer file"))
    }
}

/// `chgprtf LIB/FILE keywords...`.
pub fn chgprtf(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = prtf_id(txn, ctx, &qual)?;
        let mut f =
            Prtf::decode(&wm::obj::read(txn, id).map_err(|e| err(&e))?).map_err(|e| err(&e))?;
        f.apply(p).map_err(|e| err(&e))?;
        wm::obj::write(txn, id, &f.encode()).map_err(|e| err(&e))?;
        Ok(format!("Printer file {qual} changed."))
    })
}

/// `DLTF` of a printer file; `None` when `qual` is not one.
pub fn dltf(system: &mut System, qual: &str) -> Option<Result<String, String>> {
    let (store, ctx) = db(system).ok()?;
    let found = with_txn(store, |txn| {
        let id = file_id(txn, ctx, qual)?;
        if !is_prtf(txn, id) {
            return Ok(false);
        }
        ops::delete(txn, JobHandle(0), id).map_err(|e| e.to_string())?;
        Ok(true)
    });
    match found {
        Ok(false) => None,
        Ok(true) => Some(Ok(format!("File {qual} deleted."))),
        Err(e) => Some(Err(e)),
    }
}

/// `DSPFD` of a printer file; `None` when `qual` is not one.
pub fn dspfd(system: &mut System, qual: &str) -> Option<Result<String, String>> {
    let (store, ctx) = db(system).ok()?;
    let r = with_txn(store, |txn| {
        let id = file_id(txn, ctx, qual)?;
        if !is_prtf(txn, id) {
            return Ok(None);
        }
        let f = Prtf::decode(&wm::obj::read(txn, id).map_err(|e| err(&e))?).map_err(|e| err(&e))?;
        let mut out = format!("File  . . . . . . . : {qual}\n");
        out.push_str(&f.describe());
        if let Some(def) = dds::prtf::codec::decode(&f.formats) {
            for fmt in &def.formats {
                let _ = writeln!(
                    out,
                    "  Record format {:<10} {} field(s) and constant(s)",
                    fmt.name,
                    fmt.fields.len()
                );
            }
        }
        Ok(Some(out))
    });
    r.transpose()
}

/// `ovrprtf FILE(F) [TOFILE(LIB/F)] [OVRSCOPE(*ACTGRPDFN|*JOB)] keywords...`.
pub fn ovrprtf(system: &mut System, line: &str) -> Result<String, String> {
    let p = Params::parse(line);
    let file = p.pos_or(0, "FILE")?.to_ascii_uppercase();
    qsname::Name::parse(&file).map_err(|e| format!("CPD0078 FILE({file}): {e}"))?;
    Prtf::default().apply(&p).map_err(|e| err(&e))?;
    let scope = match p.upper("OVRSCOPE").as_deref() {
        Some("*JOB") => "*JOB".to_string(),
        None | Some("*ACTGRPDFN") => db::odp::ovrdbf::DFTACTGRP.to_string(),
        Some(s) => return Err(format!("CPD0084 OVRSCOPE({s}) is not valid")),
    };
    let table = &mut system.svc.prt_overrides;
    table.retain(|o| o.file != file || o.scope != scope);
    table.push(PrtOverride {
        file: file.clone(),
        scope,
        line: line.to_string(),
    });
    Ok(format!(
        "Override of printer file {file} in effect ({} printer override(s)).",
        table.len()
    ))
}

/// `DLTOVR` of printer file overrides: how many went.
pub fn dltovr(system: &mut System, file: &str) -> usize {
    let table = &mut system.svc.prt_overrides;
    let before = table.len();
    if file.eq_ignore_ascii_case("*ALL") {
        table.clear();
    } else {
        table.retain(|o| !o.file.eq_ignore_ascii_case(file));
    }
    before - table.len()
}
