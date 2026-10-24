// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What externally described files declare: each record format (renamed
//! by `RENAME`, left out by `IGNORE`/`INCLUDE`), its fields as program
//! fields (`PREFIX`, I-spec renames; shared by name between files and
//! with `EXTNAME` subfields), the fields of program-described input
//! specifications, and the subfields of `EXTNAME` and `LIKEREC`
//! structures.

use rpgfront::Diag;
use rpgfront::ast::{Def, Expr, Input, Step};

use crate::decl::{Cx, int_arg, str_arg, word_arg};
use crate::diag::ids;
use crate::files::{ExtFormat, File};
use crate::names::mangle;
use crate::syms::{Kind, Storage, Sym, Table};
use crate::types::Ty;

/// A record format of a declared file.
#[derive(Clone, Debug, Default)]
pub struct RecFmt {
    /// The file's index in the module's files.
    pub file: usize,
    /// The file's name.
    pub file_name: String,
    /// The name the program uses (after `RENAME`).
    pub name: String,
    /// The description.
    pub fmt: ExtFormat,
    /// The file is `QUALIFIED`: no fields of its own.
    pub qualified: bool,
    /// The program name of each field, in format order.
    pub names: Vec<String>,
}

/// `PREFIX(text:n)` applied to a name.
#[must_use]
pub fn prefixed(name: &str, prefix: Option<&(String, u32)>) -> String {
    match prefix {
        Some((p, n)) => {
            let p = p.trim_matches('\'');
            let rest: String = name.chars().skip(*n as usize).collect();
            format!("{p}{rest}").to_ascii_uppercase()
        }
        None => name.to_string(),
    }
}

/// The formats a file keeps: `INCLUDE` names the kept, `IGNORE` the
/// dropped.
pub fn kept(f: &File, fmt: &str) -> bool {
    if !f.include.is_empty() {
        return f.include.iter().any(|i| i == fmt);
    }
    !f.ignore.iter().any(|i| i == fmt)
}

/// The type a file's field has in the program: zoned and binary
/// numbers are packed there, as IBM brings them in.
fn program_type(t: &Ty) -> Ty {
    match t {
        Ty::Zoned(d, s) | Ty::Bin(d, s) => Ty::Packed(*d, *s),
        other => other.clone(),
    }
}

/// Declare the record formats and fields of a scope's files, and the
/// fields of its program-described input specifications.
pub fn declare_files(t: &mut Table, diags: &mut Vec<Diag>, scope: usize, prefix: &str, files: &[File], first: usize, inputs: &[Input]) {
    for (fi, f) in files.iter().enumerate().skip(first) {
        let Some(ext) = &f.ext else { continue };
        if f.template {
            continue;
        }
        for fm in &ext.formats {
            if !kept(f, &fm.name) {
                continue;
            }
            let name = f
                .renames
                .iter()
                .find(|(e, _)| *e == fm.name)
                .map_or_else(|| fm.name.clone(), |(_, n)| n.clone());
            let mut rf = RecFmt {
                file: fi,
                file_name: f.name.clone(),
                name: name.clone(),
                fmt: fm.clone(),
                qualified: f.qualified,
                names: Vec::new(),
            };
            let idx = t.recfmts.len();
            for fd in &fm.fields {
                let mut pname = if f.alias && !fd.alias.is_empty() {
                    fd.alias.to_ascii_uppercase()
                } else {
                    prefixed(&fd.name, f.prefix.as_ref())
                };
                // An I spec of the format renames the field.
                for i in inputs.iter().filter(|i| i.external && (i.name == name || i.name == fm.name)) {
                    if let Some(x) = i.fields.iter().find(|x| x.extname == fd.name && !x.name.is_empty()) {
                        pname.clone_from(&x.name);
                    }
                }
                rf.names.push(pname.clone());
                if f.qualified || t.find_in(scope, &pname).is_some() {
                    continue;
                }
                let _ = t.add(Sym {
                    name: pname.clone(),
                    kind: Some(Kind::Field),
                    ty: program_type(&fd.ty),
                    ir: format!("${prefix}{}", mangle(&pname)),
                    scope,
                    storage: if scope == 0 { Storage::Static } else { Storage::Auto },
                    def_line: f.def_line,
                    implicit: true,
                    // BLOCKED(phase9): the record's null map does not reach
                    // the program, so the indicator stays off.
                    nullind: fd.nullable && t.alwnull_usrctl,
                    ..Sym::default()
                });
            }
            t.recfmts.push(rf);
            if !f.qualified
                && t
                    .add(Sym {
                        name: name.clone(),
                        kind: Some(Kind::Format(idx)),
                        scope,
                        def_line: f.def_line,
                        ..Sym::default()
                    })
                    .is_err()
            {
                diags.push(Diag::new(ids::DUPLICATE, 30, rpgfront::ast::Pos { line: f.def_line, col: 7 }).arg(name));
            }
        }
    }
    // Program-described input fields.
    for i in inputs.iter().filter(|i| !i.external) {
        let reclen = files.iter().find(|f| f.name == i.name).map_or(0, |f| f.reclen);
        for x in &i.fields {
            // A field past the end of the record is ignored.
            if reclen > 0 && x.to.trim().parse::<u32>().is_ok_and(|to| to > reclen) {
                diags.push(Diag::new(ids::PAST_RECORD, 30, x.at));
                if !x.name.is_empty() {
                    diags.push(Diag::new(ids::NOT_DEFINED, 30, x.at).arg(x.name.clone()));
                }
                continue;
            }
            if x.name.is_empty() || x.name.starts_with('*') || t.find_in(scope, &x.name).is_some() {
                continue;
            }
            let Some(ty) = input_type(x) else { continue };
            let _ = t.add(Sym {
                name: x.name.clone(),
                kind: Some(Kind::Field),
                ty,
                ir: format!("${prefix}{}", mangle(&x.name)),
                scope,
                def_line: x.at.line,
                implicit: true,
                ..Sym::default()
            });
        }
    }
}

/// The type of a program-described input field from its positions,
/// data format and decimals.
#[must_use]
pub fn input_type(x: &rpgfront::ast::InputField) -> Option<Ty> {
    let from: u32 = x.from.trim().parse().ok()?;
    let to: u32 = x.to.trim().parse().ok()?;
    let n = to.checked_sub(from)? + 1;
    let dec: Option<u8> = x.decimals.trim().parse().ok();
    let digits = |d: u32| u8::try_from(d.min(63)).unwrap_or(63);
    Some(match x.format.as_str() {
        "P" => Ty::Packed(digits(2 * n - 1), dec.unwrap_or(0)),
        "B" => Ty::Bin(if n <= 2 { 4 } else { 9 }, dec.unwrap_or(0)),
        "I" => Ty::Int(match n {
            1 => 3,
            2 => 5,
            8 => 20,
            _ => 10,
        }),
        "U" => Ty::Uns(match n {
            1 => 3,
            2 => 5,
            8 => 20,
            _ => 10,
        }),
        "F" => Ty::Float(if n == 4 { 4 } else { 8 }),
        "S" => Ty::Zoned(digits(n), dec.unwrap_or(0)),
        _ => match dec {
            Some(d) => Ty::Zoned(digits(n), d),
            None => Ty::Char(n),
        },
    })
}

/// Subfields from a format: name, type, offset.
/// An external structure's subfields: name, type, offset, `DFT` value.
pub type ExtSubs = Vec<(String, Ty, u32, Option<String>)>;

/// Which fields of a format a structure takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// `*ALL`: every field at its buffer offset.
    All,
    /// `*INPUT`.
    Input,
    /// `*OUTPUT`.
    Output,
    /// `*KEY`: the key fields in key order.
    Key,
}

impl Which {
    fn parse(w: &str) -> Option<Self> {
        Some(match w {
            "*ALL" => Self::All,
            "*INPUT" => Self::Input,
            "*OUTPUT" => Self::Output,
            "*KEY" => Self::Key,
            _ => return None,
        })
    }
}

/// The subfields a format gives: name, type, offset; and the length.
#[must_use]
pub fn subfields(fm: &ExtFormat, names: Option<&[String]>, which: Which) -> (ExtSubs, u32) {
    let name_of = |i: usize| names.and_then(|n| n.get(i).cloned()).unwrap_or_else(|| fm.fields[i].name.clone());
    let mut out = Vec::new();
    match which {
        Which::All => {
            for (i, f) in fm.fields.iter().enumerate() {
                out.push((name_of(i), f.ty.clone(), f.offset, f.default.clone()));
            }
            (out, fm.len)
        }
        Which::Key => {
            let mut at = 0;
            for k in &fm.keys {
                if let Some(i) = fm.fields.iter().position(|f| f.name == *k) {
                    let f = &fm.fields[i];
                    out.push((name_of(i), f.ty.clone(), at, f.default.clone()));
                    at += field_len(fm, i);
                }
            }
            (out, at)
        }
        Which::Input | Which::Output => {
            let mut at = 0;
            for (i, f) in fm.fields.iter().enumerate() {
                let ok = match which {
                    Which::Input => matches!(f.usage, 'I' | 'B'),
                    _ => matches!(f.usage, 'O' | 'B'),
                };
                if ok {
                    out.push((name_of(i), f.ty.clone(), at, f.default.clone()));
                    at += field_len(fm, i);
                }
            }
            (out, at)
        }
    }
}

/// A field's bytes in the buffer: to the next field, or the record end.
#[must_use]
pub fn field_len(fm: &ExtFormat, i: usize) -> u32 {
    let start = fm.fields[i].offset;
    let end = fm.fields.get(i + 1).map_or(fm.len, |f| f.offset);
    end.saturating_sub(start)
}

/// The external fields an `EXTNAME` or `LIKEREC` structure takes, when
/// it is one: the subfields and the length.
pub fn ds_subfields(cx: &mut Cx<'_>, d: &Def) -> Option<(ExtSubs, u32)> {
    if let Some(k) = d.kw("LIKEREC") {
        // LIKEREC(fmt|file.fmt[:*ALL|*INPUT|*OUTPUT|*KEY]).
        let (file, fmt) = match k.arg(0).and_then(|a| a.expr.clone()) {
            Some(Expr::Ref { name, steps, .. }) => match steps.first() {
                Some(Step::Field(f)) => (Some(name), f.clone()),
                _ => (None, name),
            },
            _ => (None, word_arg(k, 0).unwrap_or_default()),
        };
        let which = word_arg(k, 1).and_then(|w| Which::parse(&w)).unwrap_or(Which::All);
        let found = cx
            .t
            .recfmts
            .iter()
            .find(|r| r.name == fmt && file.as_ref().is_none_or(|f| *f == r.file_name) && (file.is_some() || !r.qualified))
            .cloned();
        let Some(r) = found else {
            cx.diags.push(Diag::new(ids::NOT_DEFINED, 30, k.at).arg(fmt));
            return Some((Vec::new(), 1));
        };
        return Some(subfields(&r.fmt, Some(&r.names), which));
    }
    let ext_col = d.fixed.as_ref().is_some_and(|c| c.ext == "E");
    let k = d.kw("EXTNAME");
    if k.is_none() && !ext_col {
        return None;
    }
    let (file, fmt, which) = match k {
        Some(k) => {
            let file = str_arg(k, 0).or_else(|| word_arg(k, 0)).unwrap_or_default().to_ascii_uppercase();
            let mut fmt = None;
            let mut which = Which::Input;
            for i in 1..k.args.len() {
                let w = str_arg(k, i).or_else(|| word_arg(k, i)).unwrap_or_default().to_ascii_uppercase();
                match Which::parse(&w) {
                    Some(x) => which = x,
                    None => fmt = Some(w),
                }
            }
            (file, fmt, which)
        }
        None => (d.name.to_ascii_uppercase(), None, Which::Input),
    };
    let (lib, name) = match file.split_once('/') {
        Some((l, n)) => (l.to_string(), n.to_string()),
        None => ("*LIBL".to_string(), file.clone()),
    };
    let Some(desc) = cx.t.extnames.get(&format!("{lib}/{name}")).cloned() else {
        cx.diags.push(Diag::new(ids::EXT_NOT_FOUND, 40, d.at).arg(name));
        cx.diags.push(Diag::new(ids::DS_NO_SUBFIELDS, 0, d.at));
        return Some((Vec::new(), 1));
    };
    let fm = match &fmt {
        Some(f) => desc.formats.iter().find(|x| x.name == *f),
        None => desc.formats.first(),
    };
    let Some(fm) = fm else {
        cx.diags.push(Diag::new(ids::NOT_DEFINED, 30, d.at).arg(fmt.unwrap_or_default()));
        return Some((Vec::new(), 1));
    };
    let (mut subs, mut len) = subfields(fm, None, which);
    let fm = fm.clone();
    // EXTBININT: binary fields without decimals are integers.
    if cx.t.extbinint {
        for s in &mut subs {
            if let Ty::Bin(dg, 0) = s.1 {
                s.1 = Ty::Int(if dg <= 4 { 5 } else { 10 });
            }
        }
    }
    // ALIAS: the fields' alternative names.
    if d.kw("ALIAS").is_some() {
        for s in &mut subs {
            if let Some(f) = fm.fields.iter().find(|f| f.name == s.0 && !f.alias.is_empty()) {
                s.0 = f.alias.to_ascii_uppercase();
            }
        }
    }
    // EXTFLD renames a field, EXTFMT changes its form; the subfields
    // after it move when its size changes.
    let mut resized = false;
    for c in &d.children {
        let Some(k) = c.kw("EXTFLD") else { continue };
        let from = str_arg(k, 0).or_else(|| word_arg(k, 0)).unwrap_or_else(|| c.name.clone()).to_ascii_uppercase();
        let Some(s) = subs.iter_mut().find(|s| s.0 == from) else {
            cx.diags.push(Diag::new(ids::EXTFLD_NOT_FOUND, 30, c.at).arg(from));
            continue;
        };
        s.0 = c.name.to_ascii_uppercase();
        if let Some(fk) = c.kw("EXTFMT") {
            let f = str_arg(fk, 0).unwrap_or_default().to_ascii_uppercase();
            // A packed or zoned field keeps its form (pub400).
            if matches!(s.1, Ty::Packed(..) | Ty::Zoned(..)) {
                let at = fk.arg(0).and_then(|a| a.toks.first()).map_or(fk.at, |t| t.at);
                cx.diags.push(Diag::new(ids::EXTFMT_PARM, 20, at).arg("EXTFMT"));
                continue;
            }
            let shape = s.1.shape();
            if let Some((dg, sc)) = shape {
                let nt = match f.as_str() {
                    "S" => Some(Ty::Zoned(dg, sc)),
                    "P" => Some(Ty::Packed(dg, sc)),
                    "B" => Some(Ty::Bin(dg, sc)),
                    _ => None,
                };
                if let Some(t) = nt {
                    s.1 = t;
                    resized = true;
                }
            }
        }
    }
    if resized && which != Which::All {
        let mut at = 0;
        for s in &mut subs {
            s.2 = at;
            at += cx.t.size(&s.1);
        }
        len = at;
    }
    // The structure's PREFIX renames its subfields.
    if let Some(p) = d.kw("PREFIX") {
        let text = p.arg(0).map(|a| a.toks.iter().map(rpgfront::lex::Tok::text).collect::<String>()).unwrap_or_default();
        let n = int_arg(p, 1).and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
        let pre = (text.to_ascii_uppercase(), n);
        for s in &mut subs {
            s.0 = prefixed(&s.0, Some(&pre));
        }
    }
    Some((subs, len))
}

/// The external descriptions the scope's `EXTNAME` structures name,
/// read before the definitions are declared.
pub fn read_extnames(t: &mut Table, defs: &[Def], ext: &mut dyn crate::files::ExtFiles) {
    for d in defs {
        let ext_col = d.fixed.as_ref().is_some_and(|c| c.ext == "E");
        let file = match d.kw("EXTNAME") {
            Some(k) => str_arg(k, 0).or_else(|| word_arg(k, 0)).unwrap_or_default().to_ascii_uppercase(),
            None if ext_col => d.name.to_ascii_uppercase(),
            None => continue,
        };
        let (lib, name) = match file.split_once('/') {
            Some((l, n)) => (l.to_string(), n.to_string()),
            None => ("*LIBL".to_string(), file.clone()),
        };
        let key = format!("{lib}/{name}");
        if t.extnames.contains_key(&key) {
            continue;
        }
        if let Some(desc) = ext.describe(&name, &lib) {
            t.extnames.insert(key, desc);
        }
    }
}

/// A subfield named by a special word (`*FILE`, `*STATUS`, ...): its
/// type and offset in a file or program status data structure.
#[must_use]
pub fn special_subfield(c: &Def, psds: bool) -> Option<(Ty, u32)> {
    let word = match &c.fixed {
        Some(cols) if cols.from.starts_with('*') => cols.from.to_ascii_uppercase(),
        _ => c.kws.iter().find(|k| k.name.starts_with('*') && k.args.is_empty())?.name.clone(),
    };
    let table = if psds { PSDS_WORDS } else { INFDS_WORDS };
    let &(_, off, len, digits) = table.iter().find(|(w, ..)| *w == word)?;
    let ty = if digits { Ty::Zoned(u8::try_from(len).unwrap_or(5), 0) } else { Ty::Char(len) };
    Some((ty, off))
}

/// The INFDS special words: word, offset from 0, length, zoned.
pub const INFDS_WORDS: &[(&str, u32, u32, bool)] = &[
    ("*FILE", 0, 8, false),
    ("*STATUS", 10, 5, true),
    ("*OPCODE", 15, 6, false),
    ("*ROUTINE", 21, 8, false),
    ("*RECORD", 37, 8, false),
    ("*SIZE", 66, 4, true),
    ("*INP", 70, 2, true),
    ("*OUT", 72, 2, true),
    ("*MODE", 74, 2, true),
];

/// The PSDS special words, as [`INFDS_WORDS`].
pub const PSDS_WORDS: &[(&str, u32, u32, bool)] = &[
    ("*PROC", 0, 10, false),
    ("*STATUS", 10, 5, true),
    ("*ROUTINE", 28, 8, false),
    ("*PARMS", 36, 3, true),
];

/// Result fields a fixed-form calculation defines with a length
/// (columns 64-68) and decimals: packed with decimals, character
/// without, unless the name is defined already.
pub fn declare_calc_fields(t: &mut Table, diags: &mut Vec<Diag>, scope: usize, prefix: &str, calcs: &[rpgfront::ast::Stmt]) {
    for s in calcs {
        if s.op.code == "DEFINE" {
            define(t, scope, prefix, s);
            continue;
        }
        let Ok(len) = s.op.len.trim().parse::<u32>() else { continue };
        let Some(name) = s.op.result.first().and_then(|e| e.name().map(str::to_string)) else { continue };
        if len == 0 {
            continue;
        }
        // A length unlike the field's definition: the definition stands.
        if let Some(id) = t.find(scope, &name) {
            let ty = &t.syms[id].ty;
            let had = ty.shape().map(|(d, _)| u32::from(d)).or_else(|| ty.str_len().filter(|_| !ty.is_varying()));
            if had.is_some_and(|h| h != len) {
                diags.push(Diag::new(crate::diag::ids::LEN_DIFFERS, 20, s.at).arg(name));
            }
            continue;
        }
        let ty = match s.op.decimals.trim().parse::<u8>() {
            Ok(d) => Ty::Packed(u8::try_from(len.min(63)).unwrap_or(63), d),
            Err(_) => Ty::Char(len),
        };
        let _ = t.add(Sym {
            name: name.clone(),
            kind: Some(Kind::Field),
            ty,
            ir: format!("${prefix}{}", mangle(&name)),
            scope,
            storage: if scope == 0 { Storage::Static } else { Storage::Auto },
            def_line: s.at.line,
            implicit: true,
            ..Sym::default()
        });
    }
}

/// `*LIKE DEFINE base new`: a field like another, its length changed
/// by a signed length entry; `*DTAARA DEFINE area field`: the field
/// tied to a data area (the field's own name when factor 2 is blank).
fn define(t: &mut Table, scope: usize, prefix: &str, s: &rpgfront::ast::Stmt) {
    let word = match s.op.f1.first() {
        Some(rpgfront::ast::Expr::Special { word, .. }) => word.clone(),
        _ => return,
    };
    let Some(name) = s.op.result.first().and_then(|e| match e {
        rpgfront::ast::Expr::Special { word, .. } => Some(word.clone()),
        e => e.name().map(str::to_string),
    }) else {
        return;
    };
    let base = s.op.f2.first().and_then(|e| e.name().map(str::to_string));
    if word == "*DTAARA" {
        let area = base.clone().unwrap_or_else(|| name.clone());
        if t.find(scope, &name).is_none()
            && let Ok(len) = s.op.len.trim().parse::<u32>()
        {
            let ty = match s.op.decimals.trim().parse::<u8>() {
                Ok(d) => Ty::Packed(u8::try_from(len.min(63)).unwrap_or(63), d),
                Err(_) => Ty::Char(len),
            };
            let _ = t.add(Sym {
                name: name.clone(),
                kind: Some(Kind::Field),
                ty,
                ir: format!("${prefix}{}", mangle(&name)),
                scope,
                storage: if scope == 0 { Storage::Static } else { Storage::Auto },
                def_line: s.at.line,
                implicit: true,
                ..Sym::default()
            });
        }
        if let Some(id) = t.find(scope, &name).or_else(|| t.find(0, &name)) {
            t.dtaaras.insert(id, area.to_ascii_uppercase());
        }
        return;
    }
    if word != "*LIKE" || t.find(scope, &name).is_some() {
        return;
    }
    let Some(b) = base.and_then(|b| t.find(scope, &b).or_else(|| t.find(0, &b))) else { return };
    let src = t.syms[b].clone();
    let adj: i64 = s.op.len.trim().replace('+', "").parse().unwrap_or(0);
    let grow = |n: u32| u32::try_from((i64::from(n) + adj).max(1)).unwrap_or(1);
    let grow8 = |n: u8| u8::try_from((i64::from(n) + adj).clamp(1, 63)).unwrap_or(63);
    let ty = match src.ty {
        Ty::Char(n) => Ty::Char(grow(n)),
        Ty::VarChar(n, p) => Ty::VarChar(grow(n), p),
        Ty::Packed(d, sc) => Ty::Packed(grow8(d), sc),
        Ty::Zoned(d, sc) => Ty::Zoned(grow8(d), sc),
        Ty::Bin(d, sc) => Ty::Bin(grow8(d), sc),
        other => other,
    };
    let _ = t.add(Sym {
        name: name.clone(),
        kind: Some(Kind::Field),
        ty,
        ir: format!("${prefix}{}", mangle(&name)),
        scope,
        storage: if scope == 0 { Storage::Static } else { Storage::Auto },
        def_line: s.at.line,
        implicit: true,
        ..Sym::default()
    });
}

/// Whether an output operation (`WRITE`, `UPDATE`, or an output
/// specification) names a record format.
#[must_use]
pub fn written(m: &rpgfront::ast::Module, fmt: &str) -> bool {
    let calcs = m.calcs.iter().chain(m.procs.iter().flat_map(|p| p.calcs.iter()));
    let by_op = calcs.filter(|s| matches!(s.op.code.as_str(), "WRITE" | "UPDATE")).any(|s| {
        s.op.f2.first().or_else(|| s.op.f1.first()).and_then(|e| e.name()).is_some_and(|n| n.eq_ignore_ascii_case(fmt))
    });
    by_op || m.outputs.iter().any(|o| o.name.eq_ignore_ascii_case(fmt))
}

/// The lines IBM's listing generates and numbers like source lines:
/// each `EXTNAME` structure's subfields after its line (`=D`), the
/// input specifications of the externally described input formats
/// before the first calculation (`=I`: a record line unless an I spec
/// names the format, and a line per input field), and the output
/// specifications of the output formats after the main calculations
/// (`=O`: a record line and a line per output field). Each entry is a
/// source line and how many generated lines come before it.
#[must_use]
pub fn generated_lines(m: &rpgfront::ast::Module, files: &[File], t: &Table) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let defs = m.defs.iter().chain(m.procs.iter().flat_map(|p| p.defs.iter()));
    for d in defs {
        let ext = d.kw("EXTNAME").is_some() || d.fixed.as_ref().is_some_and(|c| c.ext == "E");
        if !ext || d.kind != rpgfront::ast::DefKind::Ds {
            continue;
        }
        let n = t
            .find(0, &d.name.to_ascii_uppercase())
            .and_then(|id| match t.syms[id].kind {
                Some(Kind::Ds(i)) => Some(t.ds[i].subfields.len()),
                _ => None,
            })
            .unwrap_or(0);
        // Subfields the source gives come first: the block follows the
        // free-form END-DS.
        let at = d.children.last().map_or(d.at.line + 1, |c| c.at.line + if d.fixed.is_none() { 2 } else { 1 });
        out.push((at, u32::try_from(n).unwrap_or(0)));
    }
    let first_calc = m.calcs.first().map(|s| s.at.line);
    let after_calcs = m
        .outputs
        .first()
        .map(|o| o.at.line)
        .or_else(|| m.procs.first().map(|p| p.at.line))
        .or_else(|| m.ctdata.first().map(|c| c.at.line))
        .unwrap_or(u32::MAX);
    let mut gen_i = 0;
    let mut gen_o = 0;
    for r in &t.recfmts {
        let Some(f) = files.get(r.file) else { continue };
        if f.scope != 0 || r.qualified {
            continue;
        }
        if let Some(d) = &r.fmt.dsp {
            // A display format: its indicators (without INDARA) and the
            // fields of each buffer.
            let indara = f.ext.as_ref().is_some_and(|e| e.indara);
            let count = |n: usize| u32::try_from(n).unwrap_or(0);
            if f.input {
                let named = m.inputs.iter().any(|i| i.external && (i.name == r.name || i.name == r.fmt.name));
                let inds = if indara { 0 } else { d.response_inds.len() };
                gen_i += u32::from(!named) + count(inds) + count(d.offsets.iter().filter(|o| o.1.is_some()).count());
            }
            if f.output {
                let inds = if indara { 0 } else { d.option_inds.len() };
                gen_o += 1 + count(inds) + count(d.offsets.iter().filter(|o| o.0.is_some()).count());
            }
            continue;
        }
        if f.input || f.update {
            let named = m.inputs.iter().any(|i| i.external && (i.name == r.name || i.name == r.fmt.name));
            gen_i += u32::from(!named) + u32::try_from(r.fmt.fields.iter().filter(|x| matches!(x.usage, 'I' | 'B')).count()).unwrap_or(0);
        }
        if (f.output || f.update) && written(m, &r.name) {
            gen_o += 1 + u32::try_from(r.fmt.fields.iter().filter(|x| matches!(x.usage, 'O' | 'B')).count()).unwrap_or(0);
        }
    }
    if gen_i > 0 {
        out.push((first_calc.unwrap_or(after_calcs), gen_i));
    }
    if gen_o > 0 {
        out.push((after_calcs, gen_o));
    }
    out
}
