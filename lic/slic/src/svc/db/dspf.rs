// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display files at the console: `CRTDSPF` from DDS, `OVRDSPF`, the
//! `DLTF` and `DSPFD` of a display file, the open a job asks for (the
//! payload the job's runtime renders), and the shipped display files
//! (`QDSIGNON`) installed when missing.

use std::fmt::Write as _;

use dspf::{CompileOpts, compile_dspf};
use qsname::Name;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;
use wm::cmd::Params;

use super::files::{refs_for, source};
use super::{db, file_id, now, object_name, with_txn};
use crate::ipl::system::System;
use crate::stgsvc::prtf::PrtOverride;
use crate::svc::db::SqlContext;

/// The object attribute of a display file.
pub const ATTR: &str = "DSPF";

/// Whether `id` is a display file.
pub fn is_dspf(txn: &mut Txn<'_>, id: ObjectId) -> bool {
    ops::read_header(txn, id).is_ok_and(|(_, h)| {
        h.id.type_code == types::FILE && ccsid::decode_trimmed(&h.attribute) == ATTR
    })
}

fn err(e: &wm::Error) -> String {
    format!("{} {e}", e.msgid().unwrap_or("CPD0084"))
}

/// Compile DDS text into a display file's payload: the payload and a
/// note for the completion text, or the listing.
fn compile(src: &dspf::Source, refs: &dspf::MapRefs, name: &str) -> Result<(Vec<u8>, String, usize), String> {
    let comp = compile_dspf(
        src,
        refs,
        &CompileOpts {
            file: Some(name.to_string()),
            ..CompileOpts::default()
        },
    );
    let Some(df) = comp.definition else {
        return Err(format!("File {name} not created.\n{}", comp.listing));
    };
    let n = df.records.len();
    Ok((dspf::encode(&df), comp.listing, n))
}

/// `crtdspf LIB/FILE SRCFILE(LIB/SRC) [SRCMBR(M)] [TEXT('..')]`.
pub fn crtdspf(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let name = object_name(&qual)?;
    let text = p.kw("TEXT").map(str::to_string).unwrap_or_default();
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let lib = ctx.library_for(txn, &name).map_err(|e| e.to_string())?;
        let fname = SqlContext::sys_name(&name.name).map_err(|e| e.to_string())?;
        let (src, dds_text) = source(txn, ctx, p, fname)?;
        let (refs, _) = refs_for(txn, ctx, &dds_text);
        let (payload, _, records) = compile(&src, &refs, &fname.to_string())?;
        let text = if text.is_empty() {
            dspf::decode(&payload)
                .ok()
                .and_then(|df| {
                    df.file
                        .keywords
                        .iter()
                        .find(|k| k.name == "TEXT")
                        .and_then(|k| k.parms.first())
                        .and_then(|q| match q {
                            dds::Parm::Quoted(t) => Some(t.clone()),
                            _ => None,
                        })
                })
                .unwrap_or_default()
        } else {
            text
        };
        let stamp = wm::obj::Stamp {
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: now(),
        };
        wm::obj::create_attr(txn, lib, (fname, types::FILE, ATTR), &text, &stamp, &payload)
            .map_err(|e| err(&e))?;
        Ok(format!(
            "Display file {qual} created: {records} record format(s)."
        ))
    })
}

/// `DLTF` of a display file; `None` when `qual` is not one.
pub fn dltf(system: &mut System, qual: &str) -> Option<Result<String, String>> {
    let (store, ctx) = db(system).ok()?;
    let found = with_txn(store, |txn| {
        let id = file_id(txn, ctx, qual)?;
        if !is_dspf(txn, id) {
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

/// `DSPFD` of a display file; `None` when `qual` is not one.
pub fn dspfd(system: &mut System, qual: &str) -> Option<Result<String, String>> {
    let (store, ctx) = db(system).ok()?;
    let r = with_txn(store, |txn| {
        let id = file_id(txn, ctx, qual)?;
        if !is_dspf(txn, id) {
            return Ok(None);
        }
        let df = dspf::decode(&wm::obj::read(txn, id).map_err(|e| err(&e))?)
            .map_err(|e| e.to_string())?;
        let mut out = format!("File  . . . . . . . : {qual}\n");
        let _ = writeln!(out, "Type  . . . . . . . : *DSPF");
        let sizes: Vec<String> = df.file.sizes.iter().map(dspf::DisplaySize::label).collect();
        let _ = writeln!(out, "Display size  . . . : {}", sizes.join(", "));
        let _ = writeln!(
            out,
            "Separate indicators : {}",
            if df.file.indara { "*YES" } else { "*NO" }
        );
        let _ = writeln!(out, "Record formats  . . : {}", df.records.len());
        for r in &df.records {
            let _ = writeln!(
                out,
                "  {:<10} {:<24} out {:>5} in {:>5} fields {:>3}  {}",
                r.name,
                r.kind.name(),
                r.layout.output.len,
                r.layout.input.len,
                r.fields.iter().filter(|f| !f.is_constant()).count(),
                r.layout.level_id
            );
        }
        Ok(Some(out))
    });
    r.transpose()
}

/// `ovrdspf FILE(F) [TOFILE(LIB/F) OVRSCOPE(*JOB) SECURE(*YES)]`.
pub fn ovrdspf(system: &mut System, line: &str) -> Result<String, String> {
    let p = Params::parse(line);
    let file = p.pos_or(0, "FILE")?.to_ascii_uppercase();
    Name::parse(&file).map_err(|e| format!("CPD0078 FILE({file}): {e}"))?;
    let scope = match p.upper("OVRSCOPE").as_deref() {
        Some("*JOB") => "*JOB".to_string(),
        None | Some("*ACTGRPDFN" | "*CALLLVL") => db::odp::ovrdbf::DFTACTGRP.to_string(),
        Some(s) => return Err(format!("CPD0084 OVRSCOPE({s}) is not valid")),
    };
    let level = super::journal::call_level(&p);
    let table = &mut system.svc.dsp_overrides;
    table.retain(|o| o.file != file || o.scope != scope || o.level != level);
    table.push(PrtOverride {
        file: file.clone(),
        scope,
        level,
        secure: p.upper("SECURE").as_deref() == Some("*YES"),
        line: line.to_string(),
    });
    Ok(format!(
        "Override of display file {file} in effect ({} display override(s)).",
        table.len()
    ))
}

/// `DLTOVR` of display overrides: by file (or `*ALL`), at one call
/// level or every one; how many went.
pub fn dltovr(system: &mut System, file: &str, level: Option<u32>) -> usize {
    let table = &mut system.svc.dsp_overrides;
    let before = table.len();
    let all = file.eq_ignore_ascii_case("*ALL");
    table.retain(|o| {
        level.is_some_and(|l| o.level != l) || (!all && !o.file.eq_ignore_ascii_case(file))
    });
    before - table.len()
}

/// Call level `level` returned: its display overrides and any deeper
/// level's go; how many went.
pub fn end_level(system: &mut System, level: u32) -> usize {
    let table = &mut system.svc.dsp_overrides;
    let before = table.len();
    table.retain(|o| o.level < level);
    before - table.len()
}

/// Find a display file through the names in force: its object and
/// its real `lib/name`.
fn find(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    qual: &str,
) -> Result<(ObjectId, String), String> {
    let (lib, name) = wm::obj::params::qualified(qual);
    let names = crate::svc::ile::name_context(txn, qsys);
    let id = wm::obj::find(txn, qsys, &names, &lib, &name, types::FILE)
        .map_err(|_| format!("CPF4101 file {name} in library {lib} not found"))?;
    if !is_dspf(txn, id) {
        return Err(format!(
            "CPF4123 file {name} in library {lib} is not a display file"
        ));
    }
    let real = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
    Ok((id, format!("{real}/{name}")))
}

/// Open display file `(file, lib)` from activation group `actgrp`:
/// the overrides that reach it applied (the last `TOFILE` redirects).
/// The payload and the file's real `lib/name`.
pub fn open(
    system: &mut System,
    overrides: &[PrtOverride],
    (file, lib): (&str, &str),
    actgrp: &str,
) -> Result<(Vec<u8>, String), String> {
    let file = file.to_ascii_uppercase();
    let mut mine: Vec<&PrtOverride> = overrides
        .iter()
        .filter(|o| o.file == file && db::odp::ovrdbf::reaches(&o.scope, actgrp))
        .collect();
    mine.sort_by_key(|o| o.level);
    if let Some(i) = mine.iter().rposition(|o| o.secure) {
        mine.drain(..i);
    }
    let target = mine
        .iter()
        .rev()
        .find_map(|o| Params::parse(&o.line).upper("TOFILE"))
        .unwrap_or_else(|| format!("{}/{file}", if lib.is_empty() { "*LIBL" } else { lib }));
    crate::stgsvc::spool::with_store(system, |txn, qsys| {
        let (id, real) = find(txn, qsys, &target)?;
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        Ok((b, real))
    })
}

/// The shipped display files that are missing from `QSYS`, compiled
/// from their DDS and created; how many.
pub fn install_shipped(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
) -> Result<usize, String> {
    let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let mut made = 0;
    for (name, text) in dspf::sys::FILES {
        let n = Name::parse(name).map_err(|e| format!("{name}: {e}"))?;
        if lib
            .lookup(txn, types::FILE, n)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        let (src, _) = dspf::Source::from_text(text);
        let (payload, _, _) = compile(&src, &dspf::MapRefs::default(), name)?;
        wm::obj::create_attr(
            txn,
            qsys,
            (n, types::FILE, ATTR),
            &format!("Shipped display file {name}"),
            stamp,
            &payload,
        )
        .map_err(|e| err(&e))?;
        made += 1;
    }
    Ok(made)
}
