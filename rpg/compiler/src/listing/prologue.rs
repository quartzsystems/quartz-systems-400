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
    row(out, ' ', 2, "Sort sequence", "*HEX");
    row(out, ' ', 2, "Language identifier", "*JOBRUN");
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
    let define = if o.define.is_empty() { "*NONE".to_string() } else { o.define.join(" ") };
    row(out, ' ', 2, "Define condition names", &define);
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
    if v.is_empty() { "*NONE".into() } else { v.join(" ") }
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
    boxrow(out, ' ', "Optimization level", &o.optimize);
    boxrow(out, ' ', "Source listing indentation", &c.ctl.indent);
    boxrow(out, ' ', "Type conversion options", "*NONE");
    boxrow(out, ' ', "Sort sequence", "*HEX");
    boxrow(out, ' ', "Language identifier", "*JOBRUN");
    if !module {
        boxrow(out, ' ', "User profile", &o.usrprf);
    }
    boxrow(out, ' ', "Authority", "*LIBCRTAUT");
    boxrow(out, ' ', "Truncate numeric", if c.ctl.truncnbr { "*YES" } else { "*NO" });
    boxrow(out, ' ', "Fix numeric", "*NONE");
    boxrow(out, ' ', "Allow null values", &c.ctl.alwnull);
    if module || !c.ctl.dftactgrp {
        boxrow(out, ' ', "Storage model . . .", if module { "*INHERIT" } else { "*SNGLVL" });
        boxrow(out, ' ', "Binding directory from Command", &bnddir(&o.bnddir));
        let src: Vec<String> = c.ctl.bnddir.iter().filter(|b| !o.bnddir.contains(b)).cloned().collect();
        boxrow(out, ' ', "Binding directory from Source", &bnddir(&src));
    }
    if !module && !c.ctl.dftactgrp {
        boxrow(out, ' ', "Activation group", actgrp(&c.ctl.actgrp, o));
    }
    boxrow(out, ' ', "Enable performance collection", "*PEP");
    boxrow(out, ' ', "Profiling data", "*NOCOL");
    boxrow(out, ' ', "Generate program interface", "*NO");
    boxrow(out, ' ', "Require prototype for export", "*NO");
    out.line(' ', rule(8, 77));
}
