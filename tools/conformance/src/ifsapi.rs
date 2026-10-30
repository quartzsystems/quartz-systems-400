// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS API corpus: `conformance/ifs/api/programs/*.qir`, IR
//! programs calling the IFS procedures and program APIs against
//! `/home/QSTEST` of an IFS in memory, their console lines compared with
//! `expected/`. The RPG probes under `rpg/` print the same lines on the
//! oracle.
//!
//! A program's text is completed before it is assembled: the shared
//! prototypes (`protos.qir`) follow its `module` line, `z"TEXT"` is the
//! text in EBCDIC with a NUL, `q"PATH"` a `Qlg_Path_Name_T` holding the
//! path in place (CCSID 0, the job's), and `bytes(*)` before either is
//! the literal's length.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mi::exec::Program;
use sysapi::MemProvider;
use sysapi::ifs::memory::MemIfs;

/// Where the corpus lives.
pub struct Kit {
    /// `conformance/ifs/api`.
    pub root: PathBuf,
}

/// One program.
pub struct Case {
    /// Its name.
    pub name: String,
    /// Its text as checked in.
    pub text: String,
}

impl Kit {
    /// The corpus checked in beside this crate.
    #[must_use]
    pub fn locate() -> Self {
        Self {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("ifs/api"),
        }
    }

    /// Every program, by name.
    ///
    /// # Errors
    ///
    /// The directory cannot be read.
    pub fn cases(&self) -> std::io::Result<Vec<Case>> {
        let mut out = Vec::new();
        for e in fs::read_dir(self.root.join("programs"))? {
            let p = e?.path();
            if p.extension().is_some_and(|x| x == "qir") {
                out.push(Case {
                    name: p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                    text: fs::read_to_string(&p)?.replace("\r\n", "\n"),
                });
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

    /// Where a run's outcomes go.
    #[must_use]
    pub fn ours_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/conformance-ifsapi/ours")
    }

    /// The prototypes every program gets.
    ///
    /// # Errors
    ///
    /// The file cannot be read.
    pub fn protos(&self) -> std::io::Result<String> {
        Ok(fs::read_to_string(self.root.join("protos.qir"))?.replace("\r\n", "\n"))
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, c| {
        let _ = write!(s, "{c:02X}");
        s
    })
}

/// The bytes a `z"..."` or `q"..."` literal stands for.
fn literal(kind: char, text: &str) -> Vec<u8> {
    let name = ccsid::to_ebcdic(text).unwrap_or_default();
    if kind == 'z' {
        let mut b = name;
        b.push(0);
        return b;
    }
    let mut b = vec![0u8; 32];
    b[16..20].copy_from_slice(&u32::try_from(name.len()).unwrap_or(0).to_be_bytes());
    b[20] = 0x61;
    b.extend_from_slice(&name);
    b
}

/// Expand the literals of one line.
fn expand_line(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    let next = |s: &str| [s.find("z\""), s.find("q\"")].into_iter().flatten().min();
    while let Some(i) = next(rest) {
        let kind = rest.as_bytes()[i] as char;
        let after = &rest[i + 2..];
        let Some(end) = after.find('"') else { break };
        let bytes = literal(kind, &after[..end]);
        let mut head = rest[..i].to_string();
        if let Some(at) = head.rfind("bytes(*)") {
            head.replace_range(at..at + 8, &format!("bytes({})", bytes.len()));
        }
        out.push_str(&head);
        let _ = write!(out, "x\"{}\"", hex(&bytes));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// A program's text with the prototypes and literals in place.
#[must_use]
pub fn expand(text: &str, protos: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        // Literals only where a declaration starts: no instruction has one.
        let l = if line.trim_start().starts_with("static ") {
            expand_line(line)
        } else {
            line.to_string()
        };
        out.push_str(&l);
        out.push('\n');
        if line.starts_with("module ") {
            out.push_str(protos);
            if !protos.ends_with('\n') {
                out.push('\n');
            }
        }
    }
    out
}

/// Run one program as `QSTEST` in `/home/QSTEST` of a fresh IFS; its
/// console lines and what it left behind.
///
/// # Errors
///
/// The program does not assemble, or its call fails.
pub fn run(case: &Case, protos: &str) -> Result<String, String> {
    let text = expand(&case.text, protos);
    let sys = MemIfs::new();
    let mut p = Program::from_text(&text).map_err(|e| format!("{}: {e}", case.name))?;
    sysapi::install(p.job(), Rc::new(RefCell::new(MemProvider::default())));
    let client = Rc::new(sys.client("QSTEST", 1, 37));
    sysapi::ifs::configure(p.job(), client, 37);
    let r = p.call_entry();
    let j = p.job();
    let mut out = String::new();
    for l in &j.console {
        let _ = writeln!(out, "{l}");
    }
    match r {
        Ok(()) => {}
        Err(e) => {
            let _ = writeln!(out, "ended: {e}");
        }
    }
    let left = j.fds.open_count();
    let _ = writeln!(out, "descriptors left open: {left}");
    let _ = writeln!(out, "pending: {}", if j.pending() { "yes" } else { "none" });
    Ok(out)
}
