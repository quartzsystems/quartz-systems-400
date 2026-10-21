// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Field specifications to [`FieldDef`], shared by the PF and LF
//! compilers: the shape from positions 29-38 or from a reference, then
//! every field-level keyword.

use qsname::Name;

use super::{FileKind, check_db_columns, count, level_error, number_parm, parse_name, quoted_parm};
use crate::diag::{Diagnostics, ids};
use crate::lex::{Keyword, Parm, Spec};
use crate::model::{
    CompOp, DataType, DateFmt, DefaultValue, EditCode, FieldDef, FileRef, FloatPrecision, Literal,
    RecordFormat, RefSource, Reference, TimeFmt, Usage, Validity, VarLen,
};
use crate::refs::RefResolver;

/// What a field specification is compiled against.
pub struct FieldContext<'a> {
    /// Which compiler.
    pub kind: FileKind,
    /// The file-level `REF`.
    pub file_ref: Option<&'a FileRef>,
    /// Where other files come from.
    pub resolver: &'a dyn RefResolver,
    /// Fields already in the format, for `REFFLD(... *SRC)`.
    pub earlier: &'a [FieldDef],
}

/// The field-level keywords of one specification, by name.
#[derive(Default)]
pub(crate) struct Kws<'a> {
    pub(crate) reffld: Option<&'a Keyword>,
    pub(crate) colhdg: Option<&'a Keyword>,
    pub(crate) text: Option<&'a Keyword>,
    pub(crate) alias: Option<&'a Keyword>,
    pub(crate) ccsid: Option<&'a Keyword>,
    pub(crate) varlen: Option<&'a Keyword>,
    pub(crate) alwnull: Option<&'a Keyword>,
    pub(crate) dft: Option<&'a Keyword>,
    pub(crate) datfmt: Option<&'a Keyword>,
    pub(crate) datsep: Option<&'a Keyword>,
    pub(crate) timfmt: Option<&'a Keyword>,
    pub(crate) timsep: Option<&'a Keyword>,
    pub(crate) edtcde: Option<&'a Keyword>,
    pub(crate) edtwrd: Option<&'a Keyword>,
    pub(crate) check: Option<&'a Keyword>,
    pub(crate) values: Option<&'a Keyword>,
    pub(crate) range: Option<&'a Keyword>,
    pub(crate) comp: Option<&'a Keyword>,
    pub(crate) fltpcn: Option<&'a Keyword>,
    /// Logical files only.
    pub(crate) rename: Option<&'a Keyword>,
    /// Logical files only.
    pub(crate) concat: Option<&'a Keyword>,
    /// Logical files only.
    pub(crate) sst: Option<&'a Keyword>,
    /// Join logical files only.
    pub(crate) jref: Option<&'a Keyword>,
}

/// Sort a specification's keywords into their slots.
pub(crate) fn collect<'a>(spec: &'a Spec, kind: FileKind, diags: &mut Diagnostics) -> Kws<'a> {
    let mut k = Kws::default();
    for kw in &spec.keywords {
        let logical = kind == FileKind::Logical;
        let slot = match kw.name.as_str() {
            "REFFLD" if logical => {
                level_error(kw, "field", kind, diags);
                continue;
            }
            "REFFLD" => &mut k.reffld,
            "RENAME" if logical => &mut k.rename,
            "CONCAT" if logical => &mut k.concat,
            "SST" if logical => &mut k.sst,
            "JREF" if logical => &mut k.jref,
            "COLHDG" => &mut k.colhdg,
            "TEXT" => &mut k.text,
            "ALIAS" => &mut k.alias,
            "CCSID" => &mut k.ccsid,
            "VARLEN" => &mut k.varlen,
            "ALWNULL" => &mut k.alwnull,
            "DFT" => &mut k.dft,
            "DATFMT" => &mut k.datfmt,
            "DATSEP" => &mut k.datsep,
            "TIMFMT" => &mut k.timfmt,
            "TIMSEP" => &mut k.timsep,
            "EDTCDE" => &mut k.edtcde,
            "EDTWRD" => &mut k.edtwrd,
            "CHECK" => &mut k.check,
            "VALUES" => &mut k.values,
            "RANGE" => &mut k.range,
            "COMP" | "CMP" => &mut k.comp,
            "FLTPCN" => &mut k.fltpcn,
            _ => {
                level_error(kw, "field", kind, diags);
                continue;
            }
        };
        if slot.is_some() {
            diags.push(ids::duplicate_keyword(kw.line, &kw.name));
        } else {
            *slot = Some(kw);
        }
    }
    k
}

/// Compile one field specification.
pub fn build_field(
    spec: &Spec,
    ctx: &FieldContext<'_>,
    diags: &mut Diagnostics,
) -> Option<FieldDef> {
    let name = field_name(spec, diags)?;
    let kws = collect(spec, ctx.kind, diags);
    let f = if spec.reference || kws.reffld.is_some() {
        referenced(spec, &name, &kws, ctx, diags)?
    } else {
        fresh(spec, &name, diags)?
    };
    finish(spec, f, name, &kws, diags)
}

/// The name in positions 19-28 of a field specification.
pub(crate) fn field_name(spec: &Spec, diags: &mut Diagnostics) -> Option<Name> {
    let Some(text) = spec.name.as_deref() else {
        diags.push(ids::field_name_missing(spec.line));
        return None;
    };
    match parse_name(text) {
        Ok(n) => Some(n),
        Err(why) => {
            diags.push(ids::bad_name(spec.line, text, why));
            None
        }
    }
}

/// Apply the keywords of `spec` to a field whose shape is settled.
pub(crate) fn finish(
    spec: &Spec,
    mut f: FieldDef,
    name: Name,
    kws: &Kws<'_>,
    diags: &mut Diagnostics,
) -> Option<FieldDef> {
    check_db_columns(spec, diags);
    f.name = name;
    f.line = spec.line;
    f.usage = usage(spec, &name, diags);
    formats(spec, &mut f, kws, diags);
    attributes(&mut f, kws, diags);
    check_shape(spec, &f, diags)?;
    defaults(&mut f, kws, diags);
    editing(&mut f, kws, diags);
    validity(&mut f, kws, diags);
    f.byte_len = f.data_len() + if f.varlen.is_some() { 2 } else { 0 };
    Some(f)
}

/// Positions 30-37 written on a field that already has a shape replace it.
pub(crate) fn apply_overrides(spec: &Spec, f: &mut FieldDef, name: &Name, diags: &mut Diagnostics) {
    f.offset = 0;
    if let Some(ch) = spec.data_type {
        match DataType::from_letter(ch) {
            Some(t) => {
                f.data_type = t;
                if !t.is_numeric() {
                    f.decimals = 0;
                }
            }
            None => diags.push(ids::bad_data_type(spec.line, ch)),
        }
    }
    if let Some(len) = spec.length {
        f.length = len;
    }
    if spec.decimals.is_some() {
        f.decimals = decimals(spec, name, f.data_type, diags);
    }
}

fn usage(spec: &Spec, name: &Name, diags: &mut Diagnostics) -> Option<Usage> {
    let ch = spec.usage?;
    if Usage::from_letter(ch) == Some(Usage::Both) {
        return Some(Usage::Both);
    }
    diags.push(ids::bad_usage(spec.line, &name.to_string(), ch));
    None
}

fn fresh(spec: &Spec, name: &Name, diags: &mut Diagnostics) -> Option<FieldDef> {
    let data_type = match spec.data_type {
        Some(ch) => {
            let Some(t) = DataType::from_letter(ch) else {
                diags.push(ids::bad_data_type(spec.line, ch));
                return None;
            };
            t
        }
        None if spec.decimals.is_some() => DataType::Packed,
        None => DataType::Char,
    };
    let length = match spec.length {
        Some(n) => n,
        None if data_type.needs_length() => {
            diags.push(ids::length_required(spec.line, &name.to_string()));
            return None;
        }
        None => 0,
    };
    let decimals = decimals(spec, name, data_type, diags);
    Some(FieldDef::new(*name, data_type, length, decimals))
}

fn decimals(spec: &Spec, name: &Name, t: DataType, diags: &mut Diagnostics) -> u8 {
    match spec.decimals {
        Some(_) if !t.is_numeric() => {
            diags.push(ids::bad_field_decimals(
                spec.line,
                &name.to_string(),
                &format!("are not allowed for type {}", t.letter()),
            ));
            0
        }
        Some(d) => d,
        None => 0,
    }
}

fn referenced(
    spec: &Spec,
    name: &Name,
    kws: &Kws<'_>,
    ctx: &FieldContext<'_>,
    diags: &mut Diagnostics,
) -> Option<FieldDef> {
    let (field, format, source) = reference_target(spec, name, kws, ctx, diags)?;
    let (base, format) = match &source {
        RefSource::Source => {
            let found = ctx.earlier.iter().rev().find(|f| f.name == field);
            let Some(f) = found else {
                diags.push(ids::src_ref_not_found(
                    spec.line,
                    &name.to_string(),
                    &field.to_string(),
                ));
                return None;
            };
            (f.clone(), None)
        }
        RefSource::File(fr) => {
            let (f, fmt) = lookup(spec.line, fr, format.as_ref(), &field, ctx.resolver, diags)?;
            (f, Some(fmt))
        }
    };
    let mut f = base;
    f.reference = Some(Reference {
        source,
        field,
        format,
    });
    apply_overrides(spec, &mut f, name, diags);
    Some(f)
}

/// Which field, in which format, of which file a reference names.
fn reference_target(
    spec: &Spec,
    name: &Name,
    kws: &Kws<'_>,
    ctx: &FieldContext<'_>,
    diags: &mut Diagnostics,
) -> Option<(Name, Option<Name>, RefSource)> {
    let Some(kw) = kws.reffld else {
        let Some(fr) = ctx.file_ref else {
            diags.push(ids::ref_without_file(spec.line, &name.to_string()));
            return None;
        };
        return Some((*name, fr.format, RefSource::File(fr.clone())));
    };
    if !count(kw, 1, 2, diags) {
        return None;
    }
    let bad = |diags: &mut Diagnostics, i: usize, why: &str| {
        diags.push(ids::bad_parm(kw.line, &kw.name, &kw.parms[i], why));
    };
    let Some(w) = kw.word(0) else {
        bad(diags, 0, "must be a field name");
        return None;
    };
    let (format, field) = match w.split_once('/') {
        Some((fmt, fld)) => (Some(parse_name(fmt)), parse_name(fld)),
        None => (None, parse_name(w)),
    };
    let field = match field {
        Ok(n) => n,
        Err(why) => {
            bad(diags, 0, &why);
            return None;
        }
    };
    let format = match format {
        Some(Ok(n)) => Some(n),
        Some(Err(why)) => {
            bad(diags, 0, &why);
            return None;
        }
        None => None,
    };
    let source = match kw.word(1) {
        None => {
            let Some(fr) = ctx.file_ref else {
                diags.push(ids::ref_without_file(spec.line, &name.to_string()));
                return None;
            };
            RefSource::File(fr.clone())
        }
        Some("*SRC") => RefSource::Source,
        Some(w) => match FileRef::parse(w) {
            Ok(fr) => RefSource::File(fr),
            Err(why) => {
                bad(diags, 1, &why);
                return None;
            }
        },
    };
    let format = format.or(match &source {
        RefSource::File(fr) => fr.format,
        RefSource::Source => None,
    });
    Some((field, format, source))
}

/// Find `field` in `fr`, in format `fmt` when given.
fn lookup(
    line: usize,
    fr: &FileRef,
    fmt: Option<&Name>,
    field: &Name,
    resolver: &dyn RefResolver,
    diags: &mut Diagnostics,
) -> Option<(FieldDef, Name)> {
    let formats = resolve(line, fr, resolver, diags)?;
    let candidates: Vec<&RecordFormat> = match fmt {
        Some(n) => {
            let Some(r) = formats.iter().find(|r| r.name == *n) else {
                diags.push(ids::ref_format_not_found(line, fr, &n.to_string()));
                return None;
            };
            vec![r]
        }
        None => formats.iter().collect(),
    };
    for r in candidates {
        if let Some((_, f)) = r.field(field) {
            return Some((f.clone(), r.name));
        }
    }
    diags.push(ids::ref_field_not_found(line, &field.to_string(), fr));
    None
}

/// The formats of a referenced file, with the lookup errors reported.
pub(crate) fn resolve(
    line: usize,
    fr: &FileRef,
    resolver: &dyn RefResolver,
    diags: &mut Diagnostics,
) -> Option<Vec<RecordFormat>> {
    match resolver.formats(&fr.library, &fr.file) {
        Ok(Some(f)) => Some(f),
        Ok(None) => {
            diags.push(ids::ref_file_not_found(line, fr));
            None
        }
        Err(e) => {
            diags.push(ids::ref_error(line, fr, e));
            None
        }
    }
}

/// `DATFMT`, `DATSEP`, `TIMFMT`, `TIMSEP` and the lengths they fix.
fn formats(spec: &Spec, f: &mut FieldDef, kws: &Kws<'_>, diags: &mut Diagnostics) {
    let name = f.name.to_string();
    let t = f.data_type;
    if let Some(kw) = kws.datfmt {
        if t != DataType::Date {
            diags.push(ids::keyword_type(kw.line, &kw.name, &name, t.letter()));
        } else if count(kw, 1, 1, diags) {
            match kw.word(0).and_then(DateFmt::from_word) {
                Some(d) => f.date_fmt = Some(d),
                None => diags.push(ids::bad_parm(
                    kw.line,
                    &kw.name,
                    &kw.parms[0],
                    "unknown format",
                )),
            }
        }
    }
    if let Some(kw) = kws.timfmt {
        if t != DataType::Time {
            diags.push(ids::keyword_type(kw.line, &kw.name, &name, t.letter()));
        } else if count(kw, 1, 1, diags) {
            match kw.word(0).and_then(TimeFmt::from_word) {
                Some(d) => f.time_fmt = Some(d),
                None => diags.push(ids::bad_parm(
                    kw.line,
                    &kw.name,
                    &kw.parms[0],
                    "unknown format",
                )),
            }
        }
    }
    match t {
        DataType::Date => {
            let fmt = f.date_fmt.unwrap_or_default();
            f.date_fmt = Some(fmt);
            let want = fmt.length();
            if spec.length.is_some() && f.length != want {
                diags.push(ids::datetime_length(
                    spec.line,
                    &name,
                    f.length,
                    want,
                    fmt.name(),
                ));
            }
            f.length = want;
            f.date_sep = Some(separator(
                kws.datsep,
                fmt.fixed_separator(),
                '/',
                fmt.name(),
                diags,
            ));
        }
        DataType::Time => {
            let fmt = f.time_fmt.unwrap_or_default();
            f.time_fmt = Some(fmt);
            if spec.length.is_some() && f.length != 8 {
                diags.push(ids::datetime_length(
                    spec.line,
                    &name,
                    f.length,
                    8,
                    fmt.name(),
                ));
            }
            f.length = 8;
            f.time_sep = Some(separator(
                kws.timsep,
                fmt.fixed_separator(),
                ':',
                fmt.name(),
                diags,
            ));
        }
        DataType::Timestamp => {
            if spec.length.is_some() && f.length != 26 {
                diags.push(ids::datetime_length(
                    spec.line,
                    &name,
                    f.length,
                    26,
                    "a timestamp",
                ));
            }
            f.length = 26;
        }
        _ => {
            for kw in [kws.datsep, kws.timsep].into_iter().flatten() {
                diags.push(ids::keyword_type(kw.line, &kw.name, &name, t.letter()));
            }
            f.date_fmt = None;
            f.date_sep = None;
            f.time_fmt = None;
            f.time_sep = None;
        }
    }
}

/// The separator in effect: the format's own, the keyword's, or the default.
fn separator(
    kw: Option<&Keyword>,
    fixed: Option<char>,
    default: char,
    fmt_name: &str,
    diags: &mut Diagnostics,
) -> char {
    let Some(kw) = kw else {
        return fixed.unwrap_or(default);
    };
    if let Some(c) = fixed {
        diags.push(ids::separator_not_allowed(kw.line, &kw.name, fmt_name));
        return c;
    }
    if !count(kw, 1, 1, diags) {
        return default;
    }
    let chosen = match &kw.parms[0] {
        Parm::Quoted(q) if q.chars().count() == 1 && "/-.,: ".contains(q.as_str()) => {
            q.chars().next()
        }
        Parm::Word(w) if w == "*JOB" => {
            diags.push(ids::bad_parm(
                kw.line,
                &kw.name,
                w,
                "*JOB is not valid for a database file",
            ));
            None
        }
        p => {
            diags.push(ids::bad_parm(
                kw.line,
                &kw.name,
                p,
                "must be a quoted separator",
            ));
            None
        }
    };
    chosen.unwrap_or(default)
}

/// `FLTPCN`, `COLHDG`, `TEXT`, `ALIAS`, `CCSID`, `VARLEN`, `ALWNULL`.
fn attributes(f: &mut FieldDef, kws: &Kws<'_>, diags: &mut Diagnostics) {
    let name = f.name.to_string();
    let t = f.data_type;
    let wrong_type = |diags: &mut Diagnostics, kw: &Keyword| {
        diags.push(ids::keyword_type(kw.line, &kw.name, &name, t.letter()));
    };
    if let Some(kw) = kws.fltpcn {
        if t != DataType::Float {
            wrong_type(diags, kw);
        } else if count(kw, 1, 1, diags) {
            match kw.word(0) {
                Some("*SINGLE") => f.precision = FloatPrecision::Single,
                Some("*DOUBLE") => f.precision = FloatPrecision::Double,
                _ => diags.push(ids::bad_parm(
                    kw.line,
                    &kw.name,
                    &kw.parms[0],
                    "use *SINGLE or *DOUBLE",
                )),
            }
        }
    }
    if let Some(kw) = kws.colhdg
        && count(kw, 1, 3, diags)
    {
        let mut lines = Vec::new();
        for p in &kw.parms {
            match p.as_quoted() {
                Some(q) if q.chars().count() <= 20 => lines.push(q.to_string()),
                Some(_) => diags.push(ids::bad_parm(
                    kw.line,
                    &kw.name,
                    p,
                    "longer than 20 characters",
                )),
                None => diags.push(ids::bad_parm(kw.line, &kw.name, p, "must be quoted")),
            }
        }
        f.colhdg = lines;
    }
    if let Some(kw) = kws.text
        && count(kw, 1, 1, diags)
        && let Some(s) = quoted_parm(kw, 0, 50, diags)
    {
        f.text = Some(s);
    }
    if let Some(kw) = kws.alias
        && count(kw, 1, 1, diags)
    {
        match kw.word(0).filter(|w| valid_alias(w)) {
            Some(a) => f.alias = Some(a.to_string()),
            None => diags.push(ids::bad_parm(
                kw.line,
                &kw.name,
                &kw.parms[0],
                "1 to 30 letters, digits or underscores",
            )),
        }
    }
    if let Some(kw) = kws.ccsid {
        if !matches!(t, DataType::Char | DataType::Graphic | DataType::DbcsOpen) {
            wrong_type(diags, kw);
        } else if count(kw, 1, 1, diags) {
            match number_parm(kw, 0, diags).map(u16::try_from) {
                Some(Ok(c)) if c > 0 => f.ccsid = Some(c),
                Some(_) => diags.push(ids::bad_parm(
                    kw.line,
                    &kw.name,
                    &kw.parms[0],
                    "must be 1 to 65535",
                )),
                None => {}
            }
        }
    }
    if let Some(kw) = kws.varlen {
        if !matches!(t, DataType::Char | DataType::Graphic | DataType::DbcsOpen) {
            wrong_type(diags, kw);
        } else if count(kw, 0, 1, diags) {
            let allocated = if kw.parms.is_empty() {
                None
            } else {
                number_parm(kw, 0, diags)
            };
            if let Some(a) = allocated
                && a > f.length
            {
                diags.push(ids::varlen_alloc(kw.line, &name, a, f.length));
            }
            f.varlen = Some(VarLen { allocated });
        }
    }
    if let Some(kw) = kws.alwnull {
        count(kw, 0, 0, diags);
        f.allow_null = true;
    }
}

fn valid_alias(w: &str) -> bool {
    let n = w.chars().count();
    (1..=30).contains(&n)
        && w.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || "$#@".contains(c))
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_$#@".contains(c))
}

/// Length, decimals and precision against the type.
fn check_shape(spec: &Spec, f: &FieldDef, diags: &mut Diagnostics) -> Option<()> {
    let name = f.name.to_string();
    let t = f.data_type;
    let (lo, hi) = t.length_range();
    let mut ok = true;
    if !(lo..=hi).contains(&f.length) {
        diags.push(ids::bad_field_length(
            spec.line,
            &name,
            f.length,
            t.letter(),
            &format!("{lo} to {hi}"),
        ));
        ok = false;
    }
    if t == DataType::Float && f.precision == FloatPrecision::Single && f.length > 9 {
        diags.push(ids::float_precision(spec.line, &name, f.length));
        ok = false;
    }
    if t.is_numeric() && u32::from(f.decimals) > f.length {
        diags.push(ids::bad_field_decimals(
            spec.line,
            &name,
            "exceed the length",
        ));
        ok = false;
    }
    ok.then_some(())
}

/// `DFT`.
fn defaults(f: &mut FieldDef, kws: &Kws<'_>, diags: &mut Diagnostics) {
    let Some(kw) = kws.dft else {
        return;
    };
    if !count(kw, 1, 1, diags) {
        return;
    }
    let name = f.name.to_string();
    let t = f.data_type;
    let bad =
        |diags: &mut Diagnostics, why: &str| diags.push(ids::bad_default(kw.line, &name, why));
    let chars = |s: &str| u32::try_from(s.chars().count()).unwrap_or(u32::MAX);
    let value = match &kw.parms[0] {
        Parm::Word(w) if w == "*NULL" => {
            if !f.allow_null {
                diags.push(ids::null_default_needs_alwnull(kw.line, &name));
            }
            DefaultValue::Null
        }
        Parm::Word(w) => {
            if !t.is_numeric() {
                return bad(diags, "is numeric but the field is not");
            }
            if !numeric_fits(w, f.length, f.decimals) {
                return bad(diags, "does not fit the field");
            }
            DefaultValue::Number(w.clone())
        }
        Parm::Quoted(q) => {
            if t.is_numeric() {
                return bad(diags, "is a quoted string but the field is numeric");
            }
            if t.is_datetime() && chars(q) != f.length {
                return bad(diags, "does not match the format length");
            }
            if t.is_character() && chars(q) > f.length {
                return bad(diags, "is longer than the field");
            }
            DefaultValue::Quoted(q.clone())
        }
        Parm::Hex(h) => {
            if !t.is_character() {
                return bad(diags, "is hexadecimal but the field is not character");
            }
            if u32::try_from(h.len()).unwrap_or(u32::MAX) > f.data_len() {
                return bad(diags, "is longer than the field");
            }
            DefaultValue::Hex(h.clone())
        }
        Parm::Group(_) => return bad(diags, "is not a literal"),
    };
    f.default = Some(value);
}

/// The integer and fraction digits of a numeric literal, if it is one.
fn numeric_parts(w: &str) -> Option<(&str, &str)> {
    let s = w.strip_prefix(['+', '-']).unwrap_or(w);
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let digits = |p: &str| p.chars().all(|c| c.is_ascii_digit());
    if !digits(int) || !digits(frac) || (int.is_empty() && frac.is_empty()) {
        return None;
    }
    Some((int, frac))
}

/// Whether a numeric literal fits `length` digits with `decimals`.
fn numeric_fits(w: &str, length: u32, decimals: u8) -> bool {
    let Some((int, frac)) = numeric_parts(w) else {
        return false;
    };
    let int_len = u32::try_from(int.trim_start_matches('0').len()).unwrap_or(u32::MAX);
    let frac_len = u32::try_from(frac.len()).unwrap_or(u32::MAX);
    int_len <= length.saturating_sub(u32::from(decimals)) && frac_len <= u32::from(decimals)
}

/// `EDTCDE` and `EDTWRD`.
fn editing(f: &mut FieldDef, kws: &Kws<'_>, diags: &mut Diagnostics) {
    if kws.edtcde.is_none() && kws.edtwrd.is_none() {
        return;
    }
    // The referencing field's editing replaces the referenced one's.
    f.edit_code = None;
    f.edit_word = None;
    let name = f.name.to_string();
    let t = f.data_type;
    if let (Some(_), Some(w)) = (kws.edtcde, kws.edtwrd) {
        diags.push(ids::conflict(w.line, "EDTWRD", "EDTCDE"));
        return;
    }
    if let Some(kw) = kws.edtcde {
        if !t.is_numeric() {
            diags.push(ids::keyword_type(kw.line, &kw.name, &name, t.letter()));
        } else if count(kw, 1, 2, diags) {
            let code = kw
                .word(0)
                .filter(|w| w.len() == 1 && "1234ABCDJKLMNOPQXYZ56789".contains(*w))
                .and_then(|w| w.chars().next());
            let Some(code) = code else {
                diags.push(ids::bad_parm(
                    kw.line,
                    &kw.name,
                    &kw.parms[0],
                    "not an edit code",
                ));
                return;
            };
            let modifier = match kw.parms.get(1) {
                None => None,
                Some(Parm::Word(w)) if w == "*CR" || w == "-" => Some(w.clone()),
                Some(Parm::Quoted(q)) if q.chars().count() == 1 => Some(q.clone()),
                Some(p) => {
                    diags.push(ids::bad_parm(
                        kw.line,
                        &kw.name,
                        p,
                        "use *CR, - or a currency symbol",
                    ));
                    return;
                }
            };
            f.edit_code = Some(EditCode { code, modifier });
        }
    }
    if let Some(kw) = kws.edtwrd {
        if !t.is_numeric() {
            diags.push(ids::keyword_type(kw.line, &kw.name, &name, t.letter()));
        } else if count(kw, 1, 1, diags) {
            f.edit_word = quoted_parm(kw, 0, 120, diags);
        }
    }
}

/// `CHECK`, `VALUES`, `RANGE`, `COMP`.
fn validity(f: &mut FieldDef, kws: &Kws<'_>, diags: &mut Diagnostics) {
    let given: Vec<&Keyword> = [kws.check, kws.values, kws.range, kws.comp]
        .into_iter()
        .flatten()
        .collect();
    if given.is_empty() {
        return;
    }
    // The referencing field's checks replace the referenced one's.
    f.validity.clear();
    let exclusive: Vec<&Keyword> = [kws.values, kws.range, kws.comp]
        .into_iter()
        .flatten()
        .collect();
    if exclusive.len() > 1 {
        diags.push(ids::conflict(
            exclusive[1].line,
            &exclusive[1].name,
            &exclusive[0].name,
        ));
        return;
    }
    let numeric = f.data_type.is_numeric();
    if let Some(kw) = kws.check
        && count(kw, 1, 10, diags)
    {
        let mut codes = Vec::new();
        for p in &kw.parms {
            match p.as_word() {
                Some(w) => codes.push(w.to_string()),
                None => diags.push(ids::bad_parm(kw.line, &kw.name, p, "must be a check code")),
            }
        }
        f.validity.push(Validity::Check(codes));
    }
    if let Some(kw) = kws.values
        && count(kw, 1, 100, diags)
        && let Some(lits) = literals(kw, numeric, diags)
    {
        f.validity.push(Validity::Values(lits));
    }
    if let Some(kw) = kws.range
        && count(kw, 2, 2, diags)
        && let Some(mut lits) = literals(kw, numeric, diags)
    {
        let high = lits.pop().expect("two");
        let low = lits.pop().expect("two");
        f.validity.push(Validity::Range(low, high));
    }
    if let Some(kw) = kws.comp
        && count(kw, 2, 2, diags)
    {
        let Some(op) = kw.word(0).and_then(CompOp::from_word) else {
            diags.push(ids::bad_parm(
                kw.line,
                &kw.name,
                &kw.parms[0],
                "not a relational operator",
            ));
            return;
        };
        if let Some(lit) = literal(kw, &kw.parms[1], numeric, diags) {
            f.validity.push(Validity::Comp(op, lit));
        }
    }
}

pub(crate) fn literals(
    kw: &Keyword,
    numeric: bool,
    diags: &mut Diagnostics,
) -> Option<Vec<Literal>> {
    kw.parms
        .iter()
        .map(|p| literal(kw, p, numeric, diags))
        .collect()
}

/// A literal that suits the field's type.
pub(crate) fn literal(
    kw: &Keyword,
    p: &Parm,
    numeric: bool,
    diags: &mut Diagnostics,
) -> Option<Literal> {
    let lit = Literal::from_parm(p);
    let ok = match &lit {
        Some(Literal::Word(w)) => numeric && numeric_parts(w).is_some(),
        Some(Literal::Quoted(_) | Literal::Hex(_)) => !numeric,
        None => false,
    };
    if ok {
        return lit;
    }
    let why = if numeric {
        "must be a number"
    } else {
        "must be quoted"
    };
    diags.push(ids::bad_parm(kw.line, &kw.name, p, why));
    None
}
