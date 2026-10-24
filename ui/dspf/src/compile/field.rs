// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The field level: constants and named fields, their shape from the
//! columns, a reference or a subfile keyword, their location on each
//! display size and their keywords.

use dds::diag::{Diagnostics, ids as dds_ids};
use dds::lex::{Keyword, Spec, keyword::CONSTANT};
use dds::model::{CompOp, DataType, DateFmt, FileRef, Literal, TimeFmt, Validity};
use qdecimal::{EditOpts, Fill, Shape};

use super::file::{chginpdft, indtxt};
use super::{Cx, accept, classify, file_parm, ind_text, name_of, name_parm, number_in, quoted};
use crate::cond::{self, Entry};
use crate::diag::ids;
use crate::kw::{KwInfo, Level};
use crate::model::{
    Attr, Constant, Edit, ErrMsg, Field, FieldType, FileAttrs, Position, Record, SflRole,
    TypeClass, Usage,
};

const DSPATR_VALUES: &[&str] = &[
    "BL", "CS", "HI", "ND", "PC", "PR", "RI", "UL", "MDT", "OID", "SP",
];
const DSPATR_INPUT_ONLY: &[&str] = &["PR", "MDT", "OID", "SP"];
const COLORS: &[&str] = &["BLU", "GRN", "PNK", "RED", "TRQ", "WHT", "YLW"];
const CHECKS: &[&str] = &[
    "AB", "ER", "FE", "LC", "M10", "M10F", "M11", "M11F", "MF", "ME", "RB", "RL", "RLTB", "RZ",
    "VN", "VNE",
];
const EDIT_CODES: &str = "1234ABCDJKLMNOPQXYZ56789";

/// Positions 42-44.
#[derive(Clone, Copy)]
enum Col {
    Abs(u8),
    Rel(u8),
}

/// Positions 39-44, or `None` when blank.
fn location(
    spec: &Spec,
    cx: &Cx<'_>,
    diags: &mut Diagnostics,
) -> Result<Option<(Option<u8>, Col)>, ()> {
    let raw = &cx.source.lines()[spec.line];
    let row = raw.cols(39, 41);
    let col = raw.cols(42, 44);
    let (r, c) = (row.trim(), col.trim());
    if r.is_empty() && c.is_empty() {
        return Ok(None);
    }
    let bad = |diags: &mut Diagnostics| {
        diags.push(ids::bad_location(spec.line, &format!("{row}{col}")));
    };
    let row = if r.is_empty() {
        None
    } else {
        match r.parse::<u8>() {
            Ok(n) if (1..=27).contains(&n) => Some(n),
            _ => {
                bad(diags);
                return Err(());
            }
        }
    };
    let col = if let Some(rel) = c.strip_prefix('+') {
        if let Ok(n) = rel.trim().parse::<u8>() {
            Col::Rel(n)
        } else {
            bad(diags);
            return Err(());
        }
    } else {
        match c.parse::<u8>() {
            Ok(n) if (1..=132).contains(&n) => Col::Abs(n),
            _ => {
                bad(diags);
                return Err(());
            }
        }
    };
    if row.is_none() && matches!(col, Col::Abs(_)) {
        bad(diags);
        return Err(());
    }
    Ok(Some((row, col)))
}

/// The position on display size `i` from a parsed location, `+n`
/// counted from the end of `prev`.
fn place(
    prev: Option<&Field>,
    size: usize,
    cols: u8,
    row: Option<u8>,
    col: Col,
    line: usize,
    diags: &mut Diagnostics,
) -> Option<Position> {
    match col {
        Col::Abs(c) => Some(Position { row: row?, col: c }),
        Col::Rel(gap) => {
            let Some(before) = prev.and_then(|f| f.position(size)) else {
                diags.push(ids::relative_without_previous(line));
                return None;
            };
            let width = u32::from(cols);
            let start = (u32::from(before.row) - 1) * width + u32::from(before.col);
            let end = start + prev?.screen_len.max(1) - 1;
            let at = end + u32::from(gap) + 1;
            let new_row = (at - 1) / width + 1;
            let new_col = (at - 1) % width + 1;
            if let Some(given) = row
                && u32::from(given) != new_row
            {
                diags.push(ids::relative_without_previous(line));
                return None;
            }
            Some(Position {
                row: u8::try_from(new_row).unwrap_or(u8::MAX),
                col: u8::try_from(new_col).unwrap_or(u8::MAX),
            })
        }
    }
}

/// Whether the specification is an alternate location for the field
/// before it: `*DSn` in positions 8-16 and only a location besides.
pub(crate) fn is_alt_location(spec: &Spec) -> bool {
    cond::size_name(&spec.conditioning).is_some()
        && spec.name.is_none()
        && spec.attributes_blank()
        && !spec.location.is_empty()
}

/// Place the previous field on another display size and apply the
/// line's keywords to it.
pub(crate) fn alt_location(
    cx: &Cx<'_>,
    rec: &mut Record,
    spec: &Spec,
    entry: Entry,
    file: &mut FileAttrs,
    diags: &mut Diagnostics,
) {
    let name = spec.conditioning.trim();
    let Some(i) = cx.names.iter().position(|n| n == name) else {
        diags.push(ids::unknown_size_condition(spec.line, name));
        return;
    };
    let n = rec.fields.len();
    if n == 0 {
        diags.push(ids::alt_location_without_field(spec.line));
        return;
    }
    let (before, this) = rec.fields.split_at_mut(n - 1);
    let f = &mut this[0];
    if !f.is_displayed() {
        diags.push(ids::location_not_allowed(
            spec.line,
            &f.label(),
            f.usage.letter(),
        ));
        return;
    }
    if let Ok(Some((row, col))) = location(spec, cx, diags)
        && let Some(p) = place(
            before.last(),
            i,
            cx.sizes[i].cols,
            row,
            col,
            spec.line,
            diags,
        )
    {
        while f.positions.len() <= i {
            let primary = f.positions.first().copied().unwrap_or_default();
            f.positions.push(primary);
        }
        f.positions[i] = p;
    }
    let accepted = classify_all(&spec.keywords, diags);
    keywords(cx, f, &accepted, entry, file, diags);
    f.screen_len = screen_len(f);
}

fn classify_all<'k>(
    kws: &'k [Keyword],
    diags: &mut Diagnostics,
) -> Vec<(&'k Keyword, &'static KwInfo)> {
    kws.iter()
        .filter_map(|k| classify(k, Level::Field, diags).map(|i| (k, i)))
        .collect()
}

/// Compile one field or constant specification.
#[allow(clippy::too_many_lines)] // the shape has many sources
pub(crate) fn build(
    cx: &Cx<'_>,
    rec: &Record,
    spec: &Spec,
    entry: Entry,
    file: &mut FileAttrs,
    diags: &mut Diagnostics,
) -> Option<Field> {
    let cond = cx.cond(entry, spec.line, diags);
    let loc = location(spec, cx, diags).ok()?;
    let accepted = classify_all(&spec.keywords, diags);
    let find = |name: &str| {
        accepted
            .iter()
            .find(|(k, _)| k.name == name)
            .map(|(k, _)| *k)
    };

    let constant = constant_of(spec, &accepted, diags);
    let mut f = if let Some(c) = constant {
        if spec.name.is_some() || !spec.attributes_blank() {
            diags.push(ids::constant_attributes(spec.line));
            return None;
        }
        let mut f = Field::new(None, FieldType::char(), 0, 0, Usage::Output);
        f.constant = Some(c);
        f
    } else {
        let Some(text) = spec.name.as_deref() else {
            diags.push(ids::unnamed_field(spec.line));
            return None;
        };
        let name = match name_of(text) {
            Ok(n) => n,
            Err(why) => {
                diags.push(dds_ids::bad_name(spec.line, text, why));
                return None;
            }
        };
        if rec.field(&name).is_some() {
            diags.push(ids::duplicate_field(spec.line, &name));
            return None;
        }
        let Some(usage) = Usage::from_letter(spec.usage) else {
            diags.push(ids::bad_usage(spec.line, spec.usage.unwrap_or(' ')));
            return None;
        };
        let (mut ty, mut len, mut dec): (Option<FieldType>, Option<u32>, Option<u8>) =
            (None, spec.length, spec.decimals);
        let mut reference = None;
        if spec.reference || find("REFFLD").is_some() {
            let (field, file_ref) = match find("REFFLD") {
                Some(k) => reffld(k, cx, diags)?,
                None => (name.clone(), cx.ref_file.clone()),
            };
            let Some((t, l, d)) = referenced(cx, file_ref.as_ref(), &field, usage == Usage::Hidden) else {
                diags.push(ids::ref_not_found(spec.line, &field));
                return None;
            };
            ty = Some(t);
            len = len.or(Some(l));
            dec = dec.or(t.is_numeric().then_some(d));
            reference = Some((field, file_ref));
        }
        let mut role = None;
        for (k, _) in &accepted {
            let (r, t, l) = match k.name.as_str() {
                "SFLRCDNBR" => (
                    SflRole::RcdNbr {
                        cursor: k.parms.iter().any(|p| p.as_word() == Some("CURSOR")),
                        top: k.parms.iter().any(|p| p.as_word() == Some("*TOP")),
                    },
                    FieldType::zoned(),
                    4,
                ),
                "SFLPGMQ" => {
                    let l = if k.parms.first().and_then(|p| p.as_word()) == Some("276") {
                        276
                    } else {
                        10
                    };
                    (SflRole::PgmQ, FieldType::char(), l)
                }
                "SFLMSGKEY" => (SflRole::MsgKey, FieldType::char(), 4),
                "SFLROLVAL" => (SflRole::RolVal, FieldType::zoned(), 4),
                _ => continue,
            };
            role = Some(r);
            ty = ty.or(Some(t));
            len = len.or(Some(l));
            if t.is_numeric() {
                dec = dec.or(Some(0));
            }
        }
        if let Some(ch) = spec.data_type {
            if let Some(t) = FieldType::from_letter(ch, dec.is_some()) {
                ty = Some(t);
            } else {
                diags.push(ids::bad_data_type(spec.line, ch));
                return None;
            }
        }
        let ty = ty.unwrap_or_else(|| {
            if dec.is_some() {
                FieldType::zoned()
            } else {
                FieldType::char()
            }
        });
        let mut f = Field::new(Some(name.clone()), ty, 0, 0, usage);
        f.reference = reference;
        f.sfl_role = role;
        if let Some(d) = dec {
            if ty.is_numeric() {
                f.decimals = d;
            } else {
                diags.push(ids::bad_decimals(
                    spec.line,
                    &name,
                    &format!("are not allowed for type {}", ty.letter),
                ));
                return None;
            }
        }
        if ty.is_datetime() {
            f.date_fmt = find("DATFMT").and_then(|k| datfmt(k, diags));
            f.time_fmt = find("TIMFMT").and_then(|k| timfmt(k, diags));
            f.length = match ty.class {
                TypeClass::Date => f.date_fmt.unwrap_or_default().length(),
                TypeClass::Time => 8,
                _ => 26,
            };
        } else {
            let Some(l) = len.filter(|l| *l > 0) else {
                diags.push(ids::length_required(spec.line, &name));
                return None;
            };
            if l > ty.max_length() {
                diags.push(ids::bad_length(
                    spec.line,
                    &name,
                    l,
                    ty.letter,
                    ty.max_length(),
                ));
                return None;
            }
            f.length = l;
            if u32::from(f.decimals) > l {
                diags.push(ids::bad_decimals(spec.line, &name, "exceed the length"));
                return None;
            }
        }
        if usage == Usage::Message && ty.class != TypeClass::Char {
            diags.push(ids::keyword_type(spec.line, "usage M", &name, ty.letter));
            return None;
        }
        f
    };
    f.cond = cond;
    f.line = spec.line;
    let displayed = f.is_displayed();
    match (loc, displayed) {
        (Some((row, col)), true) => {
            let prev = rec.fields.last();
            let cols = cx.sizes[0].cols;
            f.relative = matches!(col, Col::Rel(_));
            let p = place(prev, 0, cols, row, col, spec.line, diags)?;
            f.positions.push(p);
        }
        (Some(_), false) => {
            diags.push(ids::location_not_allowed(
                spec.line,
                &f.label(),
                f.usage.letter(),
            ));
            return None;
        }
        (None, true) => {
            diags.push(ids::location_missing(spec.line, &f.label()));
            return None;
        }
        (None, false) => {}
    }
    keywords(cx, &mut f, &accepted, entry, file, diags);
    f.bytes = if f.is_constant() {
        0
    } else {
        f.ty.bytes(f.length)
    };
    f.screen_len = screen_len(&f);
    Some(f)
}

/// What constant the line defines, if it is one.
fn constant_of(
    spec: &Spec,
    accepted: &[(&Keyword, &KwInfo)],
    diags: &mut Diagnostics,
) -> Option<Constant> {
    let mut found = None;
    for (k, _) in accepted {
        let c = match k.name.as_str() {
            CONSTANT => Constant::Text(k.quoted(0).unwrap_or_default().to_string()),
            "DFT" if spec.name.is_none() => {
                Constant::Text(quoted(k, 0, 32_767, diags).unwrap_or_default())
            }
            "DATE" => {
                let words: Vec<&str> = k.parms.iter().filter_map(|p| p.as_word()).collect();
                for w in &words {
                    if !matches!(*w, "*SYS" | "*JOB" | "*Y" | "*YY") {
                        diags.push(ids::bad_value(k.line, "DATE", w, "*SYS, *JOB, *Y or *YY"));
                    }
                }
                Constant::Date {
                    sys: words.contains(&"*SYS"),
                    year4: words.contains(&"*YY"),
                }
            }
            "TIME" => {
                let w = k.parms.first().and_then(|p| p.as_word());
                if let Some(w) = w
                    && !matches!(w, "*SYS" | "*JOB")
                {
                    diags.push(ids::bad_value(k.line, "TIME", w, "*SYS or *JOB"));
                }
                Constant::Time {
                    sys: w == Some("*SYS"),
                }
            }
            "USER" => Constant::User,
            "SYSNAME" => Constant::SysName,
            "MSGCON" => {
                let len = number_in(k, 0, 1, 132, diags)?;
                let id = name_parm(k, 1, diags)?;
                let file = if k.parms.len() > 2 {
                    file_parm(k, 2, diags)?
                } else {
                    FileRef::parse("QCPFMSG").ok()?
                };
                Constant::Message { len, id, file }
            }
            _ => continue,
        };
        if found.is_some() {
            diags.push(dds_ids::conflict(
                k.line,
                &k.name,
                "another constant keyword",
            ));
        }
        found = Some(c);
    }
    found
}

/// `REFFLD(field [file | *SRC])`: the field and the file.
fn reffld(k: &Keyword, cx: &Cx<'_>, diags: &mut Diagnostics) -> Option<(String, Option<FileRef>)> {
    let first = k.parms.first()?.as_word()?;
    let field = name_of(first.rsplit('/').next().unwrap_or(first))
        .map_err(|why| diags.push(dds_ids::bad_parm(k.line, "REFFLD", first, why)))
        .ok()?;
    let file = match k.parms.get(1).and_then(|p| p.as_word()) {
        Some("*SRC") | None => cx.ref_file.clone(),
        Some(_) => Some(file_parm(k, 1, diags)?),
    };
    Some((field, file))
}

/// The shape of a referenced field.
fn referenced(cx: &Cx<'_>, file: Option<&FileRef>, field: &str, hidden: bool) -> Option<(FieldType, u32, u8)> {
    let f = file?;
    let formats = cx.resolver.formats(&f.library, &f.file).ok()??;
    let d = formats
        .iter()
        .filter(|fmt| f.format.as_ref().is_none_or(|n| *n == fmt.name))
        .flat_map(|fmt| fmt.fields.iter())
        .find(|d| d.name.to_string() == field)?;
    let letter = match d.data_type {
        DataType::Char | DataType::Hex => 'A',
        DataType::Graphic => 'G',
        DataType::DbcsOpen => 'O',
        // Only a hidden field keeps them; the screen shows them zoned.
        DataType::Packed if hidden => 'P',
        DataType::Binary if hidden => 'B',
        DataType::Zoned | DataType::Packed | DataType::Binary => 'S',
        DataType::Float => 'F',
        DataType::Date => 'L',
        DataType::Time => 'T',
        DataType::Timestamp => 'Z',
    };
    let ty = FieldType::from_letter(letter, d.data_type.is_numeric())?;
    Some((ty, d.length, d.decimals))
}

fn datfmt(k: &Keyword, diags: &mut Diagnostics) -> Option<DateFmt> {
    let w = k.parms[0].as_word().unwrap_or("");
    let f = DateFmt::from_word(w);
    if f.is_none() {
        diags.push(ids::bad_value(
            k.line,
            "DATFMT",
            w,
            "*ISO, *USA, *EUR, *JIS, *MDY, *DMY, *YMD or *JUL",
        ));
    }
    f
}

fn timfmt(k: &Keyword, diags: &mut Diagnostics) -> Option<TimeFmt> {
    let w = k.parms[0].as_word().unwrap_or("");
    let f = TimeFmt::from_word(w);
    if f.is_none() {
        diags.push(ids::bad_value(
            k.line,
            "TIMFMT",
            w,
            "*HMS, *ISO, *USA, *EUR or *JIS",
        ));
    }
    f
}

fn separator(k: &Keyword, diags: &mut Diagnostics) -> Option<char> {
    let p = &k.parms[0];
    match p.as_quoted().map(|q| q.chars().collect::<Vec<_>>()) {
        Some(v) if v.len() == 1 => Some(v[0]),
        _ if p.as_word() == Some("*JOB") => None,
        _ => {
            diags.push(dds_ids::bad_parm(
                k.line,
                &k.name,
                p,
                "must be one quoted character or *JOB",
            ));
            None
        }
    }
}

/// Apply the field's keywords.
#[allow(clippy::too_many_lines)] // one arm per field keyword
fn keywords(
    cx: &Cx<'_>,
    f: &mut Field,
    accepted: &[(&Keyword, &KwInfo)],
    entry: Entry,
    file: &mut FileAttrs,
    diags: &mut Diagnostics,
) {
    let label = f.label();
    for (k, info) in accepted {
        let used = accept(cx, k, info, entry, diags);
        let input = f.usage.is_input();
        let needs_input = |diags: &mut Diagnostics| {
            if !input {
                diags.push(ids::keyword_usage(k.line, &k.name, &label, "I or B"));
            }
            input
        };
        let numeric = f.ty.is_numeric() && !f.is_constant();
        match k.name.as_str() {
            CONSTANT | "DATE" | "TIME" | "USER" | "SYSNAME" | "MSGCON" | "REFFLD" | "SFLRCDNBR"
            | "SFLPGMQ" | "SFLMSGKEY" | "SFLROLVAL" | "DATFMT" | "TIMFMT" => {}
            "DFT" if f.is_constant() => {}
            "DFT" => {
                if let Some(v) = dft(k, f, &label, diags) {
                    f.dft = Some(v);
                }
            }
            "DSPATR" => {
                if !f.is_displayed() {
                    diags.push(ids::needs_display(k.line, &k.name, &label));
                } else if let Some(a) = dspatr(k, &used.cond, input, &label, diags) {
                    f.dspatr.push(a);
                }
            }
            "COLOR" => {
                let w = k.parms[0].as_word().unwrap_or("");
                if !f.is_displayed() {
                    diags.push(ids::needs_display(k.line, &k.name, &label));
                } else if COLORS.contains(&w) {
                    f.color.push(Attr {
                        values: vec![w.to_string()],
                        cond: used.cond.clone(),
                        line: k.line,
                    });
                } else {
                    diags.push(ids::bad_value(k.line, "COLOR", w, &COLORS.join(", ")));
                }
            }
            "EDTCDE" => {
                let date = matches!(f.constant, Some(Constant::Date { .. }));
                if !numeric && !date {
                    diags.push(ids::keyword_type(k.line, &k.name, &label, f.ty.letter));
                } else if f.edit != Edit::None {
                    diags.push(dds_ids::conflict(k.line, "EDTCDE", "EDTWRD"));
                } else if let Some(e) = edtcde(k, date, diags) {
                    f.edit = e;
                }
            }
            "EDTWRD" => {
                if !numeric {
                    diags.push(ids::keyword_type(k.line, &k.name, &label, f.ty.letter));
                } else if f.edit != Edit::None {
                    diags.push(dds_ids::conflict(k.line, "EDTWRD", "EDTCDE"));
                } else if let Some(w) = quoted(k, 0, 132, diags) {
                    f.edit = Edit::Word(w);
                }
            }
            "CHECK" => {
                if needs_input(diags) {
                    for p in &k.parms {
                        match p.as_word() {
                            Some(w) if CHECKS.contains(&w) => f.check.push(w.to_string()),
                            _ => diags.push(ids::bad_value(
                                k.line,
                                "CHECK",
                                &p.to_string(),
                                &CHECKS.join(", "),
                            )),
                        }
                    }
                }
            }
            "VALUES" => {
                if needs_input(diags)
                    && let Some(v) = literals(k, diags)
                {
                    f.validity.push(Validity::Values(v));
                }
            }
            "RANGE" => {
                if needs_input(diags)
                    && let Some(v) = literals(k, diags)
                    && v.len() == 2
                {
                    f.validity.push(Validity::Range(v[0].clone(), v[1].clone()));
                }
            }
            "COMP" | "CMP" => {
                if needs_input(diags) {
                    let op = k.parms[0].as_word().and_then(CompOp::from_word);
                    let value = Literal::from_parm(&k.parms[1]);
                    match (op, value) {
                        (Some(op), Some(v)) => f.validity.push(Validity::Comp(op, v)),
                        (None, _) => diags.push(ids::bad_value(
                            k.line,
                            &k.name,
                            &k.parms[0].to_string(),
                            "EQ, NE, LT, NL, GT, NG, LE or GE",
                        )),
                        (_, None) => diags.push(dds_ids::bad_parm(
                            k.line,
                            &k.name,
                            &k.parms[1],
                            "must be a literal",
                        )),
                    }
                }
            }
            "ERRMSG" => {
                let text = quoted(k, 0, 80, diags);
                if let Some((ind, _)) = ind_text(k, 1, diags) {
                    if let Some(i) = ind {
                        f.responses.push((i, k.line));
                    }
                    if let Some(t) = text {
                        f.errmsg.push(ErrMsg {
                            text: Some(t),
                            id: None,
                            ind,
                            cond: used.cond.clone(),
                            line: k.line,
                        });
                    }
                }
            }
            "ERRMSGID" => {
                let id = name_parm(k, 0, diags);
                let file_ref = file_parm(k, 1, diags);
                let mut ind = None;
                for i in 2..k.parms.len() {
                    if k.parms[i].as_word().is_some_and(|w| w.starts_with('&')) {
                        super::field_parm(k, i, diags);
                    } else if let Some(n) = number_in(k, i, 1, 99, diags) {
                        ind = Some(u8::try_from(n).unwrap_or(99));
                        f.responses.push((u8::try_from(n).unwrap_or(99), k.line));
                    }
                }
                if let (Some(id), Some(fr)) = (id, file_ref) {
                    f.errmsg.push(ErrMsg {
                        text: None,
                        id: Some((id, fr)),
                        ind,
                        cond: used.cond.clone(),
                        line: k.line,
                    });
                }
            }
            "MSGID" => {
                if f.usage == Usage::Message {
                    for i in 0..k.parms.len() {
                        if k.parms[i].as_word().is_some_and(|w| w.starts_with('&')) {
                            super::field_parm(k, i, diags);
                        } else if i == 1 {
                            file_parm(k, i, diags);
                        } else if i == 0 {
                            name_parm(k, i, diags);
                        }
                    }
                } else {
                    diags.push(ids::keyword_usage(k.line, &k.name, &label, "M"));
                }
            }
            "DUP" | "BLANKS" | "CHANGE" => {
                if needs_input(diags)
                    && let Some((Some(ind), _)) = ind_text(k, 0, diags)
                {
                    f.responses.push((ind, k.line));
                }
            }
            "SETOF" | "SETOFF" => {
                if let Some((Some(ind), _)) = ind_text(k, 0, diags) {
                    f.responses.push((ind, k.line));
                }
            }
            "AUTO" => {
                let w = k.parms[0].as_word().unwrap_or("");
                if needs_input(diags) && !matches!(w, "RA" | "RAB" | "RAZ") {
                    diags.push(ids::bad_value(k.line, "AUTO", w, "RA, RAB or RAZ"));
                }
            }
            "DATSEP" => {
                if f.ty.class == TypeClass::Date {
                    f.date_sep = separator(k, diags);
                } else {
                    diags.push(ids::keyword_type(k.line, &k.name, &label, f.ty.letter));
                }
            }
            "TIMSEP" => {
                if f.ty.class == TypeClass::Time {
                    f.time_sep = separator(k, diags);
                } else {
                    diags.push(ids::keyword_type(k.line, &k.name, &label, f.ty.letter));
                }
            }
            "TEXT" => f.text = quoted(k, 0, 50, diags),
            "INDTXT" => indtxt(&mut file.indtxt, k, diags),
            "CHGINPDFT" => {
                chginpdft(k, diags);
            }
            "MNUBARCHC" => {
                number_in(k, 0, 1, 99, diags);
                name_parm(k, 1, diags);
                if let Some(p) = k.parms.get(2)
                    && p.as_quoted().is_none()
                    && !p.as_word().is_some_and(|w| w.starts_with('&'))
                {
                    diags.push(dds_ids::bad_parm(
                        k.line,
                        &k.name,
                        p,
                        "must be quoted text or &field",
                    ));
                }
            }
            _ => {}
        }
        f.keywords.push(used);
    }
    if let Some(ch) = f.date_sep
        && let Some(fmt) = f.date_fmt
        && let Some(fixed) = fmt.fixed_separator()
        && fixed != ch
    {
        f.date_sep = Some(fixed);
    }
    shape_of_role(f, &label, diags);
}

/// `DATFMT`-shaped and `SFL*`-shaped fields must have the shape the
/// keyword expects.
fn shape_of_role(f: &Field, label: &str, diags: &mut Diagnostics) {
    let Some(role) = &f.sfl_role else {
        return;
    };
    let (kw, ok, want) = match role {
        SflRole::RcdNbr { .. } => (
            "SFLRCDNBR",
            f.ty.class == TypeClass::Zoned
                && f.length == 4
                && f.decimals == 0
                && !f.usage.is_input(),
            "zoned 4,0 with usage O or H",
        ),
        SflRole::PgmQ => (
            "SFLPGMQ",
            f.ty.class == TypeClass::Char && (f.length == 10 || f.length == 276),
            "character, 10 or 276 positions",
        ),
        SflRole::MsgKey => (
            "SFLMSGKEY",
            f.ty.class == TypeClass::Char && f.length == 4,
            "character, 4 positions",
        ),
        SflRole::RolVal => (
            "SFLROLVAL",
            f.ty.class == TypeClass::Zoned
                && f.length == 4
                && f.decimals == 0
                && f.usage.is_input(),
            "zoned 4,0 with usage I or B",
        ),
    };
    if !ok {
        diags.push(ids::field_shape(f.line, kw, label, want));
    }
}

/// `DFT('value')` on a named field.
fn dft(k: &Keyword, f: &Field, label: &str, diags: &mut Diagnostics) -> Option<String> {
    let p = &k.parms[0];
    if f.ty.is_numeric() {
        let w = p.as_word()?;
        if w.parse::<f64>().is_err() {
            diags.push(dds_ids::bad_parm(k.line, "DFT", p, "must be a number"));
            return None;
        }
        return Some(w.to_string());
    }
    let q = quoted(k, 0, 32_767, diags)?;
    if q.chars().count() > f.length as usize {
        diags.push(ids::dft_length(k.line, label, q.chars().count(), f.length));
        return None;
    }
    Some(q)
}

fn dspatr(
    k: &Keyword,
    cond: &crate::cond::Cond,
    input: bool,
    label: &str,
    diags: &mut Diagnostics,
) -> Option<Attr> {
    let mut values = Vec::new();
    for p in &k.parms {
        let Some(w) = p.as_word() else {
            diags.push(ids::bad_value(
                k.line,
                "DSPATR",
                &p.to_string(),
                &DSPATR_VALUES.join(", "),
            ));
            return None;
        };
        if let Some(field) = w.strip_prefix('&') {
            if k.parms.len() != 1 {
                diags.push(dds_ids::bad_parm(
                    k.line,
                    "DSPATR",
                    p,
                    "a program-to-system field stands alone",
                ));
                return None;
            }
            match name_of(field) {
                Ok(n) => values.push(format!("&{n}")),
                Err(why) => {
                    diags.push(dds_ids::bad_parm(k.line, "DSPATR", p, why));
                    return None;
                }
            }
            continue;
        }
        if !DSPATR_VALUES.contains(&w) {
            diags.push(ids::bad_value(
                k.line,
                "DSPATR",
                w,
                &DSPATR_VALUES.join(", "),
            ));
            return None;
        }
        if DSPATR_INPUT_ONLY.contains(&w) && !input {
            diags.push(ids::keyword_usage(
                k.line,
                &format!("DSPATR({w})"),
                label,
                "I or B",
            ));
            return None;
        }
        values.push(w.to_string());
    }
    Some(Attr {
        values,
        cond: cond.clone(),
        line: k.line,
    })
}

fn edtcde(k: &Keyword, date: bool, diags: &mut Diagnostics) -> Option<Edit> {
    let code = k
        .word(0)
        .filter(|w| w.len() == 1 && EDIT_CODES.contains(w))
        .and_then(|w| w.chars().next());
    let Some(code) = code else {
        diags.push(dds_ids::bad_parm(
            k.line,
            "EDTCDE",
            &k.parms[0],
            "not an edit code",
        ));
        return None;
    };
    if date && code != 'Y' {
        diags.push(ids::bad_value(
            k.line,
            "EDTCDE",
            &code.to_string(),
            "Y on a DATE field",
        ));
        return None;
    }
    let modifier = k
        .parms
        .get(1)
        .map(|p| p.as_quoted().map_or_else(|| p.to_string(), str::to_string));
    if let Some(m) = &modifier
        && m != "*"
        && m.chars().count() != 1
    {
        diags.push(dds_ids::bad_parm(
            k.line,
            "EDTCDE",
            &k.parms[1],
            "must be * or one currency character",
        ));
        return None;
    }
    Some(Edit::Code(code, modifier))
}

fn literals(k: &Keyword, diags: &mut Diagnostics) -> Option<Vec<Literal>> {
    let mut out = Vec::new();
    for p in &k.parms {
        if let Some(l) = Literal::from_parm(p) {
            out.push(l);
        } else {
            diags.push(dds_ids::bad_parm(k.line, &k.name, p, "must be a literal"));
            return None;
        }
    }
    Some(out)
}

/// Positions the field takes on the display.
pub(crate) fn screen_len(f: &Field) -> u32 {
    if let Some(c) = &f.constant {
        return match c {
            Constant::Text(t) => u32::try_from(t.chars().count()).unwrap_or(u32::MAX),
            Constant::Date { year4, .. } => {
                let digits = if *year4 { 8 } else { 6 };
                digits
                    + if matches!(f.edit, Edit::Code('Y', _)) {
                        2
                    } else {
                        0
                    }
            }
            Constant::Time { .. } | Constant::SysName => 8,
            Constant::User => 10,
            Constant::Message { len, .. } => *len,
        };
    }
    match f.ty.class {
        TypeClass::Char => f.length,
        TypeClass::Graphic => f.length * 2,
        TypeClass::Date => f.date_fmt.unwrap_or_default().length(),
        TypeClass::Time => 8,
        TypeClass::Timestamp => 26,
        _ => edited_len(f),
    }
}

/// A number's positions: edited, or digits with room for the sign and
/// the decimal point when the operator types it.
fn edited_len(f: &Field) -> u32 {
    match &f.edit {
        Edit::Code(c, m) => {
            let fill = match m.as_deref() {
                Some("*") => Fill::Asterisk,
                Some(cur) => Fill::Currency(cur.chars().next().unwrap_or('$')),
                None => Fill::Blank,
            };
            let opts = EditOpts {
                fill,
                ..EditOpts::default()
            };
            Shape::new(u8::try_from(f.length).unwrap_or(63), f.decimals)
                .and_then(|s| qdecimal::code_width(s, *c, &opts))
                .map_or(f.length, |n| u32::try_from(n).unwrap_or(f.length))
        }
        Edit::Word(w) => u32::try_from(w.chars().count()).unwrap_or(f.length),
        // Numeric only (Y) has no sign position.
        Edit::None if f.usage.is_input() && f.ty.letter == 'Y' => f.length,
        Edit::None if f.usage.is_input() => f.length + 1 + u32::from(f.decimals > 0),
        Edit::None => f.length,
    }
}
