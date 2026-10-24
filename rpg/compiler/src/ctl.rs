// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compile options: the command's parameters, then the control
//! specification's keywords, which override them. `ACTGRP`, `BNDDIR`
//! or `STGMDL` on the control specification imply `DFTACTGRP(*NO)`.

use rpgfront::Diag;
use rpgfront::ast::Keyword;

use crate::decl::{int_arg, str_arg, word_arg};
use crate::diag::ids;
use crate::types::{DatFmt, TimFmt};

/// Which command compiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Command {
    /// `CRTBNDRPG`: a program.
    #[default]
    CrtBndRpg,
    /// `CRTRPGMOD`: a module.
    CrtRpgMod,
    /// `CRTSQLRPGI`, after its precompile.
    CrtSqlRpgi,
}

impl Command {
    /// The command name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::CrtBndRpg => "CRTBNDRPG",
            Self::CrtRpgMod => "CRTRPGMOD",
            Self::CrtSqlRpgi => "CRTSQLRPGI",
        }
    }
}

/// The command's parameters.
#[derive(Clone, Debug)]
pub struct Options {
    /// The command.
    pub command: Command,
    /// The object (program or module) name.
    pub name: String,
    /// Its library.
    pub lib: String,
    /// `SRCFILE` library, file and `SRCMBR`.
    pub src: (String, String, String),
    /// `TEXT`.
    pub text: String,
    /// `GENLVL`.
    pub genlvl: u8,
    /// `OPTION` values.
    pub option: Vec<String>,
    /// `DBGVIEW`.
    pub dbgview: String,
    /// `OUTPUT`.
    pub output: String,
    /// `OPTIMIZE`.
    pub optimize: String,
    /// `INDENT`.
    pub indent: String,
    /// `TRUNCNBR`.
    pub truncnbr: bool,
    /// `TGTRLS`.
    pub tgtrls: String,
    /// `ALWNULL`.
    pub alwnull: String,
    /// `DFTACTGRP`.
    pub dftactgrp: bool,
    /// `ACTGRP`.
    pub actgrp: String,
    /// `BNDDIR`.
    pub bnddir: Vec<String>,
    /// `DEFINE`.
    pub define: Vec<String>,
    /// `USRPRF`.
    pub usrprf: String,
    /// `REPLACE`.
    pub replace: bool,
    /// `CRTSQLRPGI`'s SQL options: the source is precompiled first.
    pub sql: Option<rpgsqlpre::Options>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            command: Command::CrtBndRpg,
            name: "MAIN".into(),
            lib: "QGPL".into(),
            src: ("QGPL".into(), "QRPGLESRC".into(), "MAIN".into()),
            text: String::new(),
            genlvl: 10,
            option: Vec::new(),
            dbgview: "*STMT".into(),
            output: "*PRINT".into(),
            optimize: "*NONE".into(),
            indent: "*NONE".into(),
            truncnbr: true,
            tgtrls: "*CURRENT".into(),
            alwnull: "*NO".into(),
            dftactgrp: true,
            actgrp: "QILE".into(),
            bnddir: Vec::new(),
            define: Vec::new(),
            usrprf: "*USER".into(),
            replace: true,
            sql: None,
        }
    }
}

/// The options in force after the control specification.
#[derive(Clone, Debug, Default)]
pub struct Ctl {
    /// `DFTACTGRP`.
    pub dftactgrp: bool,
    /// `ACTGRP`: `*NEW`, `*CALLER`, `*STGMDL` or a name.
    pub actgrp: String,
    /// `MAIN(proc)`: a linear-main module.
    pub main: Option<String>,
    /// `NOMAIN`.
    pub nomain: bool,
    /// `DATFMT`.
    pub datfmt: DatFmt,
    /// `TIMFMT`.
    pub timfmt: TimFmt,
    /// `DATFMT`'s separator when one is given (`'\0'` for none).
    pub datsep: Option<char>,
    /// `TIMFMT`'s separator when one is given.
    pub timsep: Option<char>,
    /// `DECEDIT`: the decimal point and the thousands separator.
    pub decedit: (char, char),
    /// `EXPROPTS(*RESDECPOS)`.
    pub resdecpos: bool,
    /// `DECPREC`.
    pub decprec: u8,
    /// `BNDDIR`.
    pub bnddir: Vec<String>,
    /// `TEXT`.
    pub text: String,
    /// `ALWNULL`.
    pub alwnull: String,
    /// `DEBUG(*YES)`: `DUMP` operations run.
    pub debug: bool,
    /// `COPYRIGHT`.
    pub copyright: String,
    /// `INDENT`.
    pub indent: String,
    /// `OPTION`.
    pub option: Vec<String>,
    /// `TRUNCNBR`.
    pub truncnbr: bool,
    /// `GENLVL`.
    pub genlvl: u8,
    /// `THREAD`: `*SERIALIZE`, `*CONCURRENT`, or empty.
    pub thread: String,
}

/// Control keywords the compiler accepts.
const KNOWN: &[&str] = &[
    "ACTGRP", "ALLOC", "ALTSEQ", "ALWNULL", "AUT", "BNDDIR", "CCSID", "CCSIDCVT", "COPYNEST", "COPYRIGHT",
    "CURSYM", "CVTOPT", "DATEDIT", "DATFMT", "DCLOPT", "DEBUG", "DECEDIT", "DECPREC", "DFTACTGRP", "DFTNAME",
    "ENBPFRCOL", "EXPROPTS", "EXTBININT", "FIXNBR", "FLTDIV", "FTRANS", "GENLVL", "INDENT", "INTPREC",
    "LANGID", "MAIN", "NOMAIN", "OPENOPT", "OPTIMIZE", "OPTION", "PGMINFO", "PRFDTA", "REQPREXP", "SRTSEQ",
    "STGMDL", "TEXT", "THREAD", "TIMFMT", "TRUNCNBR", "USRPRF", "VALIDATE",
];

/// A control keyword's parameter that is not valid; with `skip`, IBM
/// also skips the rest of the keyword.
fn bad_parm(diags: &mut Vec<Diag>, k: &Keyword, id: &'static str, skip: bool) {
    let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
    diags.push(Diag::new(id, 20, at).arg(k.name.clone()));
    if skip {
        diags.push(Diag::new(ids::TEXT_SKIPPED, 0, at));
    }
}

/// Merge the command options and the control keywords.
pub fn resolve(o: &Options, kws: &[Keyword], diags: &mut Vec<Diag>) -> Ctl {
    let mut c = Ctl {
        dftactgrp: o.dftactgrp && o.command == Command::CrtBndRpg,
        actgrp: o.actgrp.clone(),
        decedit: ('.', ','),
        decprec: 31,
        bnddir: o.bnddir.iter().filter(|b| !b.starts_with('*')).cloned().collect(),
        text: o.text.clone(),
        alwnull: o.alwnull.clone(),
        indent: o.indent.clone(),
        option: o.option.clone(),
        truncnbr: o.truncnbr,
        genlvl: o.genlvl,
        ..Ctl::default()
    };
    let mut seen: Vec<&str> = Vec::new();
    for k in kws {
        // Each keyword is known, given once and right for the command.
        if !KNOWN.contains(&k.name.as_str()) {
            diags.push(Diag::new(ids::UNKNOWN_CTL_KEYWORD, 20, k.at).arg(k.name.clone()));
            continue;
        }
        // BNDDIR alone may be repeated.
        if seen.contains(&k.name.as_str()) && k.name != "BNDDIR" {
            diags.push(Diag::new(ids::CTL_KEYWORD_TWICE, 20, k.at).arg(k.name.clone()));
            continue;
        }
        seen.push(&k.name);
        if o.command == Command::CrtRpgMod && matches!(k.name.as_str(), "DFTACTGRP" | "ACTGRP" | "USRPRF") {
            diags.push(Diag::new(ids::CTL_NOT_FOR_MODULE, 20, k.at).arg(k.name.clone()));
            continue;
        }
        match k.name.as_str() {
            "DFTACTGRP" => c.dftactgrp = word_arg(k, 0).as_deref() == Some("*YES"),
            "ACTGRP" => {
                let w = str_arg(k, 0).or_else(|| word_arg(k, 0)).unwrap_or_default().to_ascii_uppercase();
                if str_arg(k, 0).is_none() && !matches!(w.as_str(), "*NEW" | "*CALLER" | "*STGMDL") {
                    bad_parm(diags, k, ids::CTL_ACTGRP, true);
                    continue;
                }
                c.actgrp = w;
                c.dftactgrp = false;
            }
            "BNDDIR" => {
                c.dftactgrp = false;
                for i in 0..k.args.len() {
                    if let Some(s) = str_arg(k, i) {
                        c.bnddir.push(s.to_ascii_uppercase());
                    }
                }
            }
            "STGMDL" => c.dftactgrp = false,
            "THREAD" => c.thread = word_arg(k, 0).unwrap_or_default(),
            "MAIN" => {
                c.main = word_arg(k, 0);
                c.dftactgrp = false;
            }
            // A program has a main procedure.
            "NOMAIN" if o.command == Command::CrtBndRpg => {
                diags.push(Diag::new(ids::NOMAIN_PROGRAM, 20, k.at));
            }
            "NOMAIN" => c.nomain = true,
            "DATFMT" => {
                if let Some((f, s)) = word_arg(k, 0).and_then(|w| crate::dtfmt::field_date(&w)) {
                    c.datfmt = f;
                    c.datsep = Some(s);
                } else {
                    // Not valid: *ISO, and the rest is skipped.
                    let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
                    diags.push(Diag::new(ids::BAD_DATFMT, 20, at));
                    diags.push(Diag::new(ids::TEXT_SKIPPED, 0, at));
                }
            }
            "TIMFMT" => {
                if let Some((f, s)) = word_arg(k, 0).and_then(|w| crate::dtfmt::field_time(&w)) {
                    c.timfmt = f;
                    c.timsep = Some(s);
                } else {
                    bad_parm(diags, k, ids::BAD_TIMFMT, true);
                }
            }
            "DATEDIT" => {
                let w = word_arg(k, 0).unwrap_or_default();
                if !["*MDY", "*DMY", "*YMD"].iter().any(|f| w.starts_with(f) && w.len() <= f.len() + 1) {
                    bad_parm(diags, k, ids::CTL_DATEDIT, false);
                }
            }
            "DECEDIT" => {
                let ok = str_arg(k, 0).is_some_and(|s| matches!(s.as_str(), "." | "," | "0." | "0,"))
                    || word_arg(k, 0).as_deref() == Some("*JOBRUN") && str_arg(k, 0).is_none();
                if !ok {
                    bad_parm(diags, k, ids::CTL_DECEDIT, false);
                    continue;
                }
                if let Some(s) = str_arg(k, 0) {
                    c.decedit = match s.as_str() {
                        "," => (',', '.'),
                        "0," => (',', '.'),
                        "0." => ('.', ','),
                        _ => ('.', ','),
                    };
                }
            }
            "EXPROPTS" => {
                let known = ["*MAXDIGITS", "*RESDECPOS", "*ALWBLANKNUM", "*USEDECEDIT", "*STRICTKEYS"];
                if (0..k.args.len()).any(|i| word_arg(k, i).is_none_or(|w| !known.contains(&w.as_str()))) {
                    bad_parm(diags, k, ids::CTL_EXPROPTS, true);
                    continue;
                }
                for i in 0..k.args.len() {
                    if word_arg(k, i).as_deref() == Some("*RESDECPOS") {
                        c.resdecpos = true;
                    }
                }
            }
            "DECPREC" => c.decprec = int_arg(k, 0).and_then(|v| u8::try_from(v).ok()).unwrap_or(31),
            "TEXT" => c.text = str_arg(k, 0).unwrap_or_default(),
            "ALWNULL" => c.alwnull = word_arg(k, 0).unwrap_or_else(|| "*NO".into()),
            "DEBUG" => c.debug = word_arg(k, 0).is_none_or(|w| w == "*YES" || w == "*DUMP"),
            "COPYRIGHT" => match str_arg(k, 0) {
                Some(s) => c.copyright = s,
                None => bad_parm(diags, k, ids::CTL_COPYRIGHT, false),
            },
            "INDENT" => c.indent = str_arg(k, 0).or_else(|| word_arg(k, 0)).unwrap_or_default(),
            "OPTION" => {
                let known = [
                    "XREF", "GEN", "SECLVL", "SHOWCPY", "EXPDDS", "EXT", "SHOWSKP", "SRCSTMT", "DEBUGIO", "UNREF",
                ];
                let ok = |w: &str| {
                    let w = w.strip_prefix('*').unwrap_or("");
                    known.contains(&w.strip_prefix("NO").unwrap_or(w)) || known.contains(&w)
                };
                if (0..k.args.len()).any(|i| word_arg(k, i).is_none_or(|w| !ok(&w))) {
                    bad_parm(diags, k, ids::CTL_OPTION, true);
                    continue;
                }
                c.option.extend((0..k.args.len()).filter_map(|i| word_arg(k, i)));
            }
            "TRUNCNBR" => c.truncnbr = word_arg(k, 0).as_deref() != Some("*NO"),
            "GENLVL" => c.genlvl = int_arg(k, 0).and_then(|v| u8::try_from(v).ok()).unwrap_or(10),
            _ => {}
        }
    }
    c
}
