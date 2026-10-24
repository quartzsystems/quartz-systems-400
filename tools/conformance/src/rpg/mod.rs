// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The RPG differential corpus in memory: `conformance/rpg/<batch>/`
//! members compiled with `rpgc` and called in an in-memory job, each
//! reduced to the same normalized outcome `rpg-oracle` writes from the
//! oracle (`rpg_oracle::Outcome`), and compared with
//! `conformance/rpg/expected/NAME.out`. Members without an expected
//! outcome are reported, not failed, until the oracle has run them.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ile::{ActGrpAttr, CreateData, ModuleObject, PgmKind};
use mi::io::spool::{MemSpool, PrinterOpen, SpoolSink};
use mi::io::IoFail;
use mi::{Job, MemSource, MemUserSpaces};
use rpg_oracle::Case;
use rpg_oracle::listing::Diag as ODiag;
use rpg_oracle::outcome::{Outcome, Run, Spooled};
use rpgfront::{MemIncludes, Member};
use sysapi::{LocalNow, MemProvider};

use crate::qir::CLOCK;

pub mod build;
pub use rpg_oracle::fuzz;

/// How long one member may run in the kit before its request ends.
const LIMIT_MS: u64 = 10_000;

/// When the running member's time is up, as milliseconds since the
/// epoch; 0 when none runs.
static DEADLINE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// A thread that raises the attention flag once a deadline passes, so
/// a member that never ends (a cycle without `LR`) is stopped.
fn watchdog() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
                let d = DEADLINE.load(std::sync::atomic::Ordering::Relaxed);
                if d != 0 && now_ms() > d {
                    mi::rtlib::ATTN.store(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });
    });
}

pub mod files;
pub mod screen;

/// The message descriptions a case's setup adds (`ADDMSGD`), by
/// identifier, for its display files' `ERRMSGID`.
fn messages(case: &Case) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for s in &case.setup {
        let cmd = case.fill(s, "QGPL");
        if !cmd.trim_start().to_ascii_uppercase().starts_with("ADDMSGD") {
            continue;
        }
        let id = build::kwd(&cmd, "MSGID");
        // The text as written: kwd() upper-cases.
        let text = cmd
            .find("MSG('")
            .and_then(|at| cmd[at + 5..].find("')").map(|end| cmd[at + 5..at + 5 + end].replace("''", "'")))
            .unwrap_or_default();
        out.insert(id, text);
    }
    out
}
pub mod lstcmp;

/// Where the corpus lives.
pub struct Kit {
    /// `conformance/rpg`.
    pub root: PathBuf,
}

impl Kit {
    /// The corpus checked in beside this crate.
    #[must_use]
    pub fn locate() -> Self {
        Self {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("rpg"),
        }
    }

    /// Every member of every batch, in name order.
    pub fn cases(&self) -> std::io::Result<Vec<(String, Case)>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let dir = entry?.path();
            let batch = dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if !dir.is_dir() || matches!(batch.as_str(), "expected" | "oracle" | "perf") {
                continue;
            }
            for f in fs::read_dir(&dir)? {
                let p = f?.path();
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("rpgle")) {
                    out.push((batch.clone(), Case::load(&p)?));
                }
            }
        }
        out.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        Ok(out)
    }

    /// `expected/`.
    #[must_use]
    pub fn expected_dir(&self) -> PathBuf {
        self.root.join("expected")
    }

    /// Where a run writes its outputs.
    #[must_use]
    pub fn ours_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("conformance-rpg")
            .join("ours")
    }
}

/// The kit's spooled files, with the member's printer files' formats.
#[derive(Clone, Default)]
struct KitSpool(Rc<RefCell<MemSpool>>, Vec<(String, Rc<dds::prtf::PrtfDefinition>)>);

impl SpoolSink for KitSpool {
    fn open(&mut self, file: &str, lib: &str, actgrp: &str) -> Result<PrinterOpen, IoFail> {
        let mut o = self.0.borrow_mut().open(file, lib, actgrp)?;
        if let Some((_, def)) = self.1.iter().find(|(n, _)| n.eq_ignore_ascii_case(file)) {
            o.prtf.formats = dds::prtf::codec::encode(def);
        }
        Ok(o)
    }
    fn put(&mut self, id: u32, data: &[u8], pages: u32, records: u32) -> Result<(), IoFail> {
        self.0.borrow_mut().put(id, data, pages, records)
    }
    fn close(&mut self, id: u32, hold: bool) -> Result<(), IoFail> {
        self.0.borrow_mut().close(id, hold)
    }
}

/// The compile options a case's command asks for.
#[must_use]
pub fn options(case: &Case) -> rpgc::Options {
    let cmd = case.fill(&case.cmd, "QGPL").to_ascii_uppercase();
    let word = |kw: &str| -> Option<String> {
        let at = cmd.find(&format!("{kw}("))?;
        let rest = &cmd[at + kw.len() + 1..];
        let end = rest.find(')')?;
        Some(rest[..end].trim().to_string())
    };
    let sql = cmd.starts_with("CRTSQLRPGI");
    let command = if cmd.starts_with("CRTRPGMOD") || sql && matches!(word("OBJTYPE").as_deref(), Some("*MODULE" | "*SRVPGM")) {
        rpgc::Command::CrtRpgMod
    } else if sql {
        rpgc::Command::CrtSqlRpgi
    } else {
        rpgc::Command::CrtBndRpg
    };
    let mut o = rpgc::Options {
        command,
        name: case.name.clone(),
        lib: "QGPL".into(),
        src: ("QGPL".into(), "QRPGLESRC".into(), case.name.clone()),
        ..rpgc::Options::default()
    };
    if let Some(v) = word("DBGVIEW") {
        o.dbgview = v;
    }
    if let Some(v) = word("DFTACTGRP") {
        o.dftactgrp = v == "*YES";
    }
    if let Some(v) = word("ACTGRP") {
        o.actgrp = v;
    }
    if let Some(v) = word("OPTION") {
        o.option = v.split_whitespace().map(str::to_string).collect();
    }
    if let Some(v) = word("DEFINE") {
        o.define = v.split_whitespace().map(|s| s.trim_matches('\'').to_string()).collect();
    }
    if let Some(v) = word("TRUNCNBR") {
        o.truncnbr = v != "*NO";
    }
    if sql {
        let mut s = rpgc::sqlpre::Options::default();
        for (kw, slot) in [
            ("COMMIT", &mut s.commit),
            ("NAMING", &mut s.naming),
            ("DATFMT", &mut s.datfmt),
            ("DATSEP", &mut s.datsep),
            ("TIMFMT", &mut s.timfmt),
            ("TIMSEP", &mut s.timsep),
            ("CLOSQLCSR", &mut s.closqlcsr),
        ] {
            if let Some(v) = word(kw) {
                *slot = v;
            }
        }
        o.sql = Some(s);
    }
    if let Some(v) = word("GENLVL") {
        o.genlvl = v.parse().unwrap_or(o.genlvl);
    }
    for (kw, slot) in [("OPTIMIZE", &mut o.optimize), ("ALWNULL", &mut o.alwnull), ("OUTPUT", &mut o.output)] {
        if let Some(v) = word(kw) {
            *slot = v;
        }
    }
    // Quoted values keep their case.
    let quoted = |kw: &str| -> Option<String> {
        let full = case.fill(&case.cmd, "QGPL");
        let at = full.to_ascii_uppercase().find(&format!("{kw}('"))?;
        let rest = &full[at + kw.len() + 2..];
        let end = rest.find("')")?;
        Some(rest[..end].to_string())
    };
    if let Some(v) = quoted("TEXT") {
        o.text = v;
    }
    if let Some(v) = quoted("INDENT") {
        o.indent = format!("'{v}'");
    } else if let Some(v) = word("INDENT") {
        o.indent = v;
    }
    o
}

/// Whether a case is precompiled.
fn is_sql(case: &Case) -> bool {
    case.cmd.trim_start().to_ascii_uppercase().starts_with("CRTSQLRPGI")
}

/// The member as the oracle's upload leaves it: numbered from 1.00,
/// zero dates.
#[must_use]
pub fn member(case: &Case) -> Member {
    Member::from_text("QGPL", "QRPGLESRC", &case.name, &case.lines.join("\n"))
}

/// Compile a case, with the external descriptions of its files.
#[must_use]
pub fn compile(case: &Case, db: Option<&files::KitDb>) -> rpgc::Compiled {
    match db {
        Some(d) => rpgc::compile(member(case), &mut includes(case), &mut d.ext(), &options(case)),
        None => rpgc::compile(member(case), &mut includes(case), &mut rpgc::NoExtFiles, &options(case)),
    }
}

/// The members a case's `with:` brings, for `/COPY` and the setup.
#[must_use]
pub fn includes(case: &Case) -> MemIncludes {
    let mut inc = MemIncludes::default();
    for (n, lines) in &case.with {
        inc.members.push(Member::from_text("QGPL", "QRPGLESRC", n, &lines.join("\n")));
    }
    inc
}

/// Compile one of a case's members as a module with a setup command's
/// options: the IR, or the reason it was not created.
fn compile_member(case: &Case, db: Option<&files::KitDb>, mbr: &str, module: &str, cmd: &str) -> Result<String, String> {
    let lines = if mbr == case.name {
        case.lines.clone()
    } else {
        case.with
            .iter()
            .find(|(n, _)| n == mbr)
            .map(|(_, l)| l.clone())
            .ok_or_else(|| format!("{mbr}: not among the case's members"))?
    };
    let sub = Case {
        name: module.to_string(),
        lines,
        cmd: cmd.to_string(),
        ..case.clone()
    };
    let mut o = options(&sub);
    // CRTSQLRPGI's own choice stands.
    if !is_sql(&sub) {
        o.command = if cmd.trim_start().to_ascii_uppercase().starts_with("CRTBNDRPG") {
            rpgc::Command::CrtBndRpg
        } else {
            rpgc::Command::CrtRpgMod
        };
    }
    let member = Member::from_text("QGPL", "QRPGLESRC", mbr, &sub.lines.join("\n"));
    let c = match db {
        Some(d) => rpgc::compile(member, &mut includes(case), &mut d.ext(), &o),
        None => rpgc::compile(member, &mut includes(case), &mut rpgc::NoExtFiles, &o),
    };
    if c.created {
        Ok(c.qir)
    } else {
        let ids: Vec<String> = c
            .diags
            .iter()
            .filter(|d| d.severity > 10)
            .map(|d| if d.args.is_empty() { d.msgid.to_string() } else { format!("{} {}", d.msgid, d.args.join(" ")) })
            .collect();
        Err(format!("{module}: not created ({})", ids.join("; ")))
    }
}

/// The diagnostics as the oracle's listing reports them.
#[must_use]
pub fn diags(c: &rpgc::Compiled) -> Vec<ODiag> {
    let mut out: Vec<ODiag> = c
        .diags
        .iter()
        .map(|d| if d.msgid.starts_with("SQL") {
            // The precompile listing numbers source records.
            ODiag {
                line: d.at.line,
                msgid: d.msgid.to_string(),
                severity: d.severity,
                seq: String::new(),
            }
        } else { ODiag {
            line: c.listing_line(d.at.line),
            msgid: d.msgid.to_string(),
            severity: d.severity,
            seq: c
                .read
                .lines
                .iter()
                .find(|l| l.no == d.at.line && d.at.line > 0)
                .map(|l| l.seq.clone())
                .unwrap_or_default(),
        } })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The program a compile made.
fn program(c: &rpgc::Compiled, name: &str, objs: &build::Objects, cmd: &str) -> Result<ile::ProgramObject, String> {
    let m = qir::parse(&c.qir).map_err(|e| format!("{name}: {e}"))?;
    let actgrp = match &m.actgrp {
        qir::ActGrp::New => ActGrpAttr::New,
        qir::ActGrp::Caller => ActGrpAttr::Caller,
        qir::ActGrp::Named(n) => ActGrpAttr::Named(n.clone()),
    };
    let dft = m.dftactgrp;
    let mo = ModuleObject::compile(m, CreateData::default(), CLOCK).map_err(|e| format!("{name}: {e}"))?;
    // The directories the control specification and the command name.
    let mut dirs: Vec<String> = c.ctl.bnddir.iter().map(|d| d.rsplit('/').next().unwrap_or(d).to_string()).collect();
    dirs.extend(build::names_of(&build::kwd(cmd, "BNDDIR")));
    objs.bind(&mo.name.clone(), PgmKind::Pgm, vec![mo], &dirs, &[], if dft { ActGrpAttr::Default } else { actgrp })
}

/// The spooled files as page/line rows of printed lines.
fn spooled(spool: &MemSpool) -> Vec<Spooled> {
    // A spooled file nothing was printed in is not kept (pub400).
    spool
        .files
        .values()
        .filter(|f| f.records > 0)
        .map(|f| {
            let (pages, _) = spool::scs::decode(&f.data);
            Spooled {
                file: f.file.clone(),
                lines: spool::view::rows(&pages)
                    .into_iter()
                    .filter(|(_, _, t)| !t.trim().is_empty())
                    .map(|(p, l, t)| format!("{p}/{l:<3} {}", t.trim_end()))
                    .collect(),
            }
        })
        .collect()
}

/// What a run leaves for a look: the IR and the diagnostics with
/// their values.
pub fn notes(c: &rpgc::Compiled) -> String {
    let mut s = String::new();
    for d in &c.diags {
        let _ = writeln!(s, "; {} {:02} line {}: {}", d.msgid, d.severity, d.at.line, rpgc::msgs::text(d));
    }
    s.push_str(&c.ir);
    s
}

/// Compile and run a case: its outcome, listing and notes.
#[allow(clippy::many_single_char_names)]
pub fn run(case: &Case) -> Result<(Outcome, Vec<String>, String), String> {
    let db = if case.db.is_empty() && !is_sql(case) {
        None
    } else {
        Some(files::KitDb::build(&case.db).map_err(|e| format!("{}: {e}", case.name))?)
    };
    // The setup's modules, service programs and directories.
    let mut objs = build::Objects::default();
    for s in &case.setup {
        let cmd = case.fill(s, "QGPL");
        objs.setup(&cmd, &|mbr, module, cmd| compile_member(case, db.as_ref(), mbr, module, cmd))?;
    }
    let main_cmd = case.fill(&case.cmd, "QGPL");
    let verb = main_cmd.trim().to_ascii_uppercase().split_whitespace().next().unwrap_or("").to_string();
    // CRTPGM binds what the setup made; nothing is compiled.
    if verb == "CRTPGM" {
        let mods: Vec<ModuleObject> = build::names_of(&build::kwd(&main_cmd, "MODULE"))
            .iter()
            .filter_map(|m| objs.modules.get(m).cloned())
            .collect();
        let p = objs.bind(
            &case.name,
            PgmKind::Pgm,
            mods,
            &build::names_of(&build::kwd(&main_cmd, "BNDDIR")),
            &build::names_of(&build::kwd(&main_cmd, "BNDSRVPGM")),
            ActGrpAttr::Named("QILE".into()),
        );
        let mut o = Outcome::default();
        return match p {
            Ok(p) => {
                o.created = true;
                call_into(case, &mut o, p, &objs, db.as_ref())?;
                Ok((o, Vec::new(), String::new()))
            }
            Err(e) => {
                o.cause = "CPF5D04".into();
                Ok((o, Vec::new(), e))
            }
        };
    }
    let c = compile(case, db.as_ref());
    let notes = notes(&c);
    let mut o = Outcome {
        created: c.created,
        // A module not created is RNS9309, a program RNS9310.
        cause: if c.created {
            String::new()
        } else if c.sql_failed {
            "SQL9001".into()
        } else if verb == "CRTRPGMOD" {
            "RNS9309".into()
        } else {
            "RNS9310".into()
        },
        diags: diags(&c),
        ..Outcome::default()
    };
    // A program that names binding directories is bound even when it
    // is not run: one that is not found means no program.
    if c.created && case.call.is_empty() && verb == "CRTBNDRPG" && !c.ctl.bnddir.is_empty()
        && program(&c, &case.name, &objs, &main_cmd).is_err()
    {
        o.created = false;
        o.cause = "RNS9310".into();
    }
    if c.created && !case.call.is_empty() && verb != "CRTRPGMOD" {
        let p = program(&c, &case.name, &objs, &main_cmd)?;
        call_into(case, &mut o, p, &objs, db.as_ref())?;
    }
    Ok((o, c.listing, notes))
}

/// Call a program with the case's service programs at hand, and note
/// how it ended and what it printed.
fn call_into(
    case: &Case,
    o: &mut Outcome,
    p: ile::ProgramObject,
    objs: &build::Objects,
    db: Option<&files::KitDb>,
) -> Result<(), String> {
    {
        let mut src = MemSource::default();
        src.add("QGPL", p);
        for s in objs.srvpgms.values().chain(objs.programs.values()) {
            src.add("QGPL", s.clone());
        }
        let mut job = Job::boxed(Box::new(src));
        job.usrspcs.set_source(Box::new(MemUserSpaces::default()));
        if let Some(d) = db {
            job.io = Some(Box::new(d.io()?));
            job.sql = Some(Box::new(d.sql()));
        }
        let mut provider = MemProvider::default();
        provider.clock = Some(LocalNow {
            unix: CLOCK,
            offset_min: 0,
            zone: "QN0000UTC".into(),
            dst: false,
        });
        sysapi::install(&mut job, Rc::new(RefCell::new(provider)));
        qrpgrt::install(&mut job);
        let spool = KitSpool(Rc::default(), db.map(|d| d.printers.clone()).unwrap_or_default());
        job.spool = Some(Box::new(spool.clone()));
        let display = db
            .filter(|d| !d.displays.is_empty())
            .map(|d| screen::KitDisplay::new(d.displays.clone(), &case.keys, messages(case)));
        if let Some(d) = &display {
            job.display = Some(Box::new(d.clone()));
        }
        // Past the deadline the attention hook ends the request.
        watchdog();
        job.attn = Some(Rc::new(|j: &mut Job| {
            let d = DEADLINE.load(std::sync::atomic::Ordering::Relaxed);
            if d != 0 && now_ms() > d {
                j.cancel_request("CPF9999", "QCPFMSG", b"TIMEOUT".to_vec());
            }
        }));
        DEADLINE.store(now_ms() + LIMIT_MS, std::sync::atomic::Ordering::Relaxed);
        let r = job.call_program(&case.name, "QGPL", &[]);
        DEADLINE.store(0, std::sync::atomic::Ordering::Relaxed);
        job.close_printers();
        // An exception left pending ended the program with an escape.
        o.run = match (r, &job.exception) {
            (Err(c), _) => Run::Escape(c.msgid()),
            (Ok(()), Some(e)) => Run::Escape(e.msgid.clone()),
            (Ok(()), None) => Run::Ok,
        };
        sysapi::uninstall(&job);
        o.spool = spooled(&spool.0.borrow());
        if let Some(d) = display {
            o.screens.clone_from(&d.0.borrow().shown);
        }
    }
    Ok(())
}
