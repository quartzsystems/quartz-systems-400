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
}

/// Control keywords the compiler accepts.
const KNOWN: &[&str] = &[
    "ACTGRP", "ALLOC", "ALTSEQ", "ALWNULL", "AUT", "BNDDIR", "CCSID", "CCSIDCVT", "COPYNEST", "COPYRIGHT",
    "CURSYM", "CVTOPT", "DATEDIT", "DATFMT", "DCLOPT", "DEBUG", "DECEDIT", "DECPREC", "DFTACTGRP", "DFTNAME",
    "ENBPFRCOL", "EXPROPTS", "EXTBININT", "FIXNBR", "FLTDIV", "FTRANS", "GENLVL", "INDENT", "INTPREC",
    "LANGID", "MAIN", "NOMAIN", "OPENOPT", "OPTIMIZE", "OPTION", "PGMINFO", "PRFDTA", "REQPREXP", "SRTSEQ",
    "STGMDL", "TEXT", "THREAD", "TIMFMT", "TRUNCNBR", "USRPRF", "VALIDATE",
];

/// Merge the command options and the control keywords.
pub fn resolve(o: &Options, kws: &[Keyword], diags: &mut Vec<Diag>) -> Ctl {
    let mut c = Ctl {
        dftactgrp: o.dftactgrp && o.command == Command::CrtBndRpg,
        actgrp: o.actgrp.clone(),
        decedit: ('.', ','),
        decprec: 31,
        bnddir: o.bnddir.clone(),
        text: o.text.clone(),
        alwnull: o.alwnull.clone(),
        indent: o.indent.clone(),
        option: o.option.clone(),
        truncnbr: o.truncnbr,
        genlvl: o.genlvl,
        ..Ctl::default()
    };
    for k in kws {
        if !KNOWN.contains(&k.name.as_str()) {
            diags.push(Diag::new(ids::BAD_KEYWORD_VALUE, 30, k.at).arg(k.name.clone()));
            continue;
        }
        match k.name.as_str() {
            "DFTACTGRP" => c.dftactgrp = word_arg(k, 0).as_deref() == Some("*YES"),
            "ACTGRP" => {
                c.actgrp = str_arg(k, 0).or_else(|| word_arg(k, 0)).unwrap_or_default().to_ascii_uppercase();
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
            "MAIN" => {
                c.main = word_arg(k, 0);
                c.dftactgrp = false;
            }
            "NOMAIN" => c.nomain = true,
            "DATFMT" => {
                if let Some((f, s)) = word_arg(k, 0).and_then(|w| crate::dtfmt::field_date(&w)) {
                    c.datfmt = f;
                    c.datsep = Some(s);
                }
            }
            "TIMFMT" => {
                if let Some((f, s)) = word_arg(k, 0).and_then(|w| crate::dtfmt::field_time(&w)) {
                    c.timfmt = f;
                    c.timsep = Some(s);
                }
            }
            "DECEDIT" => {
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
            "COPYRIGHT" => c.copyright = str_arg(k, 0).unwrap_or_default(),
            "INDENT" => c.indent = str_arg(k, 0).or_else(|| word_arg(k, 0)).unwrap_or_default(),
            "OPTION" => c.option.extend((0..k.args.len()).filter_map(|i| word_arg(k, i))),
            "TRUNCNBR" => c.truncnbr = word_arg(k, 0).as_deref() != Some("*NO"),
            "GENLVL" => c.genlvl = int_arg(k, 0).and_then(|v| u8::try_from(v).ok()).unwrap_or(10),
            _ => {}
        }
    }
    c
}
