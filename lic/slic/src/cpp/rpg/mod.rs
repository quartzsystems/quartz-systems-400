// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CRTBNDRPG`, `CRTRPGMOD` and `CRTSQLRPGI` in the storage service:
//! the source member read with its sequence numbers and dates, `/COPY`
//! members and external descriptions found through the store, the
//! source compiled by `rpgc`, its listing spooled to `QSYSPRT`, the
//! module or program created, and the `RNS` messages sent.

mod object;
mod opts;
mod source;
#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::fmt::Write as _;

use cmd::block::Parms;
use ile::CreateData;
use stg::proto::{CmdMsg, CmdReply};

use super::direct::{in_store, lib_or};
use crate::ipl::system::{System, now_unix};

use object::{Made, Shape};
use opts::Request;
use source::Source;

/// Run one of the RPG commands.
pub fn run(system: &mut System, cmdname: &str, parms: &Parms) -> CmdReply {
    let req = match opts::request(cmdname, parms) {
        Ok(r) => r,
        Err(e) => return super::clpgm::failed(cmdname, &e),
    };
    let lib_name = match super::clpgm::target_library(system, &req.lib) {
        Ok(l) => l,
        Err(e) => return super::clpgm::failed(cmdname, &e),
    };
    let src = match &req.srcstmf {
        // A stream file, read in the job's name; its name is the
        // object's unless the command or the control specification
        // gives one.
        Some(path) => super::stmf::read_as_job(system, path).map(|text| Source {
            member: super::stmf::member_of(path, &text),
            text: String::new(),
        }),
        None => in_store(system, |txn, qsys, names| {
            let lib = lib_or(&req.srclib);
            source::read(txn, qsys, names, &req.srcfile, &lib, req.mbr.as_deref())
        }),
    };
    match src {
        Ok(src) => create(system, &req, &lib_name, &src),
        Err(e) => super::clpgm::failed(cmdname, &e),
    }
}

/// Compile the source, spool the listing, make the object.
fn create(system: &mut System, req: &Request, lib_name: &str, src: &Source) -> CmdReply {
    let mut options = req.options(&req.object_name(src), lib_name, src);
    let shape = req.shape;
    let compiled = compile(system, req, src, &mut options);
    let name = options.name.clone();
    let mut out = String::new();
    for d in &compiled.diags {
        let text = rpgc::msgs::text(d);
        let _ = writeln!(
            out,
            "{} {:02} line {}: {text}",
            d.msgid, d.severity, d.at.line
        );
    }
    if req.print {
        let listing = &compiled.listing;
        let mbr = if req.srcstmf.is_some() { &name } else { &src.member.name };
        let _ = match crate::svc::security::print_as(system, mbr, &name, listing, true) {
            Ok(w) => writeln!(out, "Listing: {w}"),
            Err(e) => writeln!(out, "The listing was not spooled: {e}"),
        };
    }
    if req.infostmf.is_some() {
        // MIGRATE(phase11): PGMINFO(*PCML) documents for INFOSTMF.
        let _ = writeln!(out, "INFOSTMF: no PCML is generated; the stream file is not written.");
    }
    let sev = format!("{:02}", compiled.max_severity);
    let refused = Refused {
        // A precompile that failed ends the command before the compile.
        id: if compiled.sql_failed { "SQL9001" } else { shape.not_created() },
        shape,
        name: &name,
        lib: lib_name,
        sev: Some(&sev),
    };
    if !compiled.created {
        return refused.reply(&out);
    }
    let made = Made {
        shape,
        name: name.clone(),
        text: options.text.clone(),
        replace: req.replace,
        usrprf_owner: req.usrprf_owner,
        bnddir: compiled.ctl.bnddir.clone(),
        source: source_view(&options.dbgview, &compiled),
    };
    // A stream file's program names no source member.
    let stmf = req.srcstmf.is_some();
    let create = CreateData {
        compiler: "RPGLE".into(),
        options: req.option.join(" "),
        src_file: if stmf { String::new() } else { src.member.file.clone() },
        src_lib: if stmf { String::new() } else { src.member.lib.clone() },
        src_mbr: if stmf { String::new() } else { src.member.name.clone() },
    };
    let stored = crate::svc::ile::commands::in_store(system, lib_name, |txn, c, lib_id| {
        object::make(txn, c, lib_id, &made, &compiled.qir, create, now_unix())
    });
    if let Err(e) = stored {
        return refused.reply(&format!("{out}{e}\n"));
    }
    let (date, time) = stamp(now_unix());
    let fields = [
        (name.as_str(), 10),
        (lib_name, 10),
        (&sev, 2),
        (&date, 8),
        (&time, 8),
    ];
    CmdReply {
        text: format!(
            "{out}{} {name} created in library {lib_name}. Highest severity: {sev}.\n",
            shape.word()
        ),
        messages: vec![CmdMsg {
            msgid: shape.created().into(),
            kind: 4,
            data: cmd::error::data(&fields),
        }],
        ..CmdReply::default()
    }
}

/// The compile, with `/COPY` members and file descriptions from the
/// store; `*CTLSPEC` takes the control specification's name, compiling
/// again under it.
fn compile(
    system: &mut System,
    req: &Request,
    src: &Source,
    options: &mut rpgc::Options,
) -> rpgc::Compiled {
    let cell = RefCell::new(system);
    let mut includes = source::StoreIncludes {
        system: &cell,
        incdir: req.incdir.clone(),
    };
    let mut files = source::StoreFiles { system: &cell };
    let c = rpgc::compile(src.member.clone(), &mut includes, &mut files, options);
    match req.ctlspec_name(&c) {
        Some(n) if n != options.name => {
            options.name = n;
            rpgc::compile(src.member.clone(), &mut includes, &mut files, options)
        }
        _ => c,
    }
}

/// An object not made: the escape `id` and what it names.
struct Refused<'a> {
    id: &'a str,
    shape: Shape,
    name: &'a str,
    lib: &'a str,
    /// The highest severity, for the diagnostic before the escape.
    sev: Option<&'a str>,
}

impl Refused<'_> {
    /// The reply: the compile's output, the diagnostic that gives the
    /// highest severity, the escape.
    fn reply(&self, out: &str) -> CmdReply {
        let messages = self
            .sev
            .map(|s| CmdMsg {
                msgid: "RNS9311".into(),
                kind: 5,
                data: cmd::error::data(&[(s, 2)]),
            })
            .into_iter()
            .collect();
        CmdReply {
            text: format!(
                "{out}{} {} {} was not created in library {}.\n",
                self.id,
                self.shape.word(),
                self.name,
                self.lib
            ),
            messages,
            escape: Some(CmdMsg {
                msgid: self.id.into(),
                kind: 7,
                data: cmd::error::data(&[(self.name, 10), (self.lib, 10)]),
            }),
            ..CmdReply::default()
        }
    }
}

/// A time as `yy/mm/dd` and `hh:mm:ss` (UTC).
fn stamp(unix: u64) -> (String, String) {
    let days = i64::try_from(unix / 86_400).unwrap_or(0);
    let secs = unix % 86_400;
    // The civil date of a day count from 1970-01-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        format!("{:02}/{m:02}/{d:02}", y % 100),
        format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60),
    )
}

impl Shape {
    /// The object's kind in words.
    fn word(self) -> &'static str {
        match self {
            Self::Module => "Module",
            Self::Program => "Program",
            Self::SrvPgm => "Service program",
        }
    }

    /// The completion message for an object made.
    fn created(self) -> &'static str {
        match self {
            Self::Module => "RNS9305",
            Self::Program | Self::SrvPgm => "RNS9304",
        }
    }

    /// The escape for an object not made.
    fn not_created(self) -> &'static str {
        match self {
            Self::Module => "RNS9309",
            Self::Program | Self::SrvPgm => "RNS9310",
        }
    }
}

/// The source view a debug view keeps: the member's own lines
/// (`*SOURCE`), with its `/COPY` members expanded (`*COPY`, `*ALL`), or
/// the compile listing (`*LIST`).
fn source_view(dbgview: &str, c: &rpgc::Compiled) -> Option<Vec<String>> {
    match dbgview {
        "*SOURCE" => Some(c.read.lines.iter().filter(|l| l.member == 0).map(|l| l.text.clone()).collect()),
        "*COPY" | "*ALL" => Some(c.read.lines.iter().map(|l| l.text.clone()).collect()),
        "*LIST" => Some(c.listing.clone()),
        _ => None,
    }
}
