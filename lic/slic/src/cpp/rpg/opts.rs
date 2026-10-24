// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The commands' parameters as one request, and the request as the
//! compiler's options.

use cmd::block::Parms;
use cmd::value::Value;
use rpgc::{Command, Compiled, Options};

use super::object::Shape;
use super::source::Source;

/// What the command asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the commands' *YES/*NO parameters
pub struct Request {
    /// The compiler's command.
    pub command: Command,
    /// What is made.
    pub shape: Shape,
    /// `CRTSQLRPGI`: the source may hold SQL.
    pub sql: bool,
    /// The object's name; `*CTLSPEC` for the control specification's.
    pub name: String,
    /// Its library as given (`*CURLIB` when not).
    pub lib: String,
    /// `SRCFILE`.
    pub srcfile: String,
    /// Its library.
    pub srclib: String,
    /// `SRCMBR`; `None` for the file's first member.
    pub mbr: Option<String>,
    /// `TEXT` as given (`*SRCMBRTXT` for the member's).
    pub text: String,
    /// `GENLVL`.
    pub genlvl: u8,
    /// `OPTION`.
    pub option: Vec<String>,
    /// `DBGVIEW`.
    pub dbgview: String,
    /// `OUTPUT(*PRINT)`.
    pub print: bool,
    /// `OPTIMIZE`.
    pub optimize: String,
    /// `INDENT`.
    pub indent: String,
    /// `TRUNCNBR(*YES)`.
    pub truncnbr: bool,
    /// `TGTRLS`.
    pub tgtrls: String,
    /// `ALWNULL`.
    pub alwnull: String,
    /// `DFTACTGRP(*YES)`.
    pub dftactgrp: bool,
    /// `ACTGRP`, `*STGMDL` resolved.
    pub actgrp: String,
    /// `BNDDIR`, as `LIB/NAME` or `NAME`.
    pub bnddir: Vec<String>,
    /// `DEFINE`.
    pub define: Vec<String>,
    /// `USRPRF(*OWNER)`.
    pub usrprf_owner: bool,
    /// `REPLACE(*YES)`.
    pub replace: bool,
    /// `CRTSQLRPGI`'s precompile options.
    pub sqlopts: Option<rpgc::sqlpre::Options>,
}

/// A parameter's value, or `dft` when it has none.
fn named(parms: &Parms, kw: &str, dft: &str) -> String {
    let v = parms.name(kw);
    if v.is_empty() { dft.to_string() } else { v }
}

/// A list parameter's items, upper case, without a lone `*NONE`.
fn words(parms: &Parms, kw: &str) -> Vec<String> {
    parms
        .list(kw)
        .iter()
        .map(|v| v.source().trim_matches('\'').trim().to_ascii_uppercase())
        .filter(|w| !w.is_empty() && w != "*NONE")
        .collect()
}

/// A qualified list item as `LIB/NAME`, or `NAME` for the library list.
fn qualified(v: &Value) -> String {
    match v {
        Value::Qual(parts) => {
            let name = parts.first().map(Value::source).unwrap_or_default();
            match parts.get(1).map(Value::source).as_deref() {
                None | Some("" | "*LIBL") => name,
                Some(lib) => format!("{lib}/{name}"),
            }
        }
        other => other.source(),
    }
    .to_ascii_uppercase()
}

/// Whether a `TGTRLS` value is one the compiler targets.
fn release_ok(r: &str) -> bool {
    matches!(r, "*CURRENT" | "*PRV") || rpgc::RELEASES.contains(&r)
}

/// The request of `CRTBNDRPG`, `CRTRPGMOD` or `CRTSQLRPGI`.
///
/// # Errors
///
/// A message identifier and text for a parameter this system cannot
/// honour.
pub fn request(cmdname: &str, parms: &Parms) -> Result<Request, String> {
    let (shape, command, kw, sql) = match cmdname {
        "CRTRPGMOD" => (Shape::Module, Command::CrtRpgMod, "MODULE", false),
        "CRTSQLRPGI" => match named(parms, "OBJTYPE", "*PGM").as_str() {
            "*MODULE" => (Shape::Module, Command::CrtRpgMod, "OBJ", true),
            "*SRVPGM" => (Shape::SrvPgm, Command::CrtRpgMod, "OBJ", true),
            _ => (Shape::Program, Command::CrtBndRpg, "OBJ", true),
        },
        _ => (Shape::Program, Command::CrtBndRpg, "PGM", false),
    };
    if parms.specified("SRCSTMF") {
        // TODO(phase10): stream files, with the integrated file system.
        return Err(
            "CPF0006 source stream files are not available; give SRCFILE and SRCMBR".into(),
        );
    }
    let (name, lib) = parms.qual(kw);
    let name = if name.is_empty() {
        "*CTLSPEC".into()
    } else {
        name.to_ascii_uppercase()
    };
    let (sf, sfl) = parms.qual("SRCFILE");
    let mbr = match parms.name("SRCMBR").as_str() {
        "" | "*PGM" | "*MODULE" | "*OBJ" => (name != "*CTLSPEC").then(|| name.clone()),
        m => Some(m.to_string()),
    };
    let tgtrls = named(parms, "TGTRLS", "*CURRENT");
    if !release_ok(&tgtrls) {
        return Err(format!(
            "CPF0006 TGTRLS({tgtrls}) is not a release this compiler targets"
        ));
    }
    let genlvl = parms
        .int("GENLVL")
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(10);
    let stgmdl = named(parms, "STGMDL", "*SNGLVL");
    let actgrp = match named(parms, "ACTGRP", "*STGMDL").as_str() {
        "*STGMDL" if stgmdl == "*TERASPACE" => "QILETS".to_string(),
        "*STGMDL" => "QILE".to_string(),
        a => a.to_string(),
    };
    Ok(Request {
        command,
        shape,
        sql,
        name,
        lib: if lib.is_empty() {
            "*CURLIB".into()
        } else {
            lib.to_ascii_uppercase()
        },
        srcfile: if sf.is_empty() {
            "QRPGLESRC".into()
        } else {
            sf.to_ascii_uppercase()
        },
        srclib: sfl.to_ascii_uppercase(),
        mbr,
        text: if parms.specified("TEXT") {
            parms.text("TEXT")
        } else {
            "*SRCMBRTXT".into()
        },
        genlvl,
        option: words(parms, "OPTION"),
        dbgview: named(parms, "DBGVIEW", if sql { "*NONE" } else { "*STMT" }),
        print: named(parms, "OUTPUT", if sql { "*NONE" } else { "*PRINT" }) == "*PRINT",
        optimize: named(parms, "OPTIMIZE", "*NONE"),
        indent: {
            let i = parms.text("INDENT");
            if i.is_empty() { "*NONE".into() } else { i }
        },
        truncnbr: !parms.specified("TRUNCNBR") || parms.yes("TRUNCNBR"),
        tgtrls,
        alwnull: named(parms, "ALWNULL", "*NO"),
        // CRTSQLRPGI binds as CRTPGM does, never into the default group.
        dftactgrp: cmdname == "CRTBNDRPG"
            && (!parms.specified("DFTACTGRP") || parms.yes("DFTACTGRP")),
        actgrp,
        bnddir: parms
            .list("BNDDIR")
            .into_iter()
            .map(qualified)
            .filter(|b| b != "*NONE")
            .collect(),
        define: words(parms, "DEFINE"),
        usrprf_owner: parms.name("USRPRF") == "*OWNER",
        replace: !parms.specified("REPLACE") || parms.yes("REPLACE"),
        sqlopts: sql.then(|| sql_options(parms, genlvl)),
    })
}

/// `CRTSQLRPGI`'s precompile options: `COMMIT`, `CLOSQLCSR`, the date
/// and time formats, and the naming in `OPTION` (`*SYS` or `*SQL`).
fn sql_options(parms: &Parms, genlvl: u8) -> rpgc::sqlpre::Options {
    let dft = rpgc::sqlpre::Options::default();
    let naming = if words(parms, "OPTION").iter().any(|w| w == "*SQL") { "*SQL" } else { "*SYS" };
    rpgc::sqlpre::Options {
        commit: named(parms, "COMMIT", &dft.commit),
        naming: naming.into(),
        datfmt: named(parms, "DATFMT", &dft.datfmt),
        datsep: named(parms, "DATSEP", &dft.datsep),
        timfmt: named(parms, "TIMFMT", &dft.timfmt),
        timsep: named(parms, "TIMSEP", &dft.timsep),
        closqlcsr: named(parms, "CLOSQLCSR", &dft.closqlcsr),
        genlvl,
        other: Vec::new(),
    }
}

impl Request {
    /// The object's name before the compile: the member's for
    /// `*CTLSPEC` until the control specification says otherwise.
    #[must_use]
    pub fn object_name(&self, src: &Source) -> String {
        if self.name == "*CTLSPEC" {
            src.member.name.clone()
        } else {
            self.name.clone()
        }
    }

    /// `*CTLSPEC`: the `DFTNAME` of the compiled source, when it has one.
    #[must_use]
    pub fn ctlspec_name(&self, c: &Compiled) -> Option<String> {
        if self.name != "*CTLSPEC" {
            return None;
        }
        let k = c.module.ctl.iter().find(|k| k.name == "DFTNAME")?;
        rpgc::decl::word_arg(k, 0).map(|n| n.to_ascii_uppercase())
    }

    /// The compiler's options.
    #[must_use]
    pub fn options(&self, name: &str, lib: &str, src: &Source) -> Options {
        Options {
            command: self.command,
            name: name.to_string(),
            lib: lib.to_string(),
            src: (
                src.member.lib.clone(),
                src.member.file.clone(),
                src.member.name.clone(),
            ),
            text: if self.text == "*SRCMBRTXT" {
                src.text.clone()
            } else {
                self.text.clone()
            },
            genlvl: self.genlvl,
            option: self.option.clone(),
            dbgview: self.dbgview.clone(),
            output: if self.print { "*PRINT" } else { "*NONE" }.into(),
            optimize: self.optimize.clone(),
            indent: self.indent.clone(),
            truncnbr: self.truncnbr,
            tgtrls: self.tgtrls.clone(),
            alwnull: self.alwnull.clone(),
            dftactgrp: self.dftactgrp,
            actgrp: self.actgrp.clone(),
            bnddir: self.bnddir.clone(),
            define: self.define.clone(),
            usrprf: if self.usrprf_owner { "*OWNER" } else { "*USER" }.into(),
            replace: self.replace,
            sql: self.sqlopts.clone(),
        }
    }
}
