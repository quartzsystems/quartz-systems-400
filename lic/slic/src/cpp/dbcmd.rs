// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database commands on the `db`, `sql` and `jrn` library functions:
//! `CHGPF`, `CHGLF`, `CHGSRCPF`, `CHGPFM`, `RNMM`, `CPYSRCF`, `RUNSQL`,
//! `RUNSQLSTM`, `SNDJRNE`, `WRKJRNA`, `WRKJRN`.

use std::fmt::Write as _;

use cmd::block::Parms;
use db::chg::{ChgLf, ChgPf};
use db::file::{Maint, Recover, Size, Wait};
use qsname::{MemberSpec, Name};
use sql::run::{CommitOpt, RunOpts, runsqlstm};
use stg::proto::CmdReply;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use super::direct::{failed, in_store, lib_or, plain};
use super::escape;

/// The `*JRN` type code.
fn jrn_type() -> u16 {
    types::type_by_name("*JRN").map_or(0x0018, |t| t.code)
}
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CHGPF" | "CHGSRCPF" => chgpf(system, name, parms),
        "CHGLF" => chglf(system, parms),
        "CHGPFM" | "CHGLFM" => chgpfm(system, parms),
        "RNMM" => rnmm(system, parms),
        "INZPFM" => inzpfm(system, parms),
        "CPYSRCF" => cpysrcf(system, parms),
        "RUNSQL" => return Some(runsql(system, parms)),
        "RUNSQLSTM" => return Some(runsqlstm_cmd(system, parms)),
        "SNDJRNE" => sndjrne(system, parms),
        "WRKJRNA" | "WRKJRN" => wrkjrna(system, parms),
        _ => return None,
    };
    Some(match r {
        Ok(text) => plain(text),
        Err(e) => failed(name, &e),
    })
}

/// The job's lock owner handle.
fn job_handle(system: &System) -> JobHandle {
    JobHandle(
        system
            .jobs
            .get(system.current_job)
            .map_or(u64::from(system.current_job), |j| j.handle),
    )
}

/// A file by `(name, library)`.
fn file(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    (name, lib): &(String, String),
) -> Result<ObjectId, String> {
    wm::obj::find(txn, qsys, names, &lib_or(lib), name, types::FILE)
        .map_err(|_| format!("CPF9812 file {name} in library {} not found", lib_or(lib)))
}

fn member(parms: &Parms, kw: &str) -> Result<MemberSpec, String> {
    Ok(match parms.name(kw).as_str() {
        "" | "*FIRST" => MemberSpec::First,
        "*LAST" => MemberSpec::Last,
        "*ALL" => MemberSpec::All,
        m => MemberSpec::Name(Name::parse(m).map_err(|e| format!("CPD0078 {kw}({m}): {e}"))?),
    })
}

/// `*YES`/`*NO` when the caller gave the keyword.
fn yes_no(parms: &Parms, kw: &str) -> Option<bool> {
    parms.specified(kw).then(|| parms.yes(kw))
}

fn wait(parms: &Parms, kw: &str) -> Result<Option<Wait>, String> {
    if !parms.specified(kw) {
        return Ok(None);
    }
    Ok(Some(match parms.name(kw).as_str() {
        "*IMMED" => Wait::Immed,
        "*CLS" | "*NOMAX" => Wait::NoMax,
        _ => Wait::Seconds(
            u16::try_from(parms.int(kw).unwrap_or(0))
                .map_err(|_| format!("CPD0084 {kw}: 1 to 32767"))?,
        ),
    }))
}

fn maint(parms: &Parms) -> Result<Option<Maint>, String> {
    Ok(match parms.name("MAINT").as_str() {
        "" => None,
        "*IMMED" => Some(Maint::Immed),
        "*DLY" => Some(Maint::Dly),
        "*REBLD" => Some(Maint::Rebld),
        v => return Err(format!("CPD0084 MAINT({v}) is not valid")),
    })
}

fn recover(parms: &Parms) -> Result<Option<Recover>, String> {
    Ok(match parms.name("RECOVER").as_str() {
        "" => None,
        "*NO" => Some(Recover::No),
        "*AFTIPL" => Some(Recover::AftIpl),
        "*IPL" => Some(Recover::Ipl),
        v => return Err(format!("CPD0084 RECOVER({v}) is not valid")),
    })
}

fn text_change(parms: &Parms) -> Option<String> {
    parms.specified("TEXT").then(|| {
        let t = parms.text("TEXT");
        if t == "*BLANK" { String::new() } else { t }
    })
}

fn chgpf(system: &mut System, name: &str, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("FILE");
    let mut out = String::new();
    if parms.specified("SRCFILE") && name == "CHGPF" {
        let (sn, sl) = parms.qual("SRCFILE");
        let mbr = {
            let m = parms.name("SRCMBR");
            if m.is_empty() || m == "*FILE" { q.0.clone() } else { m }
        };
        let line = format!(
            "chgpf {}/{} SRCFILE({}/{sn}) SRCMBR({mbr})",
            lib_or(&q.1),
            q.0,
            lib_or(&sl)
        );
        let r = super::direct::old(system, &line);
        if super::leading_msgid(r.trim()).is_some_and(|id| !id.starts_with("CPC")) {
            return Err(r.trim_end().to_string());
        }
        out = r;
    }
    let mut c = ChgPf {
        now: now_unix(),
        ..ChgPf::default()
    };
    if parms.specified("SIZE") {
        let e = parms.elems("SIZE");
        let word = |i: usize| e.get(i).copied().flatten().map(cmd::Value::source);
        c.size = Some(if word(0).as_deref() == Some("*NOMAX") {
            Size::NOMAX
        } else {
            let n = |i: usize, d: u32| -> Result<u32, String> {
                word(i).map_or(Ok(d), |w| {
                    w.trim().parse().map_err(|_| "CPD0084 SIZE: numbers are needed".to_string())
                })
            };
            Size {
                initial: n(0, 10_000)?,
                increment: n(1, 1000)?,
                max_increments: n(2, 3)?,
            }
        });
    }
    if parms.specified("MAXMBRS") {
        c.max_members = Some(if parms.name("MAXMBRS") == "*NOMAX" {
            0
        } else {
            u32::try_from(parms.int("MAXMBRS").unwrap_or(1)).unwrap_or(1)
        });
    }
    c.reusedlt = yes_no(parms, "REUSEDLT");
    c.share = yes_no(parms, "SHARE");
    c.lvlchk = yes_no(parms, "LVLCHK");
    c.frcaccpth = yes_no(parms, "FRCACCPTH");
    c.alwupd = yes_no(parms, "ALWUPD");
    c.alwdlt = yes_no(parms, "ALWDLT");
    c.maint = maint(parms)?;
    c.recover = recover(parms)?;
    if parms.specified("FRCRATIO") {
        c.frcratio = Some(if parms.name("FRCRATIO") == "*NONE" {
            0
        } else {
            u16::try_from(parms.int("FRCRATIO").unwrap_or(0)).unwrap_or(0)
        });
    }
    c.wait_file = wait(parms, "WAITFILE")?;
    c.wait_rcd = wait(parms, "WAITRCD")?;
    if parms.specified("CCSID") && parms.name("CCSID") != "*SAME" {
        c.ccsid = parms.int("CCSID").and_then(|n| u16::try_from(n).ok());
    }
    c.text = text_change(parms);
    let job = job_handle(system);
    in_store(system, |txn, qsys, names| {
        let id = file(txn, qsys, names, &q)?;
        db::chg::chgpf(txn, job, id, &c).map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF3204")))?;
        Ok(format!("{out}File {} changed.\n", q.0))
    })
}

/// `INZPFM`: through the console word.
fn inzpfm(system: &mut System, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("FILE");
    let mbr = parms.name("MBR");
    let records = parms.name("RECORDS");
    let n = parms.int("TOTRCDS").unwrap_or(1);
    let line = format!(
        "inzpfm {}/{} MBR({}) RECORDS({}) TOTRCDS({n})",
        lib_or(&q.1),
        q.0,
        if mbr.is_empty() { "*FIRST" } else { &mbr },
        if records.is_empty() { "*DFT" } else { &records }
    );
    let r = super::direct::old(system, &line);
    if super::leading_msgid(r.trim()).is_some_and(|id| !id.starts_with("CPC")) {
        return Err(r.trim_end().to_string());
    }
    Ok(r)
}

fn chglf(system: &mut System, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("FILE");
    let c = ChgLf {
        maint: maint(parms)?,
        recover: recover(parms)?,
        share: yes_no(parms, "SHARE"),
        lvlchk: yes_no(parms, "LVLCHK"),
        wait_file: wait(parms, "WAITFILE")?,
        wait_rcd: wait(parms, "WAITRCD")?,
        frcaccpth: yes_no(parms, "FRCACCPTH"),
        text: text_change(parms),
        now: now_unix(),
    };
    let job = job_handle(system);
    in_store(system, |txn, qsys, names| {
        let id = file(txn, qsys, names, &q)?;
        db::chg::chglf(txn, job, id, &c).map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF3204")))?;
        Ok(format!("File {} changed.\n", q.0))
    })
}

fn chgpfm(system: &mut System, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("FILE");
    let mbr = parms.name("MBR");
    let m = Name::parse(&mbr).map_err(|e| format!("CPD0078 MBR({mbr}): {e}"))?;
    let text = text_change(parms);
    let srctype = parms.specified("SRCTYPE").then(|| match parms.name("SRCTYPE").as_str() {
        "*NONE" | "" => Ok(None),
        t => Name::parse(t).map(Some).map_err(|e| format!("CPD0078 SRCTYPE({t}): {e}")),
    });
    let srctype = srctype.transpose()?;
    let now = now_unix();
    in_store(system, |txn, qsys, names| {
        let id = file(txn, qsys, names, &q)?;
        db::file::chgpfm(txn, id, m, text.as_deref(), srctype, now)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF3204")))?;
        Ok(format!("Member {mbr} of file {} changed.\n", q.0))
    })
}

fn rnmm(system: &mut System, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("FILE");
    let from = parms.name("MBR");
    let to = parms.name("NEWMBR");
    let f = Name::parse(&from).map_err(|e| format!("CPD0078 MBR({from}): {e}"))?;
    let t = Name::parse(&to).map_err(|e| format!("CPD0078 NEWMBR({to}): {e}"))?;
    let now = now_unix();
    in_store(system, |txn, qsys, names| {
        let id = file(txn, qsys, names, &q)?;
        db::file::rename_member(txn, id, f, t, now)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF3204")))?;
        Ok(format!("Member {from} of file {} renamed to {to}.\n", q.0))
    })
}

fn cpysrcf(system: &mut System, parms: &Parms) -> Result<String, String> {
    let from = parms.qual("FROMFILE");
    let to = parms.qual("TOFILE");
    let from_mbr = member(parms, "FROMMBR")?;
    let to_mbr = match parms.name("TOMBR").as_str() {
        "" | "*FROMMBR" => None,
        m => Some(Name::parse(m).map_err(|e| format!("CPD0078 TOMBR({m}): {e}"))?),
    };
    let job = job_handle(system);
    let now = now_unix();
    in_store(system, |txn, qsys, names| {
        let f = file(txn, qsys, names, &from)?;
        let t = file(txn, qsys, names, &to)?;
        let fdesc = db::file::describe(txn, f).map_err(|e| e.to_string())?;
        let entry = db::util::member_of(&fdesc, from_mbr).map_err(|e| e.to_string())?;
        let text = db::source::srcget(txn, f, MemberSpec::Name(entry.name))
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF3204")))?;
        let target = to_mbr.unwrap_or(entry.name);
        let tdesc = db::file::describe(txn, t).map_err(|e| e.to_string())?;
        if !tdesc.members.iter().any(|m| m.name == target) {
            db::file::add_member(txn, t, target, "", entry.srctype, now)
                .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF3204")))?;
        }
        let n = db::source::srcput(txn, job, t, MemberSpec::Name(target), &text, entry.srctype, now)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF3204")))?;
        Ok(format!(
            "{n} record(s) copied from member {} of {} to member {target} of {}.\n",
            entry.name, from.0, to.0
        ))
    })
}

fn run_opts(parms: &Parms, system: &System) -> Result<RunOpts, String> {
    Ok(RunOpts {
        commit: match parms.name("COMMIT").as_str() {
            "" | "*NONE" | "*NC" => CommitOpt::None,
            "*CHG" | "*UR" => CommitOpt::Chg,
            "*CS" => CommitOpt::Cs,
            "*ALL" | "*RS" | "*RR" => CommitOpt::All,
            x => return Err(format!("CPD0084 COMMIT({x}) is not valid")),
        },
        naming: match parms.name("NAMING").as_str() {
            "" => crate::svc::db::naming(system),
            "*SYS" => sql::Naming::Sys,
            "*SQL" => sql::Naming::Sql,
            x => return Err(format!("CPD0084 NAMING({x}) is not valid")),
        },
        errlvl: u8::try_from(parms.int("ERRLVL").unwrap_or(10)).unwrap_or(10),
        dftrdbcol: parms
            .specified("DFTRDBCOL")
            .then(|| parms.name("DFTRDBCOL"))
            .filter(|c| c != "*NONE"),
        show_rows: 20,
    })
}

/// `RUNSQL SQL('statement')`: one statement, `SQL9010` when it fails.
fn runsql(system: &mut System, parms: &Parms) -> CmdReply {
    let text = parms.text("SQL");
    let opts = match run_opts(parms, system) {
        Ok(o) => o,
        Err(e) => return failed("RUNSQL", &e),
    };
    let report = match crate::svc::db::db(system)
        .and_then(|(store, ctx)| runsqlstm(store, ctx, &text, &opts).map_err(|e| e.to_string()))
    {
        Ok(r) => r,
        Err(e) => return failed("RUNSQL", &e),
    };
    if report.errors > 0 {
        return escape("SQL9010", "RUNSQL", &format!("the statement failed\n{}", report.listing));
    }
    plain(report.listing)
}

/// `RUNSQLSTM SRCFILE(lib/file) SRCMBR(m)`: the member's statements;
/// `SQL9006` when errors passed `ERRLVL`.
fn runsqlstm_cmd(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("SRCFILE");
    let mbr = match member(parms, "SRCMBR") {
        Ok(m) => m,
        Err(e) => return failed("RUNSQLSTM", &e),
    };
    let text = in_store(system, |txn, qsys, names| {
        let id = file(txn, qsys, names, &q)?;
        db::source::srcget(txn, id, mbr).map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF3204")))
    });
    let text = match text {
        Ok(t) => t,
        Err(e) => return failed("RUNSQLSTM", &e),
    };
    let opts = match run_opts(parms, system) {
        Ok(o) => o,
        Err(e) => return failed("RUNSQLSTM", &e),
    };
    let report = match crate::svc::db::db(system)
        .and_then(|(store, ctx)| runsqlstm(store, ctx, &text, &opts).map_err(|e| e.to_string()))
    {
        Ok(r) => r,
        Err(e) => return failed("RUNSQLSTM", &e),
    };
    let summary = format!(
        "{}{} statement(s), {} error(s), severity {}\n",
        report.listing, report.statements, report.errors, report.severity
    );
    if report.severity > opts.errlvl {
        return escape("SQL9006", "RUNSQLSTM", &summary);
    }
    plain(summary)
}

fn sndjrne(system: &mut System, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("JRN");
    let kind = {
        let t = parms.name("TYPE");
        if t.is_empty() { "00".to_string() } else { t }
    };
    let data = ccsid::to_ebcdic(&parms.text("ENTDTA")).unwrap_or_default();
    let job = job_handle(system);
    in_store(system, |txn, qsys, names| {
        let id = wm::obj::find(txn, qsys, names, &lib_or(&q.1), &q.0, jrn_type())
            .map_err(|_| format!("CPF9801 journal {}/{} not found", lib_or(&q.1), q.0))?;
        let mut j = jrn::Journal::load(txn, id).map_err(|e| e.to_string())?;
        let seq = j
            .sndjrne(txn, job, &kind, &data)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF7010")))?;
        Ok(format!("Entry {seq} sent to journal {}.\n", q.0))
    })
}

/// `WRKJRNA`: the receiver directory and the journaled objects.
fn wrkjrna(system: &mut System, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("JRN");
    in_store(system, |txn, qsys, names| {
        let id = wm::obj::find(txn, qsys, names, &lib_or(&q.1), &q.0, jrn_type())
            .map_err(|_| format!("CPF9801 journal {}/{} not found", lib_or(&q.1), q.0))?;
        let j = jrn::Journal::load(txn, id).map_err(|e| e.to_string())?;
        let mut out = format!("Journal {}:\n  Receiver directory:\n", q.0);
        let attached = j.attached(txn).ok().map(|r| r.id);
        for d in j.receivers(txn).map_err(|e| e.to_string())? {
            let (_, h) = ops::read_header(txn, d.receiver).map_err(|e| e.to_string())?;
            let _ = writeln!(
                out,
                "    {:<10} {:<10} {}{}",
                h.name,
                d.status.name(),
                sec::cmd::civil(d.attached),
                if attached == Some(d.receiver) { "  (attached)" } else { "" }
            );
        }
        out.push_str("  Journaled objects:\n");
        for o in j.objects(txn).map_err(|e| e.to_string())? {
            let (_, h) = ops::read_header(txn, o.object).map_err(|e| e.to_string())?;
            let ty = store::obj::types::type_by_code(o.object_type)
                .map_or_else(|| format!("*{:04X}", o.object_type), |t| t.name.to_string());
            let _ = writeln!(
                out,
                "    {:<10} {:<8} IMAGES({}) OMTJRNE({}) since {}",
                h.name,
                ty,
                if o.both_images { "*BOTH" } else { "*AFTER" },
                if o.omit_open_close { "*OPNCLO" } else { "*NONE" },
                sec::cmd::civil(o.started)
            );
        }
        Ok(out)
    })
}
