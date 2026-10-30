// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! File, member, copy, display and source member commands.

use std::collections::HashMap;
use std::fmt::Write as _;

use db::file::{self, CreateFileOpts, MemberOpt, Size};
use db::util::{
    self, CpyfOpts, DtaFmt, ErrLvl, FmtOpt, ImpfOpts, KeyFile, MbrOpt, RcdDlm, RmvBlank, Target,
    ToMember,
};
use dds::{CompileOpts, DateFmt, FileRef, MapRefs, RefResolver, TimeFmt, compile_lf, compile_pf};
use qsname::{LibSpec, MemberSpec, Name};
use sql::SqlContext;
use store::obj::ObjectId;
use store::txn::Txn;

use super::{Params, db, db_err, file_id, member_spec, now, object_name, with_txn};
use crate::ipl::system::System;

/// The files a DDS source names in `PFILE`, `JFILE`, `FORMAT`, `REF` and
/// `REFFLD`, resolved now so the compiler and the create can use them.
pub(super) fn refs_for(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    text: &str,
) -> (MapRefs, HashMap<(Option<Name>, Name), ObjectId>) {
    let mut refs = MapRefs::default();
    let mut ids = HashMap::new();
    let upper = text.to_ascii_uppercase();
    for key in ["PFILE(", "JFILE(", "FORMAT(", "REF(", "REFFLD("] {
        let mut from = 0;
        while let Some(i) = upper[from..].find(key) {
            let start = from + i + key.len();
            let Some(end) = upper[start..].find(')') else {
                break;
            };
            let inner = &upper[start..start + end];
            let words: Vec<&str> = inner.split_whitespace().collect();
            // REFFLD(field file) names the file second.
            let names: Vec<&str> = if key == "REFFLD(" {
                words.get(1).copied().into_iter().collect()
            } else {
                words
            };
            for w in names {
                let w = w.split_whitespace().next().unwrap_or(w);
                if w.starts_with('*') || w.is_empty() {
                    continue;
                }
                let Ok(name) = object_name(w) else { continue };
                let Ok(id) = ctx.resolve_file(txn, &name) else {
                    continue;
                };
                let Ok(desc) = file::describe(txn, id) else {
                    continue;
                };
                let lib = Name::parse(&desc.library_name.to_string()).expect("valid");
                refs.insert(
                    lib,
                    desc.name(),
                    desc.formats.iter().map(|f| f.format.clone()).collect(),
                );
                ids.insert((Some(lib), desc.name()), id);
                ids.insert((None, desc.name()), id);
            }
            from = start + end;
        }
    }
    (refs, ids)
}

fn create_opts(p: &Params) -> Result<CreateFileOpts, String> {
    let mut o = CreateFileOpts {
        now: now(),
        text: p.text("TEXT"),
        ..CreateFileOpts::default()
    };
    if let Some(n) = p.upper("MAXMBRS") {
        o.max_members = if n == "*NOMAX" {
            0
        } else {
            n.parse().map_err(|_| "MAXMBRS: a number is needed")?
        };
    }
    if p.yes("REUSEDLT") {
        o.reusedlt = true;
    }
    if let Some(s) = p.upper("SIZE") {
        let words: Vec<&str> = s.split_whitespace().collect();
        o.size = if words.first() == Some(&"*NOMAX") {
            Size::NOMAX
        } else {
            let n = |i: usize, d: u32| -> Result<u32, String> {
                words.get(i).map_or(Ok(d), |w| {
                    w.parse()
                        .map_err(|_| format!("SIZE({s}): numbers are needed"))
                })
            };
            Size {
                initial: n(0, 10_000)?,
                increment: n(1, 1000)?,
                max_increments: n(2, 3)?,
            }
        };
    }
    if let Some(m) = p.upper("MBR") {
        o.member = match m.as_str() {
            "*NONE" => MemberOpt::None,
            "*FILE" => MemberOpt::File,
            other => {
                MemberOpt::Named(Name::parse(other).map_err(|e| format!("MBR({other}): {e}"))?)
            }
        };
    }
    Ok(o)
}

/// The DDS source of `SRCFILE`/`SRCMBR` as text and records.
pub(super) fn source(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    p: &Params,
    default_mbr: Name,
) -> Result<(dds::Source, String), String> {
    let srcfile = p
        .kw("SRCFILE")
        .ok_or("SRCFILE(LIB/FILE) is needed, or RCDLEN(n)")?;
    let src = file_id(txn, ctx, srcfile)?;
    let mbr = match p.upper("SRCMBR") {
        Some(m) if m != "*FILE" => Name::parse(&m).map_err(|e| format!("SRCMBR({m}): {e}"))?,
        _ => default_mbr,
    };
    let source = db::source::source_of(txn, src, MemberSpec::Name(mbr)).map_err(|e| db_err(&e))?;
    let text = db::source::srcget(txn, src, MemberSpec::Name(mbr)).map_err(|e| db_err(&e))?;
    Ok((source, text))
}

/// `crtpf LIB/FILE SRCFILE(...) | RCDLEN(n)`.
pub fn crtpf(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let name = object_name(&qual)?;
    let opts = create_opts(p)?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let lib = ctx.library_for(txn, &name).map_err(|e| e.to_string())?;
        let fname = SqlContext::sys_name(&name.name).map_err(|e| e.to_string())?;
        if let Some(rcdlen) = p.num::<u32>("RCDLEN")? {
            file::create_pf_rcdlen(txn, lib, fname, rcdlen, &opts).map_err(|e| db_err(&e))?;
            return Ok(format!("File {qual} created with record length {rcdlen}."));
        }
        let (src, text) = source(txn, ctx, p, fname)?;
        let (refs, _) = refs_for(txn, ctx, &text);
        let comp = compile_pf(
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
        file::create_pf(txn, lib, fname, &def, &opts).map_err(|e| db_err(&e))?;
        Ok(format!(
            "File {qual} created: {} fields, {} key(s), {} diagnostic(s).",
            def.format.fields.len(),
            def.format.keys.len(),
            comp.diagnostics.len()
        ))
    })
}

/// `crtsrcpf LIB/FILE [RCDLEN(n)] [TEXT('..')]`: a source physical file
/// with no members (`MAXMBRS(*NOMAX)`).
pub fn crtsrcpf(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let name = object_name(&qual)?;
    let rcdlen: u32 = p.num("RCDLEN")?.unwrap_or(112);
    let mut opts = create_opts(p)?;
    opts.max_members = 0;
    opts.member = MemberOpt::None;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let lib = ctx.library_for(txn, &name).map_err(|e| e.to_string())?;
        let fname = SqlContext::sys_name(&name.name).map_err(|e| e.to_string())?;
        file::create_srcpf(txn, lib, fname, rcdlen, &opts).map_err(|e| db_err(&e))?;
        Ok(format!(
            "Source file {qual} created with record length {rcdlen}."
        ))
    })
}

/// `crtlf LIB/FILE SRCFILE(...) [SRCMBR(M)]`.
pub fn crtlf(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let name = object_name(&qual)?;
    let opts = create_opts(p)?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let lib = ctx.library_for(txn, &name).map_err(|e| e.to_string())?;
        let fname = SqlContext::sys_name(&name.name).map_err(|e| e.to_string())?;
        let (src, text) = source(txn, ctx, p, fname)?;
        let (refs, ids) = refs_for(txn, ctx, &text);
        let comp = compile_lf(
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
        let resolve = |fr: &FileRef| -> db::Result<ObjectId> {
            let key = match fr.library {
                LibSpec::Name(l) => (Some(l), fr.file),
                _ => (None, fr.file),
            };
            ids.get(&key)
                .or_else(|| ids.get(&(None, fr.file)))
                .copied()
                .ok_or_else(|| db::DbError::Definition {
                    what: format!("based-on file {fr} not found"),
                })
        };
        let mut resolve = resolve;
        file::create_lf(txn, lib, fname, &def, &mut resolve, &opts).map_err(|e| db_err(&e))?;
        let _ = refs.formats(&LibSpec::Libl, &fname);
        Ok(format!(
            "File {qual} created: {} format(s), {} diagnostic(s).",
            def.formats.len(),
            comp.diagnostics.len()
        ))
    })
}

/// `dltf LIB/FILE`.
pub fn dltf(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        file::delete_file(txn, super::JOB, id).map_err(|e| db_err(&e))?;
        Ok(format!("File {qual} deleted."))
    })
}

/// `addpfm LIB/FILE MBR [TEXT('..')] [SRCTYPE(T)]`.
pub fn addpfm(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let mbr = p.pos_or(1, "MBR")?;
    let mname = Name::parse(&mbr).map_err(|e| format!("{mbr}: {e}"))?;
    let srctype = match p.upper("SRCTYPE") {
        Some(t) => Some(Name::parse(&t).map_err(|e| format!("SRCTYPE({t}): {e}"))?),
        None => None,
    };
    let text = p.text("TEXT");
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        file::add_member(txn, id, mname, &text, srctype, now()).map_err(|e| db_err(&e))?;
        Ok(format!("Member {mbr} added to {qual}."))
    })
}

/// `rmvm LIB/FILE MBR`.
pub fn rmvm(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let mbr = p.pos_or(1, "MBR")?;
    let mname = Name::parse(&mbr).map_err(|e| format!("{mbr}: {e}"))?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        file::remove_member(txn, super::JOB, id, mname).map_err(|e| db_err(&e))?;
        Ok(format!("Member {mbr} removed from {qual}."))
    })
}

/// `clrpfm LIB/FILE [MBR(M)]`.
pub fn clrpfm(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let mbr = member_spec(p, "MBR")?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        let n = util::clrpfm(txn, super::JOB, id, mbr).map_err(|e| db_err(&e))?;
        Ok(format!("Member cleared: {n} records removed."))
    })
}

/// `inzpfm LIB/FILE [MBR(M)] [RECORDS(*DFT|*DLT)] [TOTRCDS(n)]`.
pub fn inzpfm(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let mbr = member_spec(p, "MBR")?;
    let deleted = match p.upper("RECORDS").as_deref() {
        None | Some("*DFT") => false,
        Some("*DLT") => true,
        Some(v) => return Err(format!("CPD0084 RECORDS({v}) is not *DFT or *DLT")),
    };
    let n: u64 = p.num::<u64>("TOTRCDS")?.unwrap_or(1);
    let (store, ctx) = db(system)?;
    let job = ctx.session.job();
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        let n = util::inzpfm(txn, job, id, mbr, n, deleted).map_err(|e| db_err(&e))?;
        Ok(format!("{n} record(s) initialized in {qual}."))
    })
}

/// `chgpf LIB/FILE SRCFILE(LIB/SRC) [SRCMBR(M)]`: the file takes the
/// member's DDS, its data converted when the format changed.
pub fn chgpf(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let name = object_name(&qual)?;
    let (store, ctx) = db(system)?;
    let job = ctx.session.job();
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        let fname = SqlContext::sys_name(&name.name).map_err(|e| e.to_string())?;
        let (src, text) = source(txn, ctx, p, fname)?;
        let (refs, _) = refs_for(txn, ctx, &text);
        let comp = compile_pf(
            &src,
            &refs,
            &CompileOpts {
                file: Some(fname.to_string()),
                ..CompileOpts::default()
            },
        );
        let Some(def) = comp.definition else {
            return Err(format!("File {qual} not changed.\n{}", comp.listing));
        };
        db::chg::chgpf_srcfile(txn, job, id, &def).map_err(|e| db_err(&e))?;
        Ok(format!(
            "File {qual} changed from source: {} fields, {} key(s).",
            def.format.fields.len(),
            def.format.keys.len()
        ))
    })
}

/// `rgzpfm LIB/FILE [MBR(M)] [KEYFILE(*FILE|LIB/LF)]`.
pub fn rgzpfm(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let mbr = member_spec(p, "MBR")?;
    let keyfile = p.upper("KEYFILE");
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        let kf = match keyfile.as_deref() {
            None | Some("*NONE") => KeyFile::None,
            Some("*FILE") => KeyFile::File,
            Some(lf) => KeyFile::Logical {
                file: file_id(txn, ctx, lf)?,
                member: MemberSpec::First,
            },
        };
        let r = util::rgzpfm(txn, super::JOB, id, mbr, &kf).map_err(|e| db_err(&e))?;
        Ok(format!(
            "Member reorganized: {} records kept, {} deleted slots removed.",
            r.records, r.deleted_removed
        ))
    })
}

fn errlvl(p: &Params) -> Result<ErrLvl, String> {
    Ok(match p.upper("ERRLVL").as_deref() {
        None => ErrLvl::None,
        Some("*NOMAX") => ErrLvl::NoMax,
        Some(n) => ErrLvl::Count(
            n.parse()
                .map_err(|_| format!("ERRLVL({n}): a number is needed"))?,
        ),
    })
}

/// `cpyf FROMFILE(LIB/F) TOFILE(LIB/F) [...]`.
pub fn cpyf(system: &mut System, p: &Params) -> Result<String, String> {
    let from = p.pos_or(0, "FROMFILE")?;
    let to = p.pos_or(1, "TOFILE")?;
    let mut opts = CpyfOpts {
        from_member: member_spec(p, "FROMMBR")?,
        now: now(),
        errlvl: errlvl(p)?,
        ..CpyfOpts::default()
    };
    opts.to_member = match p.upper("TOMBR").as_deref() {
        None | Some("*FIRST") => ToMember::First,
        Some("*FROMMBR") => ToMember::FromMbr,
        Some(m) => ToMember::Named(Name::parse(m).map_err(|e| format!("TOMBR({m}): {e}"))?),
    };
    opts.mbropt = match p.upper("MBROPT").as_deref() {
        None | Some("*NONE") => MbrOpt::None,
        Some("*ADD") => MbrOpt::Add,
        Some("*REPLACE") => MbrOpt::Replace,
        Some("*UPDADD") => MbrOpt::UpdAdd,
        Some(o) => {
            return Err(format!(
                "MBROPT({o}) is not *NONE, *ADD, *REPLACE or *UPDADD"
            ));
        }
    };
    opts.fmtopt = match p
        .upper("FMTOPT")
        .as_deref()
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
    {
        None => FmtOpt::None,
        Some(w) if w == ["*NONE"] => FmtOpt::None,
        Some(w) if w == ["*NOCHK"] => FmtOpt::NoChk,
        Some(w) if w == ["*MAP"] => FmtOpt::Map,
        Some(w) if w == ["*DROP"] => FmtOpt::Drop,
        Some(w) if w.len() == 2 && w.contains(&"*MAP") && w.contains(&"*DROP") => FmtOpt::MapDrop,
        Some(w) if w == ["*CVTSRC"] => FmtOpt::CvtSrc,
        Some(w) => return Err(format!("FMTOPT({}) is not understood", w.join(" "))),
    };
    opts.crtfile = p.yes("CRTFILE");
    opts.fromrcd = p.num("FROMRCD")?;
    opts.torcd = p.num("TORCD")?;
    opts.nbrrcds = p.num("NBRRCDS")?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let from_id = file_id(txn, ctx, &from)?;
        let to_name = object_name(&to)?;
        let target = match ctx.resolve_file(txn, &to_name) {
            Ok(id) => Target::File(id),
            Err(_) if opts.crtfile => Target::Create {
                library: ctx.library_for(txn, &to_name).map_err(|e| e.to_string())?,
                name: SqlContext::sys_name(&to_name.name).map_err(|e| e.to_string())?,
            },
            Err(e) => return Err(e.to_string()),
        };
        let r = util::cpyf(txn, super::JOB, from_id, &target, &opts).map_err(|e| db_err(&e))?;
        Ok(format!(
            "{} records copied to member {}, {} excluded, {} not copied{}.",
            r.copied,
            r.to_member,
            r.excluded,
            r.errors,
            if r.created.is_some() {
                "; file created"
            } else {
                ""
            }
        ))
    })
}

fn impf_opts(p: &Params) -> Result<ImpfOpts, String> {
    let mut o = ImpfOpts {
        errlvl: errlvl(p)?,
        ..ImpfOpts::default()
    };
    match p.upper("DTAFMT").as_deref() {
        None | Some("*DLM") => {}
        Some("*FIXED") => o.dtafmt = DtaFmt::Fixed,
        Some(x) => return Err(format!("DTAFMT({x}) is not *DLM or *FIXED")),
    }
    match p.upper("RCDDLM").as_deref() {
        None | Some("*EOR") => {}
        Some("*LF") => o.rcddlm = RcdDlm::Lf,
        Some("*CR") => o.rcddlm = RcdDlm::Cr,
        Some("*CRLF") => o.rcddlm = RcdDlm::Crlf,
        Some(x) => return Err(format!("RCDDLM({x}) is not *EOR, *LF, *CR or *CRLF")),
    }
    match p.upper("RMVBLANK").as_deref() {
        None | Some("*NONE") => {}
        Some("*LEADING") => o.rmvblank = RmvBlank::Leading,
        Some("*TRAILING") => o.rmvblank = RmvBlank::Trailing,
        Some("*BOTH") => o.rmvblank = RmvBlank::Both,
        Some(x) => return Err(format!("RMVBLANK({x}) is not understood")),
    }
    if let Some(s) = p.kw("STRDLM") {
        o.strdlm = if s.eq_ignore_ascii_case("*NONE") {
            None
        } else {
            s.trim_matches('\'').chars().next()
        };
    }
    if let Some(s) = p.kw("FLDDLM") {
        o.flddlm = s.trim_matches('\'').chars().next().unwrap_or(',');
    }
    if let Some(s) = p.kw("DECPNT") {
        o.decpnt = if s.eq_ignore_ascii_case("*COMMA") {
            ','
        } else {
            '.'
        };
    }
    if let Some(f) = p.upper("DATFMT") {
        o.datfmt = Some(
            DateFmt::from_word(&f).ok_or_else(|| format!("DATFMT({f}) is not a date format"))?,
        );
    }
    if let Some(f) = p.upper("TIMFMT") {
        o.timfmt = Some(
            TimeFmt::from_word(&f).ok_or_else(|| format!("TIMFMT({f}) is not a time format"))?,
        );
    }
    o.nulls = p.yes("NULLS") || p.upper("RPLNULLVAL").as_deref() == Some("*NO");
    Ok(o)
}

/// `cpytoimpf FROMFILE(LIB/F) [TOSTMF(PATH|NAME)] [...]`: a path (with
/// a `/`) is a stream file of the job's IFS (`STMFOPT`, `STMFCCSID` or
/// `STMFCODPAG`, `STMFAUT`); a plain name is an `xfer` buffer.
pub fn cpytoimpf(system: &mut System, p: &Params) -> Result<String, String> {
    let from = p.pos_or(0, "FROMFILE")?;
    let mbr = member_spec(p, "FROMMBR")?;
    let opts = impf_opts(p)?;
    let stmf = p.kw("TOSTMF").map(unquoted);
    let write = match &stmf {
        Some(path) if path.contains('/') => Some(stmf_write(p)?),
        _ => None,
    };
    let (store, ctx) = db(system)?;
    let text = with_txn(store, |txn| {
        let id = file_id(txn, ctx, &from)?;
        let bytes = util::cpytoimpf(txn, super::JOB, id, mbr, &opts).map_err(|e| db_err(&e))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })?;
    let n = text.lines().count();
    match (stmf, write) {
        (Some(path), Some(w)) => {
            let tag = job_ifs(system, |c| {
                ifscmd::stmf::impf::write_text(c, &path, text.as_bytes(), w)
            })
            .map_err(|m| stmf_refused(&m, &path))?;
            Ok(format!("{n} records copied to {path} (CCSID {tag})."))
        }
        (Some(name), None) => {
            system.svc.xfer.insert(name.clone(), text);
            Ok(format!("{n} records copied to {name}."))
        }
        (None, _) => Ok(format!("--- {n} records\n{text}--- end")),
    }
}

/// `cpyfrmimpf FROMSTMF(PATH|NAME) TOFILE(LIB/F) [...]`: a path is read
/// from the job's IFS in its CCSID (or `STMFCCSID`'s); a name is an
/// `xfer` buffer.
pub fn cpyfrmimpf(system: &mut System, p: &Params) -> Result<String, String> {
    let stmf = unquoted(&p.pos_or(0, "FROMSTMF")?);
    let to = p.pos_or(1, "TOFILE")?;
    let mbr = member_spec(p, "TOMBR")?;
    let opts = impf_opts(p)?;
    let text = if stmf.contains('/') {
        let ccsid = from_ccsid(p)?;
        let b = job_ifs(system, |c| ifscmd::stmf::impf::read_text(c, &stmf, ccsid))
            .map_err(|m| stmf_refused(&m, &stmf))?;
        String::from_utf8_lossy(&b).into_owned()
    } else {
        system
            .svc
            .xfer
            .get(&stmf)
            .cloned()
            .ok_or_else(|| format!("{stmf}: no such buffer; xfer put it first"))?
    };
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &to)?;
        let r = util::cpyfrmimpf(txn, super::JOB, id, mbr, text.as_bytes(), &opts)
            .map_err(|e| db_err(&e))?;
        let mut out = format!("{} records copied, {} not copied.", r.records, r.errors);
        for (n, why) in r.skipped.iter().take(20) {
            let _ = write!(out, "\n  record {n}: {why}");
        }
        Ok(out)
    })
}

/// A value without its apostrophes, doubled ones made single.
fn unquoted(v: &str) -> String {
    let t = v.trim();
    match t.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        Some(inner) => inner.replace("''", "'"),
        None => t.to_string(),
    }
}

/// `STMFCCSID`, or `STMFCODPAG` when that is given instead.
fn stmf_ccsid(p: &Params) -> Result<ifscmd::stmf::StmfCcsid, String> {
    use ifscmd::stmf::StmfCcsid;
    use ifscmd::stmf::impf::ccsid_word;
    let parse = |k: &str| -> Result<StmfCcsid, String> {
        let w = p.kw(k).unwrap_or("");
        ccsid_word(w).ok_or_else(|| format!("{k}({w}) is not a CCSID"))
    };
    let (c, cp) = (parse("STMFCCSID")?, parse("STMFCODPAG")?);
    match (c, cp) {
        (StmfCcsid::Stmf, cp) => Ok(cp),
        (c, StmfCcsid::Stmf) => Ok(c),
        _ => Err("CPDA0B9 give STMFCCSID or STMFCODPAG, not both".into()),
    }
}

/// `FROMCCSID` for a stream file: `*FILE` is its tag.
fn from_ccsid(p: &Params) -> Result<ifscmd::stmf::StmfCcsid, String> {
    let w = p.kw("FROMCCSID").unwrap_or("");
    match w.trim().to_ascii_uppercase().as_str() {
        "*FILE" => Ok(ifscmd::stmf::StmfCcsid::Stmf),
        _ => ifscmd::stmf::impf::ccsid_word(w).ok_or_else(|| format!("FROMCCSID({w}) is not a CCSID")),
    }
}

/// How `TOSTMF` is written.
fn stmf_write(p: &Params) -> Result<ifscmd::stmf::impf::Write, String> {
    use ifscmd::stmf::impf::{Write, aut_word, opt_word};
    let w = p.kw("STMFOPT").unwrap_or("");
    let a = p.kw("STMFAUT").unwrap_or("");
    Ok(Write {
        opt: opt_word(w).ok_or_else(|| format!("STMFOPT({w}) is not *NONE, *ADD or *REPLACE"))?,
        ccsid: stmf_ccsid(p)?,
        aut: aut_word(a).ok_or_else(|| format!("STMFAUT({a}) is not understood"))?,
    })
}

/// A refusal as the console shows it: the message identifier first, so
/// the command's escape is that message.
fn stmf_refused(m: &ifscmd::Msg, path: &str) -> String {
    format!("{} {path}: the stream file was not copied", m.id)
}

/// Run `f` against the job's IFS, as the job, after the store's
/// transaction (the IFS lends the store itself).
fn job_ifs<T>(system: &mut System, f: impl FnOnce(&dyn ifs::client::IfsClient) -> T) -> T {
    crate::cpp::stmf::with_job(system, f)
}

/// `dspfd LIB/FILE [TYPE(...)]`.
pub fn dspfd(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let ty = match p.upper("TYPE") {
        None => db::dsp::FdType::All,
        Some(t) => {
            db::dsp::FdType::parse(&t).ok_or_else(|| format!("TYPE({t}) is not understood"))?
        }
    };
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        Ok(db::dsp::dspfd(txn, id).map_err(|e| db_err(&e))?.render(ty))
    })
}

/// `dspffd LIB/FILE`.
pub fn dspffd(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        Ok(db::dsp::dspffd(txn, id).map_err(|e| db_err(&e))?.render())
    })
}

/// `dsppfm LIB/FILE [MBR(M) FROMRCD(n) NBRRCDS(n) OUTPUT(*HEX) DELETED(*YES)]`.
pub fn dsppfm(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let mbr = member_spec(p, "MBR")?;
    let from: u64 = p.num("FROMRCD")?.unwrap_or(1);
    let max: usize = p.num("NBRRCDS")?.unwrap_or(20);
    let hex = p.upper("OUTPUT").is_some_and(|o| o == "*HEX");
    let deleted = p.yes("DELETED");
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        let views = db::dsp::dsppfm(txn, id, mbr, from, max, deleted).map_err(|e| db_err(&e))?;
        let mut out = if hex {
            db::dsp::render_hex(&views)
        } else {
            db::dsp::render_char(&views)
        };
        if let Some(last) = views.last()
            && views.len() == max
        {
            let _ = writeln!(out, "More: dsppfm {qual} FROMRCD({})", last.rrn + 1);
        }
        Ok(out)
    })
}

/// `dspdbr LIB/FILE [MBR(M)]`.
pub fn dspdbr(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let mbr = p.kw("MBR").map(|_| member_spec(p, "MBR")).transpose()?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        Ok(db::dsp::dspdbr(txn, id, mbr)
            .map_err(|e| db_err(&e))?
            .render())
    })
}

/// `srcput LIB/FILE MBR [SRCTYPE(T)] [FROMSTMF(NAME)]`: the member is
/// added when missing.
pub fn srcput(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let mbr = p.pos_or(1, "MBR")?;
    let mname = Name::parse(&mbr).map_err(|e| format!("{mbr}: {e}"))?;
    let srctype = match p.upper("SRCTYPE") {
        Some(t) => Some(Name::parse(&t).map_err(|e| format!("SRCTYPE({t}): {e}"))?),
        None => None,
    };
    let stmf = p.kw("FROMSTMF").map_or_else(|| mbr.clone(), str::to_string);
    let text = system
        .svc
        .xfer
        .get(&stmf)
        .cloned()
        .ok_or_else(|| format!("{stmf}: no such buffer; xfer put it first"))?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        let desc = file::describe(txn, id).map_err(|e| db_err(&e))?;
        if desc.member(MemberSpec::Name(mname)).is_none() {
            file::add_member(txn, id, mname, "", srctype, now()).map_err(|e| db_err(&e))?;
        }
        let n = db::source::srcput(
            txn,
            super::JOB,
            id,
            MemberSpec::Name(mname),
            &text,
            srctype,
            now(),
        )
        .map_err(|e| db_err(&e))?;
        Ok(format!(
            "{n} source records written to {qual} member {mbr}."
        ))
    })
}

/// `srcget LIB/FILE MBR [TOSTMF(NAME)]`.
pub fn srcget(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let mbr = p.pos_or(1, "MBR")?;
    let mname = Name::parse(&mbr).map_err(|e| format!("{mbr}: {e}"))?;
    let stmf = p.kw("TOSTMF").map(str::to_string);
    let (store, ctx) = db(system)?;
    let text = with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        db::source::srcget(txn, id, MemberSpec::Name(mname)).map_err(|e| db_err(&e))
    })?;
    let n = text.lines().count();
    match stmf {
        Some(name) => {
            system.svc.xfer.insert(name.clone(), text);
            Ok(format!("{n} source records copied to {name}."))
        }
        None => Ok(format!("--- {n} records\n{text}--- end")),
    }
}
