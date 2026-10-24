// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The first page, the command's options; and the box of the options
//! in effect that follows the control specifications.

use super::{Out, leader};
use crate::compile::Compiled;
use crate::ctl::{Command, Options};

/// The `OPTION` values in IBM's order: the default, then the other.
const OPTIONS: &[(&str, &str)] = &[
    ("*XREF", "*NOXREF"),
    ("*GEN", "*NOGEN"),
    ("*NOSECLVL", "*SECLVL"),
    ("*SHOWCPY", "*NOSHOWCPY"),
    ("*EXPDDS", "*NOEXPDDS"),
    ("*EXT", "*NOEXT"),
    ("*NOSHOWSKP", "*SHOWSKP"),
    ("*NOSRCSTMT", "*SRCSTMT"),
    ("*DEBUGIO", "*NODEBUGIO"),
    ("*UNREF", "*NOUNREF"),
    ("*NOEVENTF", "*EVENTF"),
];

/// The options in force, given ones replacing their defaults.
pub(crate) fn option_values(given: &[String]) -> Vec<&'static str> {
    OPTIONS
        .iter()
        .map(|(d, o)| if given.iter().any(|g| g.eq_ignore_ascii_case(o)) { *o } else { *d })
        .collect()
}

/// A prologue row: label at `start`, colon at 35, the value after.
fn row(out: &mut Out, cc: char, start: usize, label: &str, value: &str) {
    let l = leader(start, label, 35);
    if value.is_empty() {
        out.line(cc, l);
    } else {
        out.line(cc, format!("{l}{:pad$}{value}", "", pad = 3 + start - 2));
    }
}

pub(crate) fn prologue(out: &mut Out, o: &Options) {
    out.new_page(None);
    let module = o.command == Command::CrtRpgMod;
    row(out, '0', 2, "Command", o.command.name());
    row(out, ' ', 4, "Issued by", "QSECOFR");
    if module {
        row(out, '0', 2, "Module", &o.name);
    } else {
        row(out, ' ', 2, "Program", &o.name);
    }
    row(out, ' ', 4, "Library", &o.lib);
    row(out, ' ', 2, "Text 'description'", if o.text.is_empty() { "*SRCMBRTXT" } else { &o.text });
    row(out, '0', 2, "Source Member", &o.src.2);
    row(out, ' ', 2, "Source File", &o.src.1);
    row(out, ' ', 4, "Library", &o.src.0);
    row(out, ' ', 4, "CCSID", "37");
    row(out, ' ', 2, "Text 'description'", "");
    row(out, ' ', 2, "Last Change", "dd-dd-dd  tt:tt:tt");
    row(out, '0', 2, "Generation severity level", &o.genlvl.to_string());
    if !module {
        row(out, ' ', 2, "Default activation group", if o.dftactgrp { "*YES" } else { "*NO" });
    }
    let opts = option_values(&o.option);
    for (i, chunk) in opts.chunks(4).enumerate() {
        let vals: String = chunk.iter().fold(String::new(), |a, v| a + &format!("{v:<11}"));
        if i == 0 {
            row(out, ' ', 2, "Compiler options", vals.trim_end());
        } else {
            out.line(' ', format!("{:39}{}", "", vals.trim_end()));
        }
    }
    row(out, ' ', 2, "Debugging views", &o.dbgview);
    row(out, ' ', 2, "Debug encryption key", "*NONE");
    row(out, ' ', 2, "Output", &o.output);
    row(out, ' ', 2, "Optimization level", &o.optimize);
    row(out, ' ', 2, "Source listing indentation", &o.indent);
    row(out, ' ', 2, "Type conversion options", "*NONE");
    row(out, ' ', 2, "Sort sequence", if o.sql.is_some() || super::from_precompiler(o) { "*JOB" } else { "*HEX" });
    row(out, ' ', 2, "Language identifier", if o.sql.is_some() || super::from_precompiler(o) { "*JOB" } else { "*JOBRUN" });
    if module {
        row(out, ' ', 2, "Replace module", if o.replace { "*YES" } else { "*NO" });
    } else {
        row(out, ' ', 2, "Replace program", if o.replace { "*YES" } else { "*NO" });
        row(out, ' ', 2, "User profile", &o.usrprf);
    }
    row(out, ' ', 2, "Authority", "*LIBCRTAUT");
    row(out, ' ', 2, "Truncate numeric", if o.truncnbr { "*YES" } else { "*NO" });
    row(out, ' ', 2, "Fix numeric", "*NONE");
    row(out, ' ', 2, "Target release", &o.tgtrls);
    row(out, ' ', 2, "Allow null values", &o.alwnull);
    if module || !o.dftactgrp {
        row(out, ' ', 2, "Storage model . . .", if module { "*INHERIT" } else { "*SNGLVL" });
        row(out, ' ', 2, "Binding directory", &bnddir(&o.bnddir));
    }
    if !module && !o.dftactgrp {
        row(out, ' ', 2, "Activation group", actgrp(&o.actgrp, o));
    }
    // One condition name to a line.
    match o.define.split_first() {
        None => row(out, ' ', 2, "Define condition names", "*NONE"),
        Some((first, rest)) => {
            row(out, ' ', 2, "Define condition names", first);
            for d in rest {
                out.line(' ', format!("{:39}{d}", ""));
            }
        }
    }
    row(out, ' ', 2, "Enable performance collection", "*PEP");
    row(out, ' ', 2, "Profiling data", "*NOCOL");
    row(out, ' ', 2, "Licensed Internal Code options", "");
    row(out, ' ', 2, "Generate program interface", "*NO");
    row(out, ' ', 2, "Include directory", "");
    row(out, ' ', 2, "Preprocessor options", "*NONE");
    row(out, ' ', 2, "Require prototype for export", "*NO");
}

/// The activation group as the command names it: the default (our
/// `QILE`) is `*STGMDL`.
fn actgrp<'a>(g: &'a str, o: &Options) -> &'a str {
    if g == o.actgrp && g == "QILE" { "*STGMDL" } else { g }
}

fn bnddir(v: &[String]) -> String {
    if v.is_empty() {
        return "*NONE".into();
    }
    // In columns of 13.
    let mut s: String = v.iter().fold(String::new(), |a, d| a + &format!("{d:<13}"));
    s.truncate(s.trim_end().len());
    s
}

/// A row of the options box: the prologue's row inside `*` at 8 and 77.
fn boxrow(out: &mut Out, cc: char, label: &str, value: &str) {
    let inner = if label.is_empty() {
        format!("{:39}{value}", "")
    } else if value.is_empty() {
        leader(2, label, 35)
    } else {
        format!("{}   {value}", leader(2, label, 35))
    };
    out.line(cc, framed(8, &inner, 77));
}

/// `*text*`: the box's edges at columns `at` and `end`.
pub(crate) fn framed(at: usize, inner: &str, end: usize) -> String {
    format!("{:at$}*{inner:<w$}*", "", w = end - at - 1)
}

/// A rule across a box.
pub(crate) fn rule(at: usize, end: usize) -> String {
    format!("{:at$}*{}*", "", "-".repeat(end - at - 1))
}

/// The options in effect, after the control specifications.
pub(crate) fn options_box(out: &mut Out, c: &Compiled, o: &Options) {
    let module = o.command == Command::CrtRpgMod;
    out.line(' ', rule(8, 77));
    out.line(' ', framed(8, " Compiler Options in Effect:", 77));
    out.line(' ', rule(8, 77));
    boxrow(out, ' ', "Text 'description'", &c.ctl.text);
    boxrow(out, '0', "Generation severity level", &c.ctl.genlvl.to_string());
    if !module {
        boxrow(out, ' ', "Default activation group", if c.ctl.dftactgrp { "*YES" } else { "*NO" });
    }
    let opts = option_values(&c.ctl.option);
    for (i, chunk) in opts.chunks(2).enumerate() {
        let vals: String = chunk.iter().fold(String::new(), |a, v| a + &format!("{v:<11}"));
        boxrow(out, ' ', if i == 0 { "Compiler options" } else { "" }, vals.trim_end());
    }
    // The control keywords' values where given.
    let given = |k: &str, quoted: bool, d: &str| ctl_args(c, k, quoted).map_or_else(|| d.to_string(), |v| columns(&v));
    let (srtseq, langid) = if o.sql.is_some() || super::from_precompiler(o) { ("*JOB", "*JOB") } else { ("*HEX", "*JOBRUN") };
    boxrow(out, ' ', "Optimization level", &given("OPTIMIZE", false, &o.optimize));
    boxrow(out, ' ', "Source listing indentation", &given("INDENT", true, &c.ctl.indent));
    boxrow(out, ' ', "Type conversion options", "*NONE");
    boxrow(out, ' ', "Sort sequence", &given("SRTSEQ", false, srtseq));
    boxrow(out, ' ', "Language identifier", &given("LANGID", false, langid));
    if !module {
        boxrow(out, ' ', "User profile", &o.usrprf);
    }
    boxrow(out, ' ', "Authority", &given("AUT", false, "*LIBCRTAUT"));
    boxrow(out, ' ', "Truncate numeric", if c.ctl.truncnbr { "*YES" } else { "*NO" });
    // FIXNBR's values in columns of 12.
    let fixnbr = ctl_args(c, "FIXNBR", false).map_or_else(|| "*NONE".to_string(), |v| v.iter().fold(String::new(), |a, x| a + &format!("{x:<12}")).trim_end().to_string());
    boxrow(out, ' ', "Fix numeric", &fixnbr);
    boxrow(out, ' ', "Allow null values", &c.ctl.alwnull);
    if module || !c.ctl.dftactgrp {
        boxrow(out, ' ', "Storage model . . .", if module { "*INHERIT" } else { "*SNGLVL" });
        boxrow(out, ' ', "Binding directory from Command", &bnddir(&o.bnddir));
        let src: Vec<String> = c.ctl.bnddir.iter().filter(|b| !o.bnddir.contains(b)).cloned().collect();
        let names: Vec<String> = src.iter().map(|b| b.rsplit('/').next().unwrap_or(b).to_string()).collect();
        boxrow(out, ' ', "Binding directory from Source", &bnddir(&names));
        // Their libraries, under them.
        if !src.is_empty() {
            let libs: Vec<String> = src.iter().map(|b| b.rsplit_once('/').map_or("*LIBL", |(l, _)| l).to_string()).collect();
            boxrow(out, ' ', "  Library", &format!("  {}", bnddir(&libs)));
        }
    }
    if !module && !c.ctl.dftactgrp {
        boxrow(out, ' ', "Activation group", actgrp(&c.ctl.actgrp, o));
    }
    boxrow(out, ' ', "Enable performance collection", &given("ENBPFRCOL", false, "*PEP"));
    boxrow(out, ' ', "Profiling data", &given("PRFDTA", false, "*NOCOL"));
    match ctl_args(c, "PGMINFO", false) {
        Some(v) if v.first().is_some_and(|g| g != "*NO") => {
            out.line(' ', framed(8, "  Program interface information:", 77));
            boxrow(out, ' ', "  Generate", &v[0]);
            boxrow(out, ' ', "  Location", v.get(1).map_or("*MODULE", String::as_str));
        }
        _ => boxrow(out, ' ', "Generate program interface", "*NO"),
    }
    boxrow(out, ' ', "Require prototype for export", &given("REQPREXP", false, "*NO"));
    out.line(' ', rule(8, 77));
}

/// A control keyword's arguments as the options box shows them: words
/// in upper case, literals as written (`quoted` keeps the quotes).
fn ctl_args(c: &Compiled, name: &str, quoted: bool) -> Option<Vec<String>> {
    let k = c.module.ctl.iter().find(|k| k.name == name)?;
    Some(
        k.args
            .iter()
            .map(|a| {
                a.toks
                    .iter()
                    .map(|t| match &t.kind {
                        rpgfront::lex::Kind::Str(s) if !quoted => s.clone(),
                        rpgfront::lex::Kind::Str(_) => t.text(),
                        _ => t.text().to_ascii_uppercase(),
                    })
                    .collect::<String>()
            })
            .collect(),
    )
}

/// Values in columns of 11.
fn columns(v: &[String]) -> String {
    v.iter().fold(String::new(), |a, x| a + &format!("{x:<11}")).trim_end().to_string()
}
