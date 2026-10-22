// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IR conformance kit: `conformance/qir/programs/*.qir`, each a
//! case the in-memory runtime executes, with its console lines, the
//! messages it raised and handled and what it left pending rendered as
//! text and compared with `expected/`. The QEMU harness runs the same
//! programs through the service console.
//!
//! A case file holds one or more modules separated by a line of four
//! `=`; the first module's program is the one called. Header comments
//! steer the run: `; replies: C G` answers the inquiries in order,
//! `; dsply: yes no` the `dsply` replies, `; parms: 'abc' 12 x'C1'`
//! the parameters, as the console's `call` types them.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

pub mod qemu;

use blockdev::{BlockDev, MemBlockDev};
use db::file::CreateFileOpts;
use dds::{CompileOpts, NoRefs, Source, compile_pf};
use ile::{ActGrpAttr, BindInput, BindOptions, CreateData, ModuleObject, PgmKind};
use mi::io::spool::{MemSpool, PrinterOpen, SpoolSink};
use mi::io::store::StoreIo;
use mi::io::{DataAreaValue, IoFail, MemData};
use mi::{Job, MemSource, MemUserSpaces};
use qsname::{MemberSpec, Name};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};
use sysapi::{LocalNow, MemProvider};

/// The fixed clock every run sees: 2026-10-21 12:00:00 UTC.
pub const CLOCK: u64 = 1_792_670_400;

/// Where the kit lives.
pub struct Kit {
    /// `conformance/qir`.
    pub root: PathBuf,
}

impl Kit {
    /// The kit checked in beside this crate.
    #[must_use]
    pub fn locate() -> Self {
        Self {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("qir"),
        }
    }

    /// Every case, by name, in name order.
    pub fn cases(&self) -> std::io::Result<Vec<Case>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(self.root.join("programs"))? {
            let p = entry?.path();
            if p.extension().is_some_and(|e| e == "qir") {
                let name = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let text = fs::read_to_string(&p)?.replace("\r\n", "\n");
                out.push(Case::parse(&name, &text));
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
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
            .join("conformance-qir")
            .join("ours")
    }
}

/// One case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Case {
    /// File stem, upper case.
    pub name: String,
    /// Module texts, the first holding the program called.
    pub modules: Vec<String>,
    /// Inquiry replies, in order.
    pub replies: Vec<String>,
    /// `dsply` replies, in order.
    pub dsply: Vec<String>,
    /// Parameters as the console's `call` types them.
    pub parms: String,
    /// Whether the QEMU harness runs it over the console (`; qemu: no`
    /// marks cases that need the in-memory world).
    pub qemu: bool,
}

impl Case {
    /// Split a case file.
    #[must_use]
    pub fn parse(name: &str, text: &str) -> Self {
        let mut c = Self {
            name: name.to_ascii_uppercase(),
            qemu: true,
            ..Self::default()
        };
        for l in text.lines() {
            if let Some(r) = l.strip_prefix("; replies:") {
                c.replies = r.split_whitespace().map(str::to_string).collect();
            } else if let Some(r) = l.strip_prefix("; dsply:") {
                c.dsply = r.split_whitespace().map(str::to_string).collect();
            } else if let Some(r) = l.strip_prefix("; parms:") {
                c.parms = r.trim().to_string();
            } else if l.starts_with("; qemu: no") {
                c.qemu = false;
            }
        }
        c.modules = text
            .split("\n====\n")
            .map(|m| {
                m.lines()
                    .filter(|l| !l.starts_with("; "))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|m| !m.trim().is_empty())
            .collect();
        c
    }
}

fn name(s: &str) -> Name {
    Name::parse(s).expect("a valid name")
}

/// A customer record: `CUSNO packed(7,0)`, `NAME char(30)`, `BAL
/// packed(9,2)`.
#[must_use]
pub fn customer(cusno: i64, who: &str, bal: &str) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend(
        qdecimal::Packed::new(7, 0)
            .expect("valid")
            .encode(
                &qdecimal::Decimal::new(i128::from(cusno), 0).expect("fits"),
                qdecimal::Rounding::Truncate,
            )
            .expect("fits"),
    );
    let mut nm = vec![0x40u8; 30];
    ccsid::encode_fixed(who, &mut nm).expect("CP037");
    r.extend(nm);
    r.extend(
        qdecimal::Packed::new(9, 2)
            .expect("valid")
            .encode(
                &qdecimal::Decimal::parse(bal).expect("a number"),
                qdecimal::Rounding::Truncate,
            )
            .expect("fits"),
    );
    r
}

fn at(parts: &[(usize, &str)]) -> String {
    let mut s = vec![' '; 80];
    for (col, text) in parts {
        for (i, ch) in text.chars().enumerate() {
            s[col - 1 + i] = ch;
        }
    }
    s.into_iter().collect::<String>().trim_end().to_string()
}

/// A store with `QGPL/CUST` (five customers, keyed on `CUSNO` then
/// `NAME`) for the I/O cases.
pub fn customers() -> Result<(Rc<RefCell<Store>>, ObjectId), String> {
    let d = Arc::new(MemBlockDev::with_seed(16384, 11));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: CLOCK,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .map_err(|e| e.to_string())?;
    let mut t = store.begin().map_err(|e| e.to_string())?;
    let qsys = ops::bootstrap(&mut t, CLOCK, "QS400").map_err(|e| e.to_string())?;
    let lib = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: name("QGPL"),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: CLOCK,
            by_system: false,
        },
    )
    .map_err(|e| e.to_string())?;
    let lines = [
        at(&[(6, "A"), (17, "R"), (19, "CUSREC")]),
        at(&[(6, "A"), (19, "CUSNO"), (34, "7"), (35, "P"), (37, "0")]),
        at(&[(6, "A"), (19, "NAME"), (33, "30"), (35, "A")]),
        at(&[(6, "A"), (19, "BAL"), (34, "9"), (35, "P"), (37, "2")]),
        at(&[(6, "A"), (17, "K"), (19, "CUSNO")]),
        at(&[(6, "A"), (17, "K"), (19, "NAME")]),
    ];
    let (source, _) = Source::from_text(&lines.join("\n"));
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    let def = c.definition.ok_or_else(|| c.listing.clone())?;
    let id = db::file::create_pf(
        &mut t,
        lib,
        name("CUST"),
        &def,
        &CreateFileOpts {
            now: 1,
            ..CreateFileOpts::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let mut m = db::Member::open_by(&mut t, id, MemberSpec::First).map_err(|e| e.to_string())?;
    for (n, w) in [
        (1, "Alice"),
        (3, "Carol"),
        (3, "Bob"),
        (7, "Grace"),
        (10, "Judy"),
    ] {
        m.write_new(&mut t, &customer(n, w, "12.50"), &[])
            .map_err(|e| e.to_string())?;
    }
    t.commit().map_err(|e| e.to_string())?;
    Ok((Rc::new(RefCell::new(store)), id))
}

/// The parameter buffers of a `parms` line: `'text'`, numbers as
/// `packed(15,5)`, `x'..'`.
pub fn parms(rest: &str) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = rest.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let hex = chars[i].eq_ignore_ascii_case(&'x') && chars.get(i + 1) == Some(&'\'');
        if hex || chars[i] == '\'' {
            i += if hex { 2 } else { 1 };
            let start = i;
            while i < chars.len() && chars[i] != '\'' {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            i += 1;
            if hex {
                let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                out.push(
                    (0..clean.len() / 2)
                        .map(|k| {
                            u8::from_str_radix(&clean[2 * k..2 * k + 2], 16)
                                .map_err(|e| e.to_string())
                        })
                        .collect::<Result<Vec<u8>, String>>()?,
                );
            } else {
                out.push(ccsid::to_ebcdic(&text).map_err(|_| format!("{text}: not CP037"))?);
            }
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let d = qdecimal::Decimal::parse(&word).map_err(|_| format!("{word}: not a number"))?;
        let p = qdecimal::Packed::new(15, 5).expect("valid");
        out.push(
            p.encode(&d, qdecimal::Rounding::Truncate)
                .map_err(|e| e.to_string())?,
        );
    }
    Ok(out)
}

/// A module's text bound as its own program in `QGPL`, named after
/// the module.
pub fn program(text: &str) -> Result<ile::ProgramObject, String> {
    let m = qir::parse(text).map_err(|e| e.to_string())?;
    let actgrp = match &m.actgrp {
        qir::ActGrp::New => ActGrpAttr::New,
        qir::ActGrp::Caller => ActGrpAttr::Caller,
        qir::ActGrp::Named(n) => ActGrpAttr::Named(n.clone()),
    };
    let kind = if m.entry().is_some() {
        PgmKind::Pgm
    } else {
        PgmKind::SrvPgm
    };
    let mo = ModuleObject::compile(m, CreateData::default(), CLOCK).map_err(|e| e.to_string())?;
    let mut opts = BindOptions::new(&mo.name.clone(), kind);
    opts.attr.actgrp = actgrp;
    opts.allow_unresolved = true;
    ile::bind(BindInput {
        modules: vec![(mo, "QGPL".into())],
        bnddirs: Vec::new(),
        lookup: &|_| None,
        options: opts,
    })
    .map(|(p, _)| p)
    .map_err(|e| e.to_string())
}

/// The DDS of `KITPRT`, the kit's printer file.
pub const KITPRT: &str = "\n     A          R HEADER                    SKIPB(1) SPACEA(2)
\n     A                                     1'Kit report' UNDERLINE
\n     A                                    40'Page'
\n     A                                    +1PAGNBR
\n     A          R DETAIL                    SPACEA(1)
\n     A            CUST          10A        5
\n     A            AMOUNT         9S 2     +2EDTCDE(J)
\n     A  50                                +1'*'
\n     A          R TOTAL                     SPACEB(1)
\n     A                                     5'End of report' HIGHLIGHT
";

/// The kit's spooled files, shared between the job and the report.
#[derive(Clone, Default)]
pub struct KitSpool(pub Rc<RefCell<MemSpool>>);

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

/// `KITPRT` compiled.
fn kitprt() -> Result<wm::obj::prtf::Prtf, String> {
    let (src, _) = dds::Source::from_text(KITPRT);
    let c = dds::compile_prtf(&src, &dds::NoRefs, &dds::CompileOpts::default());
    let ids = c.ids().join(" ");
    let def = c.definition.ok_or_else(|| format!("KITPRT: {ids}"))?;
    Ok(wm::obj::prtf::Prtf {
        formats: dds::prtf::codec::encode(&def),
        ..wm::obj::prtf::Prtf::default()
    })
}

/// The spooled files as the expected file holds them: each file's
/// counts and text, page by page.
#[must_use]
pub fn render_spool(spool: &MemSpool) -> String {
    let mut out = String::new();
    for (id, f) in &spool.files {
        let _ = writeln!(
            out,
            "spooled {id} {}: {} pages, {} records, {}",
            f.file,
            f.pages,
            f.records,
            if f.closed { "closed" } else { "open" }
        );
        let (pages, _) = spool::scs::decode(&f.data);
        for (page, line, text) in spool::view::rows(&pages) {
            if !text.is_empty() {
                let _ = writeln!(out, "  {page}/{line:<3} {text}");
            }
        }
    }
    out
}

/// A job with the kit's data: `QGPL/CUST`, `*LDA`, a decimal area
/// `QGPL/COUNTER` holding 41, a queue `QGPL/Q1`, the system APIs with
/// the fixed clock, and the case's replies.
pub fn job_for(case: &Case, src: MemSource, programs: &[String]) -> Result<Box<Job>, String> {
    let mut job = Job::boxed(Box::new(src));
    job.usrspcs.set_source(Box::new(MemUserSpaces::default()));
    let mut data = MemData::default();
    let mut counter = DataAreaValue::decimal(5, 0);
    counter.value = qdecimal::Packed::new(5, 0)
        .expect("valid")
        .encode(
            &qdecimal::Decimal::new(41, 0).expect("fits"),
            qdecimal::Rounding::Truncate,
        )
        .expect("fits");
    data.add_area("QGPL", "COUNTER", counter);
    data.add_queue("QGPL", "Q1");
    job.data = Some(Box::new(data));
    let (store, cust) = customers()?;
    let resolve = db::odp::session::resolve_in(name("QGPL"), vec![(name("CUST"), cust)]);
    job.io = Some(Box::new(StoreIo::new(
        Box::new(store),
        JobHandle(1),
        Box::new(resolve),
    )));
    let mut provider = MemProvider::default();
    provider.clock = Some(LocalNow {
        unix: CLOCK,
        offset_min: 0,
        zone: "QN0000UTC".into(),
        dst: false,
    });
    provider.add_dtaq("QGPL", "TESTQ", 100, 0, 'F');
    // QSYRUSRI: every profile as CRTUSRPRF would make it.
    provider.sec = Some(Box::new(|q| match q {
        sec::api::Query::Profile { user } => Ok(sec::api::Answer::Profile {
            data: sec::profile::UsrPrf::new(user, 40).encode(),
        }),
        _ => Err(sysapi::ApiFail::id("CPF3CF2")),
    }));
    for p in programs {
        provider.objects.push(sysapi::ObjInfo {
            name: p.clone(),
            lib: "QGPL".into(),
            ty: "*PGM".into(),
            attr: "QIR".into(),
            text: format!("kit program {p}"),
            owner: "QSYS".into(),
            created: "1261021120000".into(),
            changed: "1261021120000".into(),
            source: None,
            user_attr: String::new(),
        });
    }
    sysapi::install(&mut job, Rc::new(RefCell::new(provider)));
    for r in &case.replies {
        job.inquiry_replies.push_back(r.clone());
    }
    for r in &case.dsply {
        job.replies.push_back(r.clone());
    }
    Ok(job)
}

/// What a run produced, as the expected file holds it.
#[must_use]
pub fn render(job: &Job, parms: &[Vec<u8>]) -> String {
    let mut out = String::from("console:\n");
    for l in &job.console {
        let _ = writeln!(out, "  {l}");
    }
    let ids = |v: &[mi::Exception]| {
        v.iter()
            .map(|e| e.msgid.clone())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let _ = writeln!(out, "raised: {}", ids(&job.raised));
    let _ = writeln!(out, "handled: {}", ids(&job.handled));
    let _ = writeln!(
        out,
        "pending: {}",
        job.exception.as_ref().map_or("none", |e| e.msgid.as_str())
    );
    let _ = writeln!(out, "replies: {}", job.inquiry_log.join(" "));
    let _ = writeln!(
        out,
        "dump: {}",
        if job.last_dump.is_some() { "yes" } else { "no" }
    );
    for (i, b) in parms.iter().enumerate() {
        let hex: String = b.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:02X}");
            s
        });
        let _ = writeln!(out, "parm {}: {hex}", i + 1);
    }
    out
}

/// Run a case in memory; the rendered outcome.
pub fn run(case: &Case) -> Result<String, String> {
    let mut src = MemSource::default();
    let mut names = Vec::new();
    for m in &case.modules {
        let p = program(m).map_err(|e| format!("{}: {e}", case.name))?;
        names.push(p.name.clone());
        src.add("QGPL", p);
    }
    let main = names
        .first()
        .cloned()
        .ok_or_else(|| format!("{}: no module", case.name))?;
    let job = job_for(case, src, &names)?;
    call(case, job, &main)
}

/// Call `main` in a job made by [`job_for`], with the kit's spool and
/// `KITPRT` attached; the rendered outcome, spooled files included.
pub fn call(case: &Case, mut job: Box<Job>, main: &str) -> Result<String, String> {
    let spool = KitSpool::default();
    spool
        .0
        .borrow_mut()
        .prtfs
        .insert("KITPRT".into(), kitprt()?);
    job.spool = Some(Box::new(spool.clone()));
    let mut buffers = parms(&case.parms)?;
    let addrs: Vec<u64> = buffers.iter_mut().map(|b| b.as_mut_ptr() as u64).collect();
    if let Err(c) = job.call_program(main, "QGPL", &addrs) {
        job.console.push(format!("call failed: {}", c.msgid()));
    }
    job.close_printers();
    sysapi::uninstall(&job);
    Ok(render(&job, &buffers) + &render_spool(&spool.0.borrow()))
}
