// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Command help: one panel group per `HLPPNLGRP`, with module `HLPID`
//! for the command and `HLPID/KWD` for each parameter, as the prompter
//! (F1, F2) and the Knowledge Center read them. The structure comes
//! from the definition (type, length, range, choices, default, whether
//! a value is required); the words from `command/sys-cpps/help/`, and
//! where nothing is written there, from the prompt texts.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use cmd::def::{CmdDef, Parm, ParmType, PmtCtlRef, RelOp};

use super::dict::Dict;
use super::uimtext::{esc, name_attr, paras, tagged};
use crate::Result;

/// Where the generated groups go.
pub const OUT_DIR: &str = "ui/uim/pnlgrp";

/// The group a definition without `HLPPNLGRP` gets.
pub const FALLBACK_GROUP: &str = "QHSYCMD";

fn lc_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        // Keep acronyms and names (`TCP/IP`, `QSYSOPR`) as they are.
        Some(_) if c.clone().next().is_some_and(char::is_uppercase) => s.to_string(),
        Some(f) => f.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

/// The command's first comment sentence, as the definition's author
/// wrote it after `NAME:`.
fn comment_sentence(source: &str, name: &str) -> Option<String> {
    let mut text = String::new();
    for line in source.lines().skip_while(|l| !l.contains(&format!("{name}:"))) {
        let l = line.trim();
        let Some(inner) = l.strip_prefix("/*") else {
            break;
        };
        let inner = inner.trim_end_matches("*/").trim();
        text.push(' ');
        text.push_str(inner);
        if !l.ends_with("*/") {
            break;
        }
        if inner.contains(". ") || inner.ends_with('.') {
            break;
        }
    }
    let after = text.split_once(':')?.1.trim();
    let end = after
        .match_indices(". ")
        .map(|(i, _)| i)
        .next()
        .unwrap_or_else(|| after.trim_end_matches('.').len());
    let s = after[..end].trim().trim_end_matches('.');
    (!s.is_empty()).then(|| s.to_string())
}

/// What the command does, for its module.
fn about(def: &CmdDef, source: &str, dict: &Dict) -> String {
    if let Some(t) = dict.command(&def.name, "about") {
        return t.to_string();
    }
    let lead = format!("The {} ({}) command", def.prompt, def.name);
    match comment_sentence(source, &def.name) {
        Some(s) => {
            let first = s.split_whitespace().next().unwrap_or_default();
            if matches!(first, "the" | "a" | "an") {
                format!("{lead} works with {s}.")
            } else {
                format!("Use the {} ({}) command to {s}.", def.prompt, def.name)
            }
        }
        None => format!("{lead}: {}.", lc_first(&def.prompt)),
    }
}

/// Whether the parameter takes only `*YES` and `*NO`.
fn yes_no(p: &Parm) -> bool {
    let mut v: Vec<&str> = p
        .values
        .iter()
        .map(String::as_str)
        .chain(p.spcval.iter().map(|s| s.from.as_str()))
        .chain(p.sngval.iter().map(|s| s.from.as_str()))
        .collect();
    v.sort_unstable();
    v.dedup();
    v == ["*NO", "*YES"]
}

/// The parameter's opening paragraph.
fn opening(def: &CmdDef, p: &Parm, dict: &Dict) -> String {
    if let Some(t) = dict.command(&def.name, &p.kwd) {
        return t.to_string();
    }
    if let Some(t) = dict.keywords.get(&p.kwd) {
        return t.to_string();
    }
    let prompt = p.prompt_text();
    if yes_no(p) {
        return format!("Whether to {}.", lc_first(prompt));
    }
    format!("The {} for this command.", lc_first(prompt))
}

/// A sentence for the kind of value a field takes.
fn type_sentence(p: &Parm) -> Option<(String, String)> {
    let l = p.length();
    let n = l.len;
    Some(match &p.ty {
        ParmType::Char => (
            "character-value".into(),
            format!("Type up to {n} characters."),
        ),
        ParmType::Name | ParmType::SName | ParmType::CName => (
            "name".into(),
            format!("Type a name of up to {n} characters."),
        ),
        ParmType::Generic => (
            "generic-name".into(),
            format!(
                "Type a name of up to {n} characters, or a generic name: characters that end with an asterisk (*) select every name that starts with them."
            ),
        ),
        ParmType::Dec if l.dec > 0 => (
            "number".into(),
            format!("Type a number of up to {n} digits, {} of them after the decimal point.", l.dec),
        ),
        ParmType::Dec => ("number".into(), format!("Type a number of up to {n} digits.")),
        ParmType::Int2 | ParmType::Int4 | ParmType::Uint2 | ParmType::Uint4 => {
            ("number".into(), "Type a whole number.".into())
        }
        ParmType::Lgl => ("logical-value".into(), "Type 1 for on or 0 for off.".into()),
        ParmType::Date => ("date".into(), "Type a date in the date format of the job.".into()),
        ParmType::Time => ("time".into(), "Type a time of day: hours, minutes and seconds.".into()),
        ParmType::VarName => (
            "variable-name".into(),
            "Type the name of a CL variable, starting with &.".into(),
        ),
        ParmType::Cmd | ParmType::CmdStr => ("command".into(), "Type a command.".into()),
        ParmType::PName => (
            "path-name".into(),
            format!("Type a path name of up to {n} characters."),
        ),
        ParmType::X => (
            "hexadecimal-value".into(),
            "Type the value as characters or as hexadecimal digits in X'...'.".into(),
        ),
        ParmType::ZeroElem | ParmType::Null | ParmType::Elem { .. } | ParmType::Qual { .. } => {
            return None;
        }
    })
}

fn rel_text(op: RelOp) -> &'static str {
    match op {
        RelOp::Eq => "equal to",
        RelOp::Ne => "other than",
        RelOp::Gt => "greater than",
        RelOp::Ge => "at least",
        RelOp::Lt => "less than",
        RelOp::Le | RelOp::Ng => "at most",
        RelOp::Nl => "at least",
    }
}

/// Sentences on the range and restrictions of one field.
fn limits(p: &Parm) -> Vec<String> {
    let mut out = Vec::new();
    if let Some((lo, hi)) = &p.range {
        out.push(format!("The value must be from {lo} to {hi}."));
    }
    if let Some((op, v)) = &p.rel {
        out.push(format!("The value must be {} {v}.", rel_text(*op)));
    }
    out
}

/// The choices of one field as `:PARML.` entries, with the default
/// marked; values the texts do not explain are named in a sentence.
fn choices(def: &CmdDef, kwd: &str, p: &Parm, dict: &Dict, b: &mut String) {
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut unexplained: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let specials = p
        .sngval
        .iter()
        .chain(&p.spcval)
        .map(|s| s.from.clone())
        .chain(p.values.iter().filter(|v| v.starts_with('*')).cloned());
    for v in specials {
        if seen.contains(&v) {
            continue;
        }
        seen.push(v.clone());
        match dict.value(&def.name, kwd, &v) {
            Some(t) => entries.push((v, t.to_string())),
            None => unexplained.push(v),
        }
    }
    let plain: Vec<&String> = p.values.iter().filter(|v| !v.starts_with('*')).collect();
    let typed = if p.rstd { None } else { type_sentence(p) };
    if !entries.is_empty() || typed.is_some() {
        b.push_str(":PARML.\n");
        for (v, t) in &entries {
            let def_attr = if p.dft.as_deref() == Some(v.as_str()) { " DEF" } else { "" };
            let _ = writeln!(b, ":PT.:PK{def_attr}.{}:EPK.", esc(v));
            b.push_str(&tagged(":PD.", t));
        }
        if let Some((pv, t)) = &typed {
            let _ = writeln!(b, ":PT.:PV.{pv}:EPV.");
            let mut t = t.clone();
            for l in limits(p) {
                t.push(' ');
                t.push_str(&l);
            }
            b.push_str(&tagged(":PD.", &t));
        }
        b.push_str(":EPARML.\n");
    } else {
        for l in limits(p) {
            b.push_str(&tagged(":P.", &l));
        }
    }
    if !unexplained.is_empty() {
        b.push_str(&tagged(
            ":P.",
            &format!("Also accepted: {}.", unexplained.join(", ")),
        ));
    }
    if !plain.is_empty() {
        let shown = if plain.len() > 16 {
            "one of the values F4 lists".to_string()
        } else {
            plain.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        };
        let lead = if p.rstd { "The value must be" } else { "Values often used:" };
        b.push_str(&tagged(":P.", &format!("{lead} {shown}.")));
    }
    if let Some(d) = &p.dft
        && !seen.contains(d)
        && !d.trim().is_empty()
    {
        b.push_str(&tagged(":P.", &format!("When no value is given, {d} is used.")));
    }
}

/// The parts of an element list or a qualified name.
fn parts(def: &CmdDef, p: &Parm, dict: &Dict, b: &mut String) {
    match &p.ty {
        ParmType::Qual { .. } => {
            let Some((first, rest)) = p.items.split_first() else {
                return;
            };
            choices(def, &p.kwd, first, dict, b);
            for q in rest {
                let what = q.prompt.as_ref().map_or("Library", |(t, _)| t.as_str());
                b.push_str(&tagged(
                    ":P.",
                    &format!("{what}: the name can be qualified by it."),
                ));
                choices(def, &p.kwd, q, dict, b);
            }
        }
        ParmType::Elem { .. } => {
            b.push_str(&tagged(
                ":P.",
                &format!("The value has {} parts, given in order:", p.items.len()),
            ));
            for (i, e) in p.items.iter().enumerate() {
                let what = e.prompt.as_ref().map_or(String::new(), |(t, _)| t.clone());
                b.push_str(&tagged(":H4.", &format!("Part {}: {what}", i + 1)));
                if matches!(e.ty, ParmType::Elem { .. } | ParmType::Qual { .. }) {
                    parts(def, e, dict, b);
                } else {
                    choices(def, &p.kwd, e, dict, b);
                }
            }
        }
        _ => choices(def, &p.kwd, p, dict, b),
    }
}

/// One parameter's module.
fn parm_module(def: &CmdDef, id: &str, p: &Parm, dict: &Dict) -> String {
    let mut b = String::new();
    let _ = writeln!(
        b,
        ":HELP NAME={}.{} ({})",
        name_attr(&format!("{id}/{}", p.kwd)),
        esc(p.prompt_text()),
        p.kwd
    );
    b.push_str(&paras(&opening(def, p, dict)));
    if p.required() {
        b.push_str(":P.A value is required.\n");
    }
    if p.is_list() {
        b.push_str(&tagged(":P.", &format!("Up to {} values can be given.", p.max)));
    }
    if matches!(p.pmtctl, Some(PmtCtlRef::PmtRqs)) {
        b.push_str(&tagged(
            ":P.",
            "The prompt shows this parameter after F10 (Additional parameters).",
        ));
    }
    let single: Vec<(String, String)> = p
        .sngval
        .iter()
        .map(|s| s.from.clone())
        .filter_map(|v| dict.value(&def.name, &p.kwd, &v).map(|t| (v, t.to_string())))
        .collect();
    let mut p2 = p.clone();
    if matches!(p.ty, ParmType::Elem { .. } | ParmType::Qual { .. }) && !p.sngval.is_empty() {
        // Single values replace the whole list or name.
        b.push_str(":PARML.\n");
        for (v, t) in &single {
            let def_attr = if p.dft.as_deref() == Some(v.as_str()) { " DEF" } else { "" };
            let _ = writeln!(b, ":PT.:PK{def_attr}.{}:EPK.", esc(v));
            b.push_str(&tagged(":PD.", t));
        }
        b.push_str(":EPARML.\n");
        let rest: Vec<String> = p
            .sngval
            .iter()
            .map(|s| s.from.clone())
            .filter(|v| !single.iter().any(|(s, _)| s == v))
            .collect();
        if !rest.is_empty() {
            b.push_str(&tagged(":P.", &format!("Also accepted: {}.", rest.join(", "))));
        }
        p2.sngval.clear();
    }
    parts(def, &p2, dict, &mut b);
    b.push_str(":EHELP.\n");
    b
}

/// The command's module.
fn command_module(def: &CmdDef, id: &str, source: &str, dict: &Dict) -> String {
    let mut b = String::new();
    let _ = writeln!(b, ":HELP NAME={}.{} ({})", name_attr(id), esc(&def.prompt), def.name);
    b.push_str(&paras(&about(def, source, dict)));
    if let Some(t) = dict.command(&def.name, "notes") {
        b.push_str(&paras(t));
    }
    if def.parms.iter().all(|p| p.constant.is_some()) {
        b.push_str(":P.The command has no parameters.\n");
    }
    b.push_str(":EHELP.\n");
    b
}

/// The help group of a definition.
#[must_use]
pub fn group_of(def: &CmdDef) -> String {
    def.hlppnlgrp
        .as_ref()
        .map_or_else(|| FALLBACK_GROUP.to_string(), |q| q.name.to_ascii_uppercase())
}

/// Every command help group: name and UIM source.
pub fn generate(root: &Path) -> Result<BTreeMap<String, String>> {
    let dict = Dict::load(root)?;
    let mut by_group: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut done: BTreeMap<(String, String), ()> = BTreeMap::new();
    for (def, source) in crate::commands::definitions(root)? {
        let group = group_of(&def);
        let id = def.hlpid.clone().unwrap_or_else(|| def.name.clone());
        if done.insert((group.clone(), id.clone()), ()).is_some() {
            continue;
        }
        let mut m = command_module(&def, &id, &source, &dict);
        for p in crate::commands::prompted(&def) {
            m.push_str(&parm_module(&def, &id, p, &dict));
        }
        by_group.entry(group).or_default().push(m);
    }
    let mut out = BTreeMap::new();
    for (g, modules) in by_group {
        let mut s = format!(
            ".* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is\n\
             .* licensed under the terms of the MIT license which can be found in the\n\
             .* root directory of this project.\n\
             .*\n\
             .* {g}: command help. Generated by `cargo run -p helpaudit -- gen` from the\n\
             .* definitions and command/sys-cpps/help/; do not edit.\n\
             :PNLGRP.\n:COPYR.(C) Copyright Quartz Systems 2026.\n"
        );
        for m in modules {
            s.push_str(&m);
        }
        s.push_str(":EPNLGRP.\n");
        out.insert(g, s);
    }
    Ok(out)
}
