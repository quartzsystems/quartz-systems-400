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

use ile::{ActGrpAttr, BindInput, BindOptions, CreateData, ModuleObject, PgmKind};
use mi::io::spool::{MemSpool, PrinterOpen, SpoolSink};
use mi::io::IoFail;
use mi::{Job, MemSource, MemUserSpaces};
use rpg_oracle::Case;
use rpg_oracle::listing::Diag as ODiag;
use rpg_oracle::outcome::{Outcome, Run, Spooled};
use rpgfront::{MemIncludes, Member};
use sysapi::{LocalNow, MemProvider};

use crate::qir::CLOCK;

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
            if !dir.is_dir() || matches!(batch.as_str(), "expected" | "oracle") {
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

/// The kit's spooled files.
#[derive(Clone, Default)]
struct KitSpool(Rc<RefCell<MemSpool>>);

impl SpoolSink for KitSpool {
    fn open(&mut self, file: &str, lib: &str, actgrp: &str) -> Result<PrinterOpen, IoFail> {
        self.0.borrow_mut().open(file, lib, actgrp)
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
    let command = if cmd.starts_with("CRTRPGMOD") {
        rpgc::Command::CrtRpgMod
    } else if cmd.starts_with("CRTSQLRPGI") {
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
    o
}

/// The member as the oracle's upload leaves it: numbered from 1.00,
/// zero dates.
#[must_use]
pub fn member(case: &Case) -> Member {
    Member::from_text("QGPL", "QRPGLESRC", &case.name, &case.lines.join("\n"))
}

/// Compile a case.
#[must_use]
pub fn compile(case: &Case) -> rpgc::Compiled {
    rpgc::compile(member(case), &mut MemIncludes::default(), &mut rpgc::NoExtFiles, &options(case))
}

/// The diagnostics as the oracle's listing reports them.
fn diags(c: &rpgc::Compiled) -> Vec<ODiag> {
    let mut out: Vec<ODiag> = c
        .diags
        .iter()
        .map(|d| ODiag {
            line: d.at.line,
            msgid: d.msgid.to_string(),
            severity: d.severity,
            seq: c
                .read
                .lines
                .iter()
                .find(|l| l.no == d.at.line && d.at.line > 0)
                .map(|l| l.seq.clone())
                .unwrap_or_default(),
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The program a compile made.
fn program(c: &rpgc::Compiled, name: &str) -> Result<ile::ProgramObject, String> {
    let m = qir::parse(&c.qir).map_err(|e| format!("{name}: {e}"))?;
    let actgrp = match &m.actgrp {
        qir::ActGrp::New => ActGrpAttr::New,
        qir::ActGrp::Caller => ActGrpAttr::Caller,
        qir::ActGrp::Named(n) => ActGrpAttr::Named(n.clone()),
    };
    let dft = m.dftactgrp;
    let mo = ModuleObject::compile(m, CreateData::default(), CLOCK).map_err(|e| format!("{name}: {e}"))?;
    let mut opts = BindOptions::new(&mo.name.clone(), PgmKind::Pgm);
    opts.attr.actgrp = if dft { ActGrpAttr::Default } else { actgrp };
    opts.allow_unresolved = true;
    ile::bind(BindInput {
        modules: vec![(mo, "QGPL".into())],
        bnddirs: Vec::new(),
        lookup: &|_| None,
        options: opts,
    })
    .map(|(p, _)| p)
    .map_err(|e| format!("{name}: {e}"))
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
    let c = compile(case);
    let notes = notes(&c);
    let mut o = Outcome {
        created: c.created,
        cause: if c.created { String::new() } else { "RNS9310".into() },
        diags: diags(&c),
        ..Outcome::default()
    };
    if c.created && !case.call.is_empty() {
        let p = program(&c, &case.name)?;
        let mut src = MemSource::default();
        src.add("QGPL", p);
        let mut job = Job::boxed(Box::new(src));
        job.usrspcs.set_source(Box::new(MemUserSpaces::default()));
        let mut provider = MemProvider::default();
        provider.clock = Some(LocalNow {
            unix: CLOCK,
            offset_min: 0,
            zone: "QN0000UTC".into(),
            dst: false,
        });
        sysapi::install(&mut job, Rc::new(RefCell::new(provider)));
        qrpgrt::install(&mut job);
        let spool = KitSpool::default();
        job.spool = Some(Box::new(spool.clone()));
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
    }
    Ok((o, c.listing, notes))
}
