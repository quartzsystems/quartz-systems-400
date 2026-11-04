// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CRTRPGPGM`: the member is converted (`cvt`) and the conversion
//! compiled by the ILE RPG compiler into a program in the default
//! activation group, as an OPM program runs. The ILE compiler's
//! diagnostics are reported as RPG/400's (`QRGnnnn`) on the RPG III
//! statement they came from; those RPG/400 does not give are dropped.
//! The listing has RPG/400's sections (`listing`).

mod listing;
mod msgs;

use rpgfront::source::Record;
use rpgfront::{Includes, Member};

use crate::cvt::{self, Conversion};

pub use msgs::{qrg, text};

/// `CRTRPGPGM`'s parameters.
#[derive(Clone, Debug)]
pub struct Options {
    /// `PGM`: name and library.
    pub pgm: (String, String),
    /// `SRCFILE` library and file, `SRCMBR`.
    pub src: (String, String, String),
    /// `GENLVL`.
    pub genlvl: u8,
    /// `TEXT`.
    pub text: String,
    /// `OPTION`.
    pub option: Vec<String>,
    /// `GENOPT`.
    pub genopt: Vec<String>,
    /// `REPLACE`.
    pub replace: bool,
    /// `TGTRLS`.
    pub tgtrls: String,
    /// `USRPRF`.
    pub usrprf: String,
    /// `IGNDECERR`.
    pub igndecerr: bool,
    /// `ALWNULL`.
    pub alwnull: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            pgm: ("MAIN".into(), "QGPL".into()),
            src: ("QGPL".into(), "QRPGSRC".into(), "MAIN".into()),
            genlvl: 9,
            text: "*SRCMBRTXT".into(),
            option: vec!["*SOURCE".into(), "*XREF".into(), "*GEN".into(), "*NODUMP".into(), "*NOSECLVL".into(), "*NOSRCDBG".into(), "*NOLSTDBG".into()],
            genopt: vec!["*NOLIST".into(), "*NOXREF".into(), "*NOATR".into(), "*NODUMP".into(), "*NOOPTIMIZE".into()],
            replace: true,
            tgtrls: "*CURRENT".into(),
            usrprf: "*USER".into(),
            igndecerr: false,
            alwnull: false,
        }
    }
}

/// One RPG/400 diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diag {
    /// The statement number (the RPG III record's sequence number).
    pub stmt: u32,
    /// `QRGnnnn`.
    pub msgid: String,
    /// Severity.
    pub severity: u8,
    /// Where the listing prints it.
    pub place: Place,
    /// Values for the text.
    pub args: Vec<String>,
}

/// Where a diagnostic is printed in the listing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    /// After its source statement.
    Source,
    /// In the additional diagnostics.
    Additional,
    /// On its cross-reference entry.
    Xref,
}

/// What `CRTRPGPGM` made.
#[derive(Clone, Debug, Default)]
pub struct Compiled {
    /// The conversion compiled.
    pub conversion: Conversion,
    /// The ILE RPG compile of it.
    pub inner: rpgc::Compiled,
    /// The RPG/400 diagnostics, by statement.
    pub diags: Vec<Diag>,
    /// The program is created.
    pub created: bool,
    /// The highest severity.
    pub max_severity: u8,
    /// The listing, one record per line with its forms control.
    pub listing: Vec<String>,
}

/// The sequence number of a record as a statement number.
fn stmt_of(seq: &str) -> u32 {
    seq.trim().parse().unwrap_or(0)
}

/// The names the ILE compiler's cross-reference marks unreferenced,
/// on the RPG III statement that defines them (`QRG7031`).
fn unreferenced(main: &Member, inner: &rpgc::Compiled, diags: &mut Vec<Diag>) {
    // Files are not names RPG/400 reports so (an unused file is QRG7064).
    let f = rpgspecs::rpg3_form("F");
    let files: Vec<String> = main
        .records
        .iter()
        .filter(|r| r.text.chars().nth(5).is_some_and(|c| c.eq_ignore_ascii_case(&'F')) && r.text.chars().nth(6) != Some('*'))
        .map(|r| f.cut(&r.text, "name").trim().to_ascii_uppercase())
        .filter(|n| !n.is_empty())
        .collect();
    for l in &inner.listing {
        let t = l.get(1..).unwrap_or("").trim_start();
        let Some(rest) = t.strip_prefix("*RNF7031") else { continue };
        let w: Vec<&str> = rest.split_whitespace().collect();
        let Some(name) = w.first() else { continue };
        if name.chars().all(|c| c.is_ascii_digit()) || files.iter().any(|f| f == name) {
            continue;
        }
        // A field of an externally described file is defined by no
        // statement of the source (RPG/400's statement 0).
        let defined = w.iter().find_map(|x| x.strip_suffix('D').and_then(|n| n.parse::<u32>().ok()));
        let stmt = defined
            .and_then(|d| inner.read.lines.iter().find(|r| inner.listing_line(r.no) == d))
            .map_or(0, |line| stmt_of(&line.seq));
        let name = name.strip_prefix("*IN").filter(|n| n.len() == 2).unwrap_or(name).to_string();
        if !diags.iter().any(|d| d.msgid == "QRG7031" && d.stmt == stmt && d.args.first() == Some(&name)) {
            diags.push(Diag {
                stmt,
                msgid: "QRG7031".into(),
                severity: 0,
                place: Place::Xref,
                args: vec![name],
            });
        }
    }
}

/// An overflow indicator nothing conditions or sets is not referenced
/// (`QRG7031` on the file's statement), which the ILE compiler does not
/// report.
fn unused_overflow(main: &Member, diags: &mut Vec<Diag>) {
    let f = rpgspecs::rpg3_form("F");
    for r in &main.records {
        let t = &r.text;
        if t.chars().nth(5).is_none_or(|c| !c.eq_ignore_ascii_case(&'F')) || t.chars().nth(6) == Some('*') {
            continue;
        }
        let ind = f.cut(t, "overflow").trim().to_ascii_uppercase();
        if ind.is_empty() {
            continue;
        }
        let used = main.records.iter().any(|o| {
            let o = o.text.to_ascii_uppercase();
            if o.chars().nth(6) == Some('*') {
                return false;
            }
            let cols = |a: usize, b: usize| rpgspecs::cut(&o, a, b);
            match o.chars().nth(5) {
                Some('C') => {
                    let c = rpgspecs::rpg3_form("C");
                    cols(7, 17).contains(&ind)
                        || ["hi", "lo", "eq"].iter().any(|n| c.cut(&o, n).trim() == ind)
                        || ["factor1", "factor2", "result"].iter().any(|n| c.cut(&o, n).contains(&format!("*IN{ind}")))
                }
                Some('O') => cols(23, 31).contains(&ind),
                _ => false,
            }
        });
        let stmt = stmt_of(&r.seq);
        if !used && !diags.iter().any(|d| d.msgid == "QRG7031" && d.stmt == stmt && d.args.first() == Some(&ind)) {
            diags.push(Diag {
                stmt,
                msgid: "QRG7031".into(),
                severity: 0,
                place: Place::Xref,
                args: vec![ind],
            });
        }
    }
}

/// An indicator 01-99 that is set (a resulting, record identifying or
/// field indicator) and never used is not referenced (`QRG7031` on
/// the statement that first sets it), as RPG/400's cross-reference
/// says; the ILE compiler counts only those never set.
fn unused_indicators(main: &Member, diags: &mut Vec<Diag>) {
    let c = rpgspecs::rpg3_form("C");
    let ix = rpgspecs::rpg3_form("IX");
    let numeric = |t: &str| t.len() == 2 && t.chars().all(|ch| ch.is_ascii_digit()) && t != "00";
    let mut set: Vec<(String, u32)> = Vec::new();
    let mut used: Vec<String> = Vec::new();
    let mut whole_array = false;
    for r in main.records.iter().take_while(|r| !r.text.starts_with("**")) {
        let t = r.text.to_ascii_uppercase();
        if t.chars().nth(6) == Some('*') {
            continue;
        }
        let stmt = stmt_of(&r.seq);
        let mut sets = |n: String| {
            if numeric(&n) && !set.iter().any(|(x, _)| *x == n) {
                set.push((n, stmt));
            }
        };
        match t.chars().nth(5) {
            Some('C') => {
                let cond = rpgspecs::cut(&t, 9, 17);
                for k in 0..3 {
                    used.push(cond.chars().skip(k * 3 + 1).take(2).collect());
                }
                for n in ["hi", "lo", "eq"] {
                    sets(c.cut(&t, n).trim().to_string());
                }
                for n in ["factor1", "factor2", "result"] {
                    let f = c.cut(&t, n).trim().to_string();
                    if let Some(rest) = f.strip_prefix("*IN") {
                        let idx = rest.trim_start_matches(',');
                        if numeric(idx) {
                            used.push(idx.to_string());
                        } else {
                            whole_array = true;
                        }
                    }
                }
            }
            Some('O') => {
                let cond = rpgspecs::cut(&t, 23, 31);
                for k in 0..3 {
                    used.push(cond.chars().skip(k * 3 + 1).take(2).collect());
                }
            }
            Some('I') => {
                if t.chars().nth(6).is_some_and(|ch| ch != ' ') {
                    sets(rpgspecs::cut(&t, 19, 20).trim().to_string());
                } else {
                    for n in ["plus", "minus", "zero"] {
                        sets(ix.cut(&t, n).trim().to_string());
                    }
                }
            }
            _ => {}
        }
    }
    if whole_array {
        return;
    }
    for (n, stmt) in set {
        if !used.contains(&n) && !diags.iter().any(|d| d.msgid == "QRG7031" && d.args.first() == Some(&n)) {
            diags.push(Diag {
                stmt,
                msgid: "QRG7031".into(),
                severity: 0,
                place: Place::Xref,
                args: vec![n],
            });
        }
    }
}

/// A program-described output file no output specification names
/// (`QRG7064`, severity 40).
fn no_output(main: &Member, diags: &mut Vec<Diag>) {
    let (fs, os, cs) = (rpgspecs::rpg3_form("F"), rpgspecs::rpg3_form("O"), rpgspecs::rpg3_form("C"));
    let live = |t: &str, letter: char| {
        t.chars().nth(5).is_some_and(|c| c.eq_ignore_ascii_case(&letter)) && t.chars().nth(6) != Some('*')
    };
    for r in main.records.iter().take_while(|r| !r.text.starts_with("**")) {
        let t = r.text.to_ascii_uppercase();
        if !live(&t, 'F') || fs.cut(&t, "type").trim() != "O" || fs.cut(&t, "format").trim() != "F" {
            continue;
        }
        let name = fs.cut(&t, "name").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let named = main
            .records
            .iter()
            .take_while(|r| !r.text.starts_with("**"))
            .any(|x| {
                let u = x.text.to_ascii_uppercase();
                // An output specification, or an operation naming it.
                (live(&u, 'O') && os.cut(&u, "name").trim() == name)
                    || (live(&u, 'C') && cs.cut(&u, "factor2").trim() == name)
            });
        if !named {
            diags.push(Diag {
                stmt: stmt_of(&r.seq),
                msgid: "QRG7064".into(),
                severity: 40,
                place: Place::Additional,
                args: vec![name],
            });
        }
    }
}

/// A converted record as the compile takes it: the overflow line one
/// later (below); `FREE` and `DEBUG`,
/// which ILE RPG does not have, do nothing (a comment keeps the
/// numbering). `DEBUG` writes no debug records here.
fn compiled_text(t: &str) -> String {
    // RPG/400 spaces after a line when the next prints, so its overflow
    // indicator goes on when a line prints on the overflow line; ILE
    // RPG spaces at once and is on when the carriage reaches it. One
    // line later is the same overflow.
    if let Some(at) = t.find("FORMOFL(")
        && let Some(end) = t[at + 8..].find(')')
        && let Ok(n) = t[at + 8..at + 8 + end].trim().parse::<u32>()
    {
        return format!("{}FORMOFL({}){}", &t[..at], n + 1, &t[at + 9 + end..]);
    }
    let c = rpgspecs::form("C").expect("the C form");
    let is_c = t.chars().nth(5).is_some_and(|ch| ch.eq_ignore_ascii_case(&'C')) && t.chars().nth(6) != Some('*');
    if is_c && matches!(c.cut(t, "opcode").trim().to_ascii_uppercase().as_str(), "FREE" | "DEBUG") {
        let mut s: String = t.chars().take(6).collect();
        s.push('*');
        s.extend(t.chars().skip(7));
        return s;
    }
    t.to_string()
}

/// Compile an RPG III member.
pub fn compile(main: &Member, includes: &mut dyn Includes, ext: &mut dyn rpgc::ExtFiles, o: &Options) -> Compiled {
    let texts: Vec<String> = main.records.iter().map(|r| r.text.clone()).collect();
    let conversion = cvt::convert(&texts);
    // The converted member keeps each line's RPG III sequence number.
    let records = conversion
        .lines
        .iter()
        .map(|l| {
            let src = main.records.get(l.from);
            Record {
                seq: src.map_or_else(String::new, |r| r.seq.clone()),
                date: src.map_or_else(|| "000000".to_string(), |r| r.date.clone()),
                text: compiled_text(&l.text),
            }
        })
        .collect();
    let member = Member {
        lib: main.lib.clone(),
        file: main.file.clone(),
        name: main.name.clone(),
        ccsid: main.ccsid,
        records,
    };
    let ropts = rpgc::Options {
        command: rpgc::Command::CrtBndRpg,
        name: o.pgm.0.clone(),
        lib: o.pgm.1.clone(),
        src: o.src.clone(),
        text: o.text.clone(),
        genlvl: 10,
        dbgview: "*NONE".into(),
        dftactgrp: true,
        alwnull: if o.alwnull { "*YES".into() } else { "*NO".into() },
        replace: o.replace,
        ..rpgc::Options::default()
    };
    let inner = rpgc::compile(member, includes, ext, &ropts);
    let mut diags: Vec<Diag> = Vec::new();
    // Only what the converter could not convert is the compile's error;
    // its notes (CALL, FREE, DEBUG) are not.
    for m in conversion.messages.iter().filter(|m| m.id.starts_with("RNM059")) {
        diags.push(Diag {
            stmt: main.records.get(m.at).map_or(0, |r| stmt_of(&r.seq)),
            msgid: msgs::conversion_qrg(m.id).to_string(),
            severity: m.severity,
            place: Place::Source,
            args: Vec::new(),
        });
    }
    for d in &inner.diags {
        let Some((id, sev, place)) = qrg(d.msgid, d.severity) else { continue };
        let stmt = stmt_of(&inner.stmt_seq(d.at.line));
        let diag = Diag {
            stmt,
            msgid: id.clone(),
            severity: sev,
            place,
            args: d.args.clone(),
        };
        if !diags.iter().any(|x| x.msgid == diag.msgid && x.stmt == diag.stmt) {
            diags.push(diag);
        }
    }
    unreferenced(main, &inner, &mut diags);
    unused_overflow(main, &mut diags);
    unused_indicators(main, &mut diags);
    no_output(main, &mut diags);
    diags.sort();
    let max_severity = diags.iter().map(|d| d.severity).max().unwrap_or(0);
    let created = inner.created && max_severity <= o.genlvl && !o.option.iter().any(|x| x.eq_ignore_ascii_case("*NOGEN"));
    let mut c = Compiled {
        conversion,
        inner,
        diags,
        created,
        max_severity,
        listing: Vec::new(),
    };
    c.listing = listing::listing(main, &c, o);
    c
}
