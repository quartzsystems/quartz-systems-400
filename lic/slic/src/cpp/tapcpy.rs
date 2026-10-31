// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Tape files and the copies to and from tape: `CRTTAPF` and `CHGTAPF`
//! (a `*FILE` with attribute `TAPF`, `medio::tapf::TapeFileDef` its
//! payload), `CPYTOTAP` (a member's records to a tape file) and
//! `CPYFRMTAP` (a tape file's records into a member, or a listing with
//! `TOFILE(*PRINT)`). `DLTF` and `DSPFD` of a tape file come here too.

use std::fmt::Write as _;

use cmd::block::Parms;
use db::file::FileKind;
use medio::MediaError;
use medio::tapf::{CopyIn, CopyOut, TapeFileDef, copy_from_tape, copy_to_tape, lengths, record_format};
use qsname::{MemberSpec, Name};
use sql::SqlContext;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;
use stg::proto::CmdReply;
use tape::label::{Date, Expiry, FileLabel};
use tape::{SeqNbr, TapeTransport, VolumeProvider};

use super::direct::failed;
use super::done;
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::media::drive::{self, TapeJob};
use crate::stgsvc::media::{operator, stream};
use crate::svc::db::{db, file_id, object_name, with_txn};

/// Tape file made, changed; records copied to and from tape.
const TAPF_CREATED: &str = "CPC7301";
const TAPF_CHANGED: &str = "CPC7302";
const COPIED_TO: &str = "CPC6778";
const COPIED_FROM: &str = "CPC6779";

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CRTTAPF" => crttapf(system, parms),
        "CHGTAPF" => chgtapf(system, parms),
        "CPYTOTAP" => cpytotap(system, parms),
        "CPYFRMTAP" => cpyfrmtap(system, parms),
        _ => return None,
    };
    Some(match r {
        Ok(reply) => reply,
        Err(e) => failed(name, &e),
    })
}

/// A qualified parameter as `LIB/NAME`, or `NAME` for `*LIBL` and
/// `*CURLIB`.
fn qual(parms: &Parms, kwd: &str) -> String {
    let (n, l) = parms.qual(kwd);
    let (n, l) = (n.to_ascii_uppercase(), l.to_ascii_uppercase());
    if l.is_empty() || l.starts_with('*') { n } else { format!("{l}/{n}") }
}

fn values(parms: &Parms, kwd: &str) -> Vec<String> {
    parms
        .list(kwd)
        .iter()
        .map(|v| v.source().trim().trim_matches('\'').to_ascii_uppercase())
        .filter(|v| !v.is_empty() && !v.starts_with('*'))
        .collect()
}

/// Whether `id` is a tape file.
fn is_tapf(txn: &mut Txn<'_>, id: ObjectId) -> bool {
    ops::read_header(txn, id).is_ok_and(|(_, h)| h.id.type_code == types::FILE && ccsid::decode_trimmed(&h.attribute) == "TAPF")
}

fn tapf_id(txn: &mut Txn<'_>, ctx: &SqlContext, qual: &str) -> Result<ObjectId, String> {
    let id = file_id(txn, ctx, qual)?;
    if is_tapf(txn, id) {
        Ok(id)
    } else {
        Err(format!("CPF4123 {qual} is not a tape file"))
    }
}

fn read_def(txn: &mut Txn<'_>, id: ObjectId) -> Result<TapeFileDef, String> {
    let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
    TapeFileDef::decode(&b).map_err(|e| format!("CPF6708 {e}"))
}

/// The tape file `qual`.
fn load(system: &mut System, qual: &str) -> Result<TapeFileDef, String> {
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = tapf_id(txn, ctx, qual)?;
        read_def(txn, id)
    })
}

/// `EXPDATE` as `CYYMMDD`, `*PERM`, `*NONE`; `None` for `*SAME` or a
/// special value this command takes from elsewhere (`*TAPF`).
fn expiry(v: &str) -> Option<Expiry> {
    let v = v.trim();
    match v {
        "*PERM" => Some(Expiry::Perm),
        "*NONE" => Some(Expiry::None),
        "" | "*SAME" | "*TAPF" => None,
        d => {
            let n: u32 = d.parse().ok()?;
            let (c, yy, mm, dd) = (n / 1_000_000, n / 10_000 % 100, n / 100 % 100, n % 100);
            let year = u16::try_from(1900 + c * 100 + yy).ok()?;
            Date::from_ymd(year, u8::try_from(mm).ok()?, u8::try_from(dd).ok()?).map(Expiry::Date)
        }
    }
}

/// The keywords of `CRTTAPF` and `CHGTAPF` over `d`.
fn apply(d: &mut TapeFileDef, parms: &Parms) {
    let name = |k: &str| parms.name(k);
    let list = |k: &str| -> Option<Vec<String>> {
        let raw: Vec<String> = parms.list(k).iter().map(|v| v.source().trim().to_ascii_uppercase()).collect();
        match raw.first().map(String::as_str) {
            None | Some("*SAME") => None,
            Some("*NONE") => Some(Vec::new()),
            _ => Some(raw.into_iter().filter(|v| !v.is_empty()).collect()),
        }
    };
    if let Some(v) = list("DEV") {
        d.devs = v;
    }
    if let Some(v) = list("VOL") {
        d.vols = v;
    }
    match name("REELS").as_str() {
        "*SL" => d.unlabeled = false,
        "*NL" => d.unlabeled = true,
        _ => {}
    }
    match parms.int("SEQNBR") {
        Some(-1) => d.seq = None,
        Some(n) if n > 0 => d.seq = u32::try_from(n).ok(),
        _ => {}
    }
    let label = parms.text("LABEL").trim().trim_matches('\'').to_ascii_uppercase();
    match label.as_str() {
        "" | "*SAME" => {}
        "*NONE" => d.label.clear(),
        l => d.label = l.to_string(),
    }
    for (k, slot) in [("RCDLEN", &mut d.rcdlen), ("BLKLEN", &mut d.blklen)] {
        if let Some(n) = parms.int(k).filter(|n| *n >= 0) {
            *slot = u32::try_from(n).unwrap_or(0);
        }
    }
    let t = name("RCDBLKTYP");
    if t.starts_with('*') && t != "*SAME" {
        d.rcdblk = t;
    }
    match name("CODE").as_str() {
        "*ASCII" => d.ascii = true,
        "*EBCDIC" => d.ascii = false,
        _ => {}
    }
    if let Some(e) = expiry(&parms.text("EXPDATE")) {
        d.expires = e;
    }
    let e = name("ENDOPT");
    if e.starts_with('*') && e != "*SAME" {
        d.end_opt = medio::tapf::parse_end_opt(&e);
    }
    let text = parms.text("TEXT");
    match text.trim() {
        "*SAME" => {}
        "*BLANK" => d.text.clear(),
        t => d.text = t.trim_matches('\'').to_string(),
    }
}

fn crttapf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let q = qual(parms, "FILE");
    let mut d = TapeFileDef::default();
    apply(&mut d, parms);
    let owner = crate::stgsvc::security::actor()
        .map(|p| p.user.clone())
        .or_else(|| system.svc_user.clone())
        .unwrap_or_else(|| "QSYS".into());
    let (store, ctx) = db(system)?;
    let lib = with_txn(store, |txn| {
        let name = object_name(&q)?;
        let lib = ctx.library_for(txn, &name).map_err(|e| e.to_string())?;
        let fname = SqlContext::sys_name(&name.name).map_err(|e| e.to_string())?;
        let stamp = wm::obj::Stamp {
            owner,
            system: "QS400".into(),
            now: now_unix(),
        };
        wm::obj::create_attr(txn, lib, (fname, types::FILE, "TAPF"), &d.text, &stamp, &d.encode())
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF7302")))?;
        let (_, h) = ops::read_header(txn, lib).map_err(|e| e.to_string())?;
        Ok(h.name.to_string())
    })?;
    let n = q.rsplit('/').next().unwrap_or(&q).to_string();
    Ok(done(format!("{TAPF_CREATED} Tape file {n} created in library {lib}.\n"), TAPF_CREATED, &[(&n, 10), (&lib, 10)]))
}

fn chgtapf(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let q = qual(parms, "FILE");
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = tapf_id(txn, ctx, &q)?;
        let mut d = read_def(txn, id)?;
        apply(&mut d, parms);
        wm::obj::write(txn, id, &d.encode()).map_err(|e| e.to_string())?;
        ops::change_text(txn, id, &d.text, now_unix()).map_err(|e| e.to_string())
    })?;
    let n = q.rsplit('/').next().unwrap_or(&q).to_string();
    let lib = q.split_once('/').map_or("*LIBL", |(l, _)| l).to_string();
    Ok(done(format!("{TAPF_CHANGED} Tape file {n} changed.\n"), TAPF_CHANGED, &[(&n, 10), (&lib, 10)]))
}

/// `DLTF` of a tape file; `None` when `qual` is not one.
pub fn dltf(system: &mut System, qual: &str) -> Option<Result<String, String>> {
    let (store, ctx) = db(system).ok()?;
    let found = with_txn(store, |txn| {
        let id = file_id(txn, ctx, qual)?;
        if !is_tapf(txn, id) {
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

/// `DSPFD` of a tape file; `None` when `qual` is not one.
pub fn dspfd(system: &mut System, qual: &str) -> Option<Result<String, String>> {
    let (store, ctx) = db(system).ok()?;
    let r = with_txn(store, |txn| {
        let id = file_id(txn, ctx, qual)?;
        if !is_tapf(txn, id) {
            return Ok(None);
        }
        let d = read_def(txn, id)?;
        Ok(Some(format!("File  . . . . . . . : {qual}\n{}", d.describe())))
    });
    match r {
        Ok(None) => None,
        Ok(Some(s)) => Some(Ok(s)),
        Err(e) => Some(Err(e)),
    }
}

/// A member's records: the data of a source member, every field of a
/// physical one. The member's name, the records, the record length,
/// whether the file is EBCDIC.
/// A member's name, records, record length and whether it is EBCDIC.
type Records = (String, Vec<Vec<u8>>, usize, bool);

fn member_records(system: &mut System, file: &str, mbr: &MemberSpec) -> Result<Records, String> {
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, file)?;
        let desc = db::file::describe(txn, id).map_err(|e| crate::svc::db::db_err(&e))?;
        let ebcdic = ccsid::conv::Codec::of(desc.attr.ccsid).ok().flatten().is_none_or(|c| c.is_ebcdic());
        let m = db::Member::open_by(txn, id, *mbr).map_err(|e| crate::svc::db::db_err(&e))?;
        let name = m.name().to_string();
        drop(m);
        let spec = MemberSpec::Name(Name::parse(&name).map_err(|e| e.to_string())?);
        if desc.attr.kind == FileKind::Source {
            let width = db::source::data_width(txn, id).map_err(|e| crate::svc::db::db_err(&e))?;
            let recs = db::source::records(txn, id, spec).map_err(|e| crate::svc::db::db_err(&e))?;
            Ok((name, recs.into_iter().map(|(_, _, d)| d).collect(), width, ebcdic))
        } else {
            let m = db::Member::open_by(txn, id, spec).map_err(|e| crate::svc::db::db_err(&e))?;
            let len = m.format().format.record_len as usize;
            let recs = m
                .scan(txn, 1, usize::MAX)
                .map_err(|e| crate::svc::db::db_err(&e))?
                .into_iter()
                .filter(|s| !s.deleted)
                .map(|s| s.record)
                .collect();
            Ok((name, recs, len, ebcdic))
        }
    })
}

/// EBCDIC records as ASCII, for `CODE(*ASCII)`.
fn to_ascii(r: &[u8]) -> Vec<u8> {
    ccsid::from_ebcdic(r).chars().map(|c| u8::try_from(c).ok().filter(u8::is_ascii).unwrap_or(b'?')).collect()
}

/// ASCII records back as EBCDIC.
fn from_ascii(r: &[u8]) -> Vec<u8> {
    r.iter()
        .map(|b| ccsid::byte_of(char::from(*b)).unwrap_or(0x6F))
        .collect()
}

struct ToTape(CopyOut);

impl TapeJob for ToTape {
    type Out = tape::volume::WriteSummary;
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, p: &mut dyn VolumeProvider<D>) -> Result<Self::Out, MediaError> {
        copy_to_tape(d, p, &self.0)
    }
}

struct FromTape(CopyIn);

impl TapeJob for FromTape {
    type Out = (Vec<Vec<u8>>, FileLabel);
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, p: &mut dyn VolumeProvider<D>) -> Result<Self::Out, MediaError> {
        copy_from_tape(d, p, &self.0)
    }
}

/// A `*TAPF`-defaulted special value: `None` when the command takes the
/// tape file's.
fn own(v: &str) -> Option<&str> {
    (!v.is_empty() && v != "*TAPF").then_some(v)
}

/// The device: the command's first, or the tape file's.
fn device(parms: &Parms, kwd: &str, d: &TapeFileDef) -> Result<String, String> {
    values(parms, kwd)
        .into_iter()
        .next()
        .or_else(|| d.devs.first().cloned())
        .ok_or_else(|| format!("CPF6708 no device: give {kwd} or set DEV on the tape file"))
}

fn cpytotap(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let def = load(system, &qual(parms, "TOFILE"))?;
    let from = qual(parms, "FROMFILE");
    let mbr = match parms.name("FROMMBR").as_str() {
        "" | "*FIRST" => MemberSpec::First,
        m => MemberSpec::Name(Name::parse(m).map_err(|e| format!("CPD0084 FROMMBR({m}): {e}"))?),
    };
    let (mname, mut recs, file_len, ebcdic) = member_records(system, &from, &mbr)?;
    if let Some(n) = parms.int("NBRRCDS").filter(|n| *n > 0) {
        recs.truncate(usize::try_from(n).unwrap_or(usize::MAX));
    }
    let dev = device(parms, "TODEV", &def)?;
    let vols = {
        let v = values(parms, "TOVOL");
        if v.is_empty() { def.vols.clone() } else { v }
    };
    let seq = match parms.int("TOSEQNBR").unwrap_or(0) {
        -1 => SeqNbr::End,
        0 => def.seq.map_or(SeqNbr::End, SeqNbr::Number),
        n => SeqNbr::Number(u32::try_from(n).unwrap_or(1)),
    };
    let label = match parms.text("TOLABEL").trim().trim_matches('\'').to_ascii_uppercase().as_str() {
        "" | "*FROMMBR" => mname.clone(),
        "*TAPF" => def.label.clone(),
        "*NONE" => String::new(),
        l => l.to_string(),
    };
    let unlabeled = match parms.elems("TOREELS").first().copied().flatten().map(|v| v.source().trim().to_ascii_uppercase()) {
        Some(v) if v == "*NL" => true,
        Some(v) if v == "*SL" => false,
        _ => def.unlabeled,
    };
    let rcdblk = own(&parms.name("TORCDBLK")).map_or_else(|| def.rcdblk.clone(), str::to_string);
    let (format, blocked) = record_format(&rcdblk);
    let rcdlen = match parms.int("TORCDLEN").unwrap_or(0) {
        0 => u32::try_from(file_len).unwrap_or(0),
        -1 => def.rcdlen,
        n => u32::try_from(n).unwrap_or(0),
    };
    let blklen = match parms.int("TOBLKLEN").unwrap_or(0) {
        0 => def.blklen,
        -1 => 0,
        n => u32::try_from(n).unwrap_or(0),
    };
    let longest = u32::try_from(recs.iter().map(Vec::len).max().unwrap_or(file_len)).unwrap_or(0);
    let (rcdlen, blklen) = lengths(format, blocked, rcdlen, blklen, longest);
    let ascii = def.ascii;
    if ascii && ebcdic {
        recs = recs.iter().map(|r| to_ascii(r)).collect();
    }
    let end_opt = own(&parms.name("TOENDOPT")).map_or(def.end_opt, medio::tapf::parse_end_opt);
    let expires = expiry(&parms.text("TOEXPDATE")).unwrap_or(def.expires);
    let today = stream::today();
    let job = system.jobs.get(system.current_job).map_or_else(|| "QSYS".to_string(), |j| j.name.clone());
    let n = recs.len();
    let mut copy = CopyOut {
        vols: vols.clone(),
        seq,
        label: FileLabel {
            file_id: label.chars().take(17).collect(),
            created: Some(today),
            expires,
            record_format: format,
            block_length: blklen,
            record_length: rcdlen,
            job_step: job.chars().take(17).collect(),
            ..FileLabel::default()
        },
        unlabeled,
        today: Some(today),
        allow_active: false,
        end_opt,
        records: recs,
        pad: if ascii || !ebcdic { b' ' } else { 0x40 },
    };
    let s = loop {
        let mut job = ToTape(copy.clone());
        match drive::with_tape(system, &dev, &vols, true, &mut job) {
            Err(e) if e.starts_with(operator::ACTIVE_FILE) && !copy.allow_active => {
                let a = operator::inquiry(system, operator::ACTIVE_FILE, &[(&dev, 10)], "C");
                if a.is("I") {
                    copy.allow_active = true;
                    continue;
                }
                return Err(format!("{} {e}; {}", stream::SAVE_ENDED, a.shown()));
            }
            r => break r?,
        }
    };
    let shown = if label.is_empty() { "*NONE".to_string() } else { label };
    let count = n.to_string();
    Ok(done(
        format!(
            "{COPIED_TO} {n} records copied to tape file {shown}, sequence {}, on volume(s) {}.\n",
            s.file_seq,
            s.volumes.join(" ")
        ),
        COPIED_TO,
        &[(&count, 10), (&shown, 17), (&dev, 10)],
    ))
}

/// A listing of records, character or character and hexadecimal.
fn listing(recs: &[Vec<u8>], hex: bool, ebcdic: bool) -> String {
    let mut s = String::new();
    for (i, r) in recs.iter().enumerate() {
        let text = if ebcdic { ccsid::from_ebcdic(r) } else { String::from_utf8_lossy(r).into_owned() };
        let _ = writeln!(s, "{:>6}  {}", i + 1, text.trim_end());
        if hex {
            let _ = write!(s, "        ");
            for b in r {
                let _ = write!(s, "{b:02X}");
            }
            s.push('\n');
        }
    }
    s
}

#[allow(clippy::too_many_lines)]
fn cpyfrmtap(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let def = load(system, &qual(parms, "FROMFILE"))?;
    let dev = device(parms, "FROMDEV", &def)?;
    let vols = {
        let v = values(parms, "FROMVOL");
        if v.is_empty() { def.vols.clone() } else { v }
    };
    let unlabeled = match parms.elems("FROMREELS").first().copied().flatten().map(|v| v.source().trim().to_ascii_uppercase()) {
        Some(v) if v == "*NL" => true,
        Some(v) if v == "*SL" => false,
        _ => def.unlabeled,
    };
    let label = match parms.text("FROMLABEL").trim().trim_matches('\'').to_ascii_uppercase().as_str() {
        "" | "*TAPF" => Some(def.label.clone()).filter(|l| !l.is_empty()),
        "*NONE" => None,
        l => Some(l.to_string()),
    };
    let seq = match parms.int("FROMSEQNBR").unwrap_or(0) {
        0 => def.seq,
        n => u32::try_from(n).ok(),
    };
    let rcdblk = own(&parms.name("FROMRCDBLK")).map(str::to_string).or_else(|| unlabeled.then(|| def.rcdblk.clone()));
    let rcdlen = match parms.int("FROMRCDLEN").unwrap_or(0) {
        0 => def.rcdlen,
        -1 => 0,
        n => u32::try_from(n).unwrap_or(0),
    };
    let c = CopyIn {
        vols: vols.clone(),
        seq: if label.is_some() && parms.int("FROMSEQNBR").unwrap_or(0) == 0 { None } else { seq },
        label: label.clone().filter(|_| !unlabeled),
        unlabeled,
        end_opt: own(&parms.name("FROMENDOPT")).map_or(def.end_opt, medio::tapf::parse_end_opt),
        format: rcdblk.as_deref().map(|t| record_format(t).0),
        rcdlen,
    };
    let (mut recs, l) = drive::with_tape(system, &dev, &vols, false, &mut FromTape(c))?;
    if let Some(n) = parms.int("NBRRCDS").filter(|n| *n > 0) {
        recs.truncate(usize::try_from(n).unwrap_or(usize::MAX));
    }
    if def.ascii {
        recs = recs.iter().map(|r| from_ascii(r)).collect();
    }
    let shown = if l.file_id.is_empty() { "*NONE".to_string() } else { l.file_id.clone() };
    let n = recs.len();
    let count = n.to_string();
    let (to, tolib) = parms.qual("TOFILE");
    if to.eq_ignore_ascii_case("*PRINT") {
        let mut text = format!("{COPIED_FROM} {n} records copied from tape file {shown}.\n");
        text.push_str(&listing(&recs, parms.name("OUTFMT") == "*HEX", true));
        return Ok(done(text, COPIED_FROM, &[(&count, 10), (&shown, 17), ("*PRINT", 10)]));
    }
    let target = qual(parms, "TOFILE");
    let _ = tolib;
    let mbr = match parms.name("TOMBR").as_str() {
        "" | "*FROMLABEL" => Some(l.file_id.clone()).filter(|m| !m.is_empty()).or_else(|| label.clone()),
        "*FIRST" => None,
        m => Some(m.to_string()),
    };
    let mbropt = parms.name("MBROPT");
    let now = now_unix();
    let (store, ctx) = db(system)?;
    let into = with_txn(store, |txn| {
        let id = file_id(txn, ctx, &target)?;
        let desc = db::file::describe(txn, id).map_err(|e| crate::svc::db::db_err(&e))?;
        let spec = match &mbr {
            Some(m) => {
                let name = Name::parse(m).map_err(|e| format!("CPD0084 member {m}: {e}"))?;
                if !desc.members.iter().any(|e| e.name == name) {
                    db::file::add_member(txn, id, name, "", None, now).map_err(|e| crate::svc::db::db_err(&e))?;
                }
                MemberSpec::Name(name)
            }
            None => MemberSpec::First,
        };
        let source = desc.attr.kind == FileKind::Source;
        let existing: Vec<Vec<u8>> = if source {
            db::source::records(txn, id, spec)
                .map_err(|e| crate::svc::db::db_err(&e))?
                .into_iter()
                .map(|(_, _, d)| d)
                .collect()
        } else {
            let m = db::Member::open_by(txn, id, spec).map_err(|e| crate::svc::db::db_err(&e))?;
            m.scan(txn, 1, usize::MAX)
                .map_err(|e| crate::svc::db::db_err(&e))?
                .into_iter()
                .filter(|s| !s.deleted)
                .map(|s| s.record)
                .collect()
        };
        let mut all = match mbropt.as_str() {
            "*ADD" => existing,
            "*REPLACE" => Vec::new(),
            _ if !existing.is_empty() => {
                return Err(format!("CPF2817 the member already holds {} records; give MBROPT(*ADD) or *REPLACE", existing.len()));
            }
            _ => Vec::new(),
        };
        all.extend(recs);
        if source {
            db::source::put_records(txn, crate::svc::db::JOB, id, spec, &all, now).map_err(|e| crate::svc::db::db_err(&e))?;
        } else {
            db::util::put_records(txn, crate::svc::db::JOB, id, spec, &all, 0x40, now).map_err(|e| crate::svc::db::db_err(&e))?;
        }
        let m = db::Member::open_by(txn, id, MemberSpec::First).map(|m| m.name().to_string()).unwrap_or_default();
        Ok(mbr.clone().unwrap_or(m))
    })?;
    let to_name = format!("{target}/{into}");
    Ok(done(
        format!("{COPIED_FROM} {n} records copied from tape file {shown} to {to_name}.\n"),
        COPIED_FROM,
        &[(&count, 10), (&shown, 17), (&into, 10)],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_dates() {
        assert_eq!(expiry("*PERM"), Some(Expiry::Perm));
        assert_eq!(expiry("*TAPF"), None);
        assert_eq!(expiry("1261231"), Date::from_ymd(2026, 12, 31).map(Expiry::Date));
    }

    #[test]
    fn ascii_round_trip() {
        let e = ccsid::to_ebcdic("ABC 123").unwrap();
        assert_eq!(to_ascii(&e), b"ABC 123");
        assert_eq!(from_ascii(b"ABC 123"), e);

    }
}
