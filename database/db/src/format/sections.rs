// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The section encoding of record formats. See `docs/DB_FORMAT.md` §3.

use dds::{
    CompOp, Condition, DataType, DateFmt, DefaultValue, DupOrder, FieldDef, FieldSource,
    FloatPrecision, KeyField, KeyTransform, Literal, NullKeys, RecordFormat, SelectOmit, TimeFmt,
    Usage, Validity, VarLen,
};
use qsname::Name;
use store::obj::ObjectId;

use super::{FormatDef, keys};
use crate::error::{DbError, Result};
use crate::lf::{JoinDef, LogicalDef};
use crate::spec::{Field, Layout, field};

/// Bytes of a section header.
pub const SECTION_LEN: usize = 8;
/// Bytes of a `FormatEntry`.
pub const FORMAT_LEN: usize = 96;
/// Bytes of a `FieldEntry`.
pub const FIELD_LEN: usize = 256;
/// Bytes of a `KeyEntry`.
pub const KEY_LEN: usize = 16;
/// Bytes of a `BasedOnEntry`.
pub const BASEDON_LEN: usize = 32;
/// Bytes of a `SourceEntry` before its names.
pub const SOURCE_HEAD: usize = 8;
/// Bytes of a `SelOmitGroup`.
pub const GROUP_LEN: usize = 8;
/// Bytes of a `SelOmitCondition`.
pub const CONDITION_LEN: usize = 8;
/// Bytes of a `SelOmitValue` before its bytes.
pub const VALUE_HEAD: usize = 4;
/// Bytes of a `JoinEntry` before its pairs.
pub const JOIN_HEAD: usize = 18;
/// Bytes of `dft` kept.
pub const DFT_MAX: usize = 64;

/// A record format.
pub const TAG_FORMAT: u16 = 1;
/// A field.
pub const TAG_FIELD: u16 = 2;
/// A key field.
pub const TAG_KEY: u16 = 3;
/// A based-on file.
pub const TAG_BASEDON: u16 = 4;
/// A field source.
pub const TAG_SOURCE: u16 = 5;
/// A select/omit group.
pub const TAG_SELOMIT: u16 = 6;
/// A join.
pub const TAG_JOIN: u16 = 7;

/// The spec layout of a section header.
pub static SECTION_LAYOUT: Layout = Layout {
    name: "Section",
    size: Some(SECTION_LEN),
    fields: &[
        field("tag", 0, 2),
        field("reserved", 2, 2),
        field("len", 4, 4),
    ],
};

/// The spec layout of a format entry.
pub static FORMAT_LAYOUT: Layout = Layout {
    name: "FormatEntry",
    size: Some(FORMAT_LEN),
    fields: &[
        field("name", 0, 10),
        field("level_id", 10, 13),
        field("reserved", 23, 1),
        field("field_count", 24, 2),
        field("key_count", 26, 2),
        field("record_len", 28, 4),
        field("null_map_len", 32, 2),
        field("flags", 34, 2),
        field("key_len", 36, 2),
        field("reserved2", 38, 8),
        field("text", 46, 50),
    ],
};

/// The spec layout of a field entry.
pub static FIELD_LAYOUT: Layout = Layout {
    name: "FieldEntry",
    size: Some(FIELD_LEN),
    fields: &[
        field("name", 0, 10),
        field("data_type", 10, 1),
        field("decimals", 11, 1),
        field("flags", 12, 2),
        field("length", 14, 4),
        field("byte_len", 18, 4),
        field("offset", 22, 4),
        field("ccsid", 26, 2),
        field("date_fmt", 28, 1),
        field("date_sep", 29, 1),
        field("time_fmt", 30, 1),
        field("time_sep", 31, 1),
        field("precision", 32, 1),
        field("edit_code", 33, 1),
        field("varlen_alloc", 34, 2),
        field("null_index", 36, 2),
        field("text", 38, 50),
        field("alias", 88, 30),
        field("colhdg", 118, 60),
        field("dft_kind", 178, 1),
        field("dft_len", 179, 1),
        field("dft", 180, 64),
        field("identity", 244, 1),
        field("id_start", 245, 4),
        field("id_incr", 249, 4),
        field("reserved", 253, 3),
    ],
};

/// The spec layout of a key entry.
pub static KEY_LAYOUT: Layout = Layout {
    name: "KeyEntry",
    size: Some(KEY_LEN),
    fields: &[
        field("field", 0, 2),
        field("flags", 2, 2),
        field("key_off", 4, 2),
        field("key_len", 6, 2),
        field("reserved", 8, 8),
    ],
};

/// The spec layout of a based-on entry.
pub static BASEDON_LAYOUT: Layout = Layout {
    name: "BasedOnEntry",
    size: Some(BASEDON_LEN),
    fields: &[
        field("file", 0, 16),
        field("format", 16, 10),
        field("index", 26, 2),
        field("reserved", 28, 4),
    ],
};

/// The spec layout of a source entry.
pub static SOURCE_LAYOUT: Layout = Layout {
    name: "SourceEntry",
    size: None,
    fields: &[
        field("kind", 0, 1),
        field("file", 1, 1),
        field("count", 2, 2),
        field("start", 4, 2),
        field("length", 6, 2),
        Field {
            name: "names",
            offset: Some(8),
            size: None,
        },
    ],
};

/// The spec layout of a select/omit group header.
pub static GROUP_LAYOUT: Layout = Layout {
    name: "SelOmitGroup",
    size: Some(GROUP_LEN),
    fields: &[
        field("select", 0, 1),
        field("all", 1, 1),
        field("cond_count", 2, 2),
        field("reserved", 4, 4),
    ],
};

/// The spec layout of a select/omit condition header.
pub static CONDITION_LAYOUT: Layout = Layout {
    name: "SelOmitCondition",
    size: Some(CONDITION_LEN),
    fields: &[
        field("field", 0, 2),
        field("test", 2, 1),
        field("op", 3, 1),
        field("value_count", 4, 2),
        field("reserved", 6, 2),
    ],
};

/// The spec layout of a select/omit value.
pub static VALUE_LAYOUT: Layout = Layout {
    name: "SelOmitValue",
    size: None,
    fields: &[
        field("kind", 0, 1),
        field("reserved", 1, 1),
        field("len", 2, 2),
        Field {
            name: "bytes",
            offset: Some(4),
            size: None,
        },
    ],
};

/// The spec layout of a join entry.
pub static JOIN_LAYOUT: Layout = Layout {
    name: "JoinEntry",
    size: None,
    fields: &[
        field("from", 0, 1),
        field("to", 1, 1),
        field("pair_count", 2, 2),
        field("dupseq", 4, 1),
        field("dupseq_descend", 5, 1),
        field("reserved", 6, 2),
        field("dupseq_field", 8, 10),
        Field {
            name: "pairs",
            offset: Some(18),
            size: None,
        },
    ],
};

// Little-endian helpers.
fn put_u16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn get_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn get_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().expect("4"))
}

fn u16_of(n: usize) -> u16 {
    u16::try_from(n).expect("fits in a section")
}

/// CP037 text into a fixed field, cut to fit.
fn put_text(b: &mut [u8], off: usize, len: usize, s: &str) {
    let out = &mut b[off..off + len];
    out.fill(ccsid::SPACE);
    for (at, ch) in s.chars().take(len).enumerate() {
        out[at] = ccsid::byte_of(ch).unwrap_or(ccsid::SPACE);
    }
}

fn get_text(b: &[u8], off: usize, len: usize) -> Option<String> {
    let s = ccsid::decode_trimmed(&b[off..off + len]);
    (!s.is_empty()).then_some(s)
}

fn name_of(b: &[u8]) -> Name {
    Name::from_bytes(b[..10].try_into().expect("10"))
}

fn corrupt(what: impl Into<String>) -> DbError {
    DbError::Corrupt { what: what.into() }
}

/// Write one section.
fn section(out: &mut Vec<u8>, tag: u16, body: &[u8]) {
    let mut h = [0u8; SECTION_LEN];
    put_u16(&mut h, 0, tag);
    put_u32(&mut h, 4, u32::try_from(body.len()).expect("fits"));
    out.extend_from_slice(&h);
    out.extend_from_slice(body);
}

/// Encode formats as sections.
#[must_use]
pub fn encode(formats: &[FormatDef]) -> Vec<u8> {
    let mut out = Vec::new();
    for def in formats {
        section(&mut out, TAG_FORMAT, &encode_format(def));
        for (i, f) in def.format.fields.iter().enumerate() {
            section(&mut out, TAG_FIELD, &encode_field(f, def.null_index(i)));
        }
        let mut key_off = 0u16;
        for k in &def.format.keys {
            let len = keys::part_len(&def.format.fields[k.field], k.transform);
            let len = u16::try_from(len).expect("checked");
            section(&mut out, TAG_KEY, &encode_key(k, key_off, len));
            key_off += len;
        }
        if let Some(log) = &def.logical {
            encode_logical(&mut out, def, log);
        }
    }
    out
}

fn encode_logical(out: &mut Vec<u8>, def: &FormatDef, log: &LogicalDef) {
    for (i, (file, format)) in log.based_on.iter().enumerate() {
        let mut b = [0u8; BASEDON_LEN];
        b[..16].copy_from_slice(&file.to_bytes());
        b[16..26].copy_from_slice(format.as_bytes());
        put_u16(&mut b, 26, u16_of(i));
        section(out, TAG_BASEDON, &b);
    }
    for src in &log.sources {
        section(out, TAG_SOURCE, &encode_source(src));
    }
    for g in &log.select_omit {
        section(out, TAG_SELOMIT, &encode_group(def, g));
    }
    for j in &log.joins {
        section(out, TAG_JOIN, &encode_join(j));
    }
}

fn encode_source(src: &FieldSource) -> Vec<u8> {
    let (kind, file, names, start, length): (u8, usize, Vec<Name>, u32, u32) = match src {
        FieldSource::Base { file, field } => (0, *file, vec![*field], 0, 0),
        FieldSource::Concat { file, fields } => (1, *file, fields.clone(), 0, 0),
        FieldSource::Substring {
            file,
            field,
            start,
            length,
        } => (2, *file, vec![*field], *start, *length),
    };
    let mut b = vec![0u8; SOURCE_HEAD + names.len() * 10];
    b[0] = kind;
    b[1] = u8::try_from(file).expect("at most 32 files");
    put_u16(&mut b, 2, u16_of(names.len()));
    put_u16(&mut b, 4, u16::try_from(start).unwrap_or(u16::MAX));
    put_u16(&mut b, 6, u16::try_from(length).unwrap_or(u16::MAX));
    for (i, n) in names.iter().enumerate() {
        b[SOURCE_HEAD + i * 10..SOURCE_HEAD + i * 10 + 10].copy_from_slice(n.as_bytes());
    }
    b
}

fn encode_value(out: &mut Vec<u8>, lit: &Literal) {
    let (kind, bytes): (u8, Vec<u8>) = match lit {
        Literal::Word(w) => (0, w.as_bytes().to_vec()),
        Literal::Quoted(q) => (
            1,
            q.chars()
                .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
                .collect(),
        ),
        Literal::Hex(h) => (2, h.clone()),
    };
    let mut h = [0u8; VALUE_HEAD];
    h[0] = kind;
    put_u16(&mut h, 2, u16_of(bytes.len()));
    out.extend_from_slice(&h);
    out.extend_from_slice(&bytes);
}

fn encode_group(def: &FormatDef, g: &SelectOmit) -> Vec<u8> {
    let mut b = vec![0u8; GROUP_LEN];
    b[0] = u8::from(g.select);
    b[1] = u8::from(g.all);
    put_u16(&mut b, 2, u16_of(g.conditions.len()));
    for c in &g.conditions {
        let mut h = [0u8; CONDITION_LEN];
        let index = def
            .format
            .field(&c.field)
            .map_or(u16::MAX, |(i, _)| u16_of(i));
        put_u16(&mut h, 0, index);
        let (test, op, values): (u8, u8, Vec<&Literal>) = match &c.test {
            Validity::Values(v) => (0, 0, v.iter().collect()),
            Validity::Range(lo, hi) => (1, 0, vec![lo, hi]),
            Validity::Comp(op, v) => (2, op_code(*op), vec![v]),
            Validity::Check(_) => (0, 0, Vec::new()),
        };
        h[2] = test;
        h[3] = op;
        put_u16(&mut h, 4, u16_of(values.len()));
        b.extend_from_slice(&h);
        for v in values {
            encode_value(&mut b, v);
        }
    }
    b
}

fn op_code(op: CompOp) -> u8 {
    match op {
        CompOp::Eq => 0,
        CompOp::Ne => 1,
        CompOp::Lt => 2,
        CompOp::Nl => 3,
        CompOp::Gt => 4,
        CompOp::Ng => 5,
        CompOp::Le => 6,
        CompOp::Ge => 7,
    }
}

fn op_of(code: u8) -> Option<CompOp> {
    Some(match code {
        0 => CompOp::Eq,
        1 => CompOp::Ne,
        2 => CompOp::Lt,
        3 => CompOp::Nl,
        4 => CompOp::Gt,
        5 => CompOp::Ng,
        6 => CompOp::Le,
        7 => CompOp::Ge,
        _ => return None,
    })
}

fn encode_join(j: &JoinDef) -> Vec<u8> {
    let mut b = vec![0u8; JOIN_HEAD + j.fields.len() * 20];
    b[0] = j.from;
    b[1] = j.to;
    put_u16(&mut b, 2, u16_of(j.fields.len()));
    b[8..18].fill(ccsid::SPACE);
    if let Some((name, desc)) = &j.dupseq {
        b[4] = 1;
        b[5] = u8::from(*desc);
        b[8..18].copy_from_slice(name.as_bytes());
    }
    for (i, (from, to)) in j.fields.iter().enumerate() {
        let at = JOIN_HEAD + i * 20;
        b[at..at + 10].copy_from_slice(from.as_bytes());
        b[at + 10..at + 20].copy_from_slice(to.as_bytes());
    }
    b
}

fn encode_format(def: &FormatDef) -> [u8; FORMAT_LEN] {
    let mut b = [0u8; FORMAT_LEN];
    b[..10].copy_from_slice(def.format.name.as_bytes());
    b[10..23].copy_from_slice(def.level_id.as_bytes());
    put_u16(&mut b, 24, u16_of(def.format.fields.len()));
    put_u16(&mut b, 26, u16_of(def.format.keys.len()));
    put_u32(&mut b, 28, def.format.record_len);
    put_u16(&mut b, 32, def.null_map_len);
    let mut flags = 0u16;
    if def.unique.is_some() {
        flags |= 1;
    }
    if def.unique == Some(NullKeys::Exclude) {
        flags |= 2;
    }
    flags |= match def.dup_order {
        None => 0,
        Some(DupOrder::Fifo) => 1 << 2,
        Some(DupOrder::Lifo) => 2 << 2,
        Some(DupOrder::Fcfo) => 3 << 2,
    };
    if def.logical.as_ref().is_some_and(|l| l.join) {
        flags |= 1 << 4;
    }
    put_u16(&mut b, 34, flags);
    put_u16(&mut b, 36, def.key_len);
    put_text(&mut b, 46, 50, def.format.text.as_deref().unwrap_or(""));
    b
}

fn date_code(d: DateFmt) -> u8 {
    match d {
        DateFmt::Iso => 0,
        DateFmt::Usa => 1,
        DateFmt::Eur => 2,
        DateFmt::Jis => 3,
        DateFmt::Mdy => 4,
        DateFmt::Dmy => 5,
        DateFmt::Ymd => 6,
        DateFmt::Jul => 7,
    }
}

fn date_of(code: u8) -> Option<DateFmt> {
    Some(match code {
        0 => DateFmt::Iso,
        1 => DateFmt::Usa,
        2 => DateFmt::Eur,
        3 => DateFmt::Jis,
        4 => DateFmt::Mdy,
        5 => DateFmt::Dmy,
        6 => DateFmt::Ymd,
        7 => DateFmt::Jul,
        _ => return None,
    })
}

fn time_code(t: TimeFmt) -> u8 {
    match t {
        TimeFmt::Hms => 0,
        TimeFmt::Iso => 1,
        TimeFmt::Usa => 2,
        TimeFmt::Eur => 3,
        TimeFmt::Jis => 4,
    }
}

fn time_of(code: u8) -> Option<TimeFmt> {
    Some(match code {
        0 => TimeFmt::Hms,
        1 => TimeFmt::Iso,
        2 => TimeFmt::Usa,
        3 => TimeFmt::Eur,
        4 => TimeFmt::Jis,
        _ => return None,
    })
}

fn encode_field(f: &FieldDef, null_index: Option<u16>) -> [u8; FIELD_LEN] {
    let mut b = [0u8; FIELD_LEN];
    b[..10].copy_from_slice(f.name.as_bytes());
    b[10] = f.data_type.letter() as u8;
    b[11] = f.decimals;
    let mut flags = 0u16;
    if f.allow_null {
        flags |= 1;
    }
    if f.varlen.is_some() {
        flags |= 2;
    }
    if f.usage == Some(Usage::Both) {
        flags |= 4;
    }
    put_u16(&mut b, 12, flags);
    put_u32(&mut b, 14, f.length);
    put_u32(&mut b, 18, f.byte_len);
    put_u32(&mut b, 22, f.offset);
    put_u16(&mut b, 26, f.ccsid.unwrap_or(0));
    if f.data_type == DataType::Date {
        b[28] = date_code(f.date_fmt.unwrap_or_default());
        b[29] = f.date_sep.map_or(0, |c| c as u8);
    }
    if f.data_type == DataType::Time {
        b[30] = time_code(f.time_fmt.unwrap_or_default());
        b[31] = f.time_sep.map_or(0, |c| c as u8);
    }
    b[32] = u8::from(f.precision == FloatPrecision::Double);
    b[33] = f.edit_code.as_ref().map_or(0, |e| e.code as u8);
    put_u16(
        &mut b,
        34,
        f.varlen
            .and_then(|v| v.allocated)
            .and_then(|a| u16::try_from(a).ok())
            .unwrap_or(0),
    );
    put_u16(&mut b, 36, null_index.unwrap_or(0xFFFF));
    put_text(&mut b, 38, 50, f.text.as_deref().unwrap_or(""));
    put_text(&mut b, 88, 30, f.alias.as_deref().unwrap_or(""));
    for i in 0..3 {
        put_text(
            &mut b,
            118 + i * 20,
            20,
            f.colhdg.get(i).map_or("", String::as_str),
        );
    }
    let (kind, bytes): (u8, Vec<u8>) = match &f.default {
        None => (0, Vec::new()),
        Some(DefaultValue::Null) => (1, Vec::new()),
        Some(DefaultValue::Quoted(q)) => (
            2,
            q.chars()
                .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
                .collect(),
        ),
        Some(DefaultValue::Hex(h)) => (3, h.clone()),
        Some(DefaultValue::Number(n)) => (4, n.as_bytes().to_vec()),
        Some(DefaultValue::Expr(e)) => (5, e.as_bytes().to_vec()),
    };
    let n = bytes.len().min(DFT_MAX);
    b[178] = kind;
    b[179] = u8::try_from(n).expect("at most 64");
    b[180..180 + n].copy_from_slice(&bytes[..n]);
    if let Some(id) = &f.identity {
        b[244] = 1 | u8::from(id.always) << 1 | u8::from(id.cycle) << 2;
        let clamp = |v: i64| i32::try_from(v).unwrap_or(if v < 0 { i32::MIN } else { i32::MAX });
        b[245..249].copy_from_slice(&clamp(id.start).to_le_bytes());
        b[249..253].copy_from_slice(&clamp(id.increment).to_le_bytes());
    }
    b
}

fn encode_key(k: &KeyField, key_off: u16, key_len: u16) -> [u8; KEY_LEN] {
    let mut b = [0u8; KEY_LEN];
    put_u16(&mut b, 0, u16_of(k.field));
    let transform: u16 = match k.transform {
        KeyTransform::Signed => 0,
        KeyTransform::Unsigned => 1,
        KeyTransform::Absval => 2,
        KeyTransform::Zone => 3,
        KeyTransform::Digit => 4,
    };
    let flags = u16::from(k.descend) | (transform << 1) | (u16::from(k.noaltseq) << 4);
    put_u16(&mut b, 2, flags);
    put_u16(&mut b, 4, key_off);
    put_u16(&mut b, 6, key_len);
    b
}

/// Decode formats from their sections.
pub fn decode(bytes: &[u8]) -> Result<Vec<FormatDef>> {
    let mut out: Vec<FormatDef> = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if at + SECTION_LEN > bytes.len() {
            return Err(corrupt("format section header cut short"));
        }
        let tag = get_u16(bytes, at);
        let len = usize::try_from(get_u32(bytes, at + 4)).expect("fits");
        at += SECTION_LEN;
        if at + len > bytes.len() {
            return Err(corrupt(format!(
                "format section {tag} of {len} bytes cut short"
            )));
        }
        let body = &bytes[at..at + len];
        at += len;
        if tag == TAG_FORMAT {
            out.push(decode_format(body)?);
            continue;
        }
        let def = out
            .last_mut()
            .ok_or_else(|| corrupt("section before format"))?;
        match tag {
            TAG_FIELD => def.format.fields.push(decode_field(body)?),
            TAG_KEY => def.format.keys.push(decode_key(body, &def.format.fields)?),
            TAG_BASEDON => logical_of(def).based_on.push(decode_basedon(body)?),
            TAG_SOURCE => logical_of(def).sources.push(decode_source(body)?),
            TAG_SELOMIT => {
                let g = decode_group(&def.format, body)?;
                logical_of(def).select_omit.push(g);
            }
            TAG_JOIN => logical_of(def).joins.push(decode_join(body)?),
            other => return Err(corrupt(format!("format section tag {other}"))),
        }
    }
    for def in &mut out {
        for (i, f) in def.format.fields.iter_mut().enumerate() {
            f.line = i;
        }
    }
    Ok(out)
}

fn logical_of(def: &mut FormatDef) -> &mut LogicalDef {
    def.logical.get_or_insert_with(|| LogicalDef {
        based_on: Vec::new(),
        sources: Vec::new(),
        select_omit: Vec::new(),
        joins: Vec::new(),
        join: false,
    })
}

fn decode_format(b: &[u8]) -> Result<FormatDef> {
    if b.len() != FORMAT_LEN {
        return Err(corrupt("format entry size"));
    }
    let name = name_of(b);
    let level_id = std::str::from_utf8(&b[10..23])
        .map_err(|_| corrupt("level id"))?
        .to_string();
    let flags = get_u16(b, 34);
    let unique = (flags & 1 != 0).then_some(if flags & 2 != 0 {
        NullKeys::Exclude
    } else {
        NullKeys::Include
    });
    let dup_order = match (flags >> 2) & 3 {
        1 => Some(DupOrder::Fifo),
        2 => Some(DupOrder::Lifo),
        3 => Some(DupOrder::Fcfo),
        _ => None,
    };
    let mut format = RecordFormat::new(name);
    format.record_len = get_u32(b, 28);
    format.text = get_text(b, 46, 50);
    let mut def = FormatDef {
        format,
        level_id,
        null_map_len: get_u16(b, 32),
        unique,
        dup_order,
        key_len: get_u16(b, 36),
        logical: None,
    };
    if flags & 16 != 0 {
        logical_of(&mut def).join = true;
    }
    Ok(def)
}

fn decode_field(b: &[u8]) -> Result<FieldDef> {
    if b.len() != FIELD_LEN {
        return Err(corrupt("field entry size"));
    }
    let name = name_of(b);
    let data_type = DataType::from_letter(char::from(b[10]))
        .ok_or_else(|| corrupt(format!("field {name} type {:#04x}", b[10])))?;
    let flags = get_u16(b, 12);
    let mut f = FieldDef::new(name, data_type, get_u32(b, 14), b[11]);
    f.allow_null = flags & 1 != 0;
    if flags & 2 != 0 {
        let alloc = get_u16(b, 34);
        f.varlen = Some(VarLen {
            allocated: (alloc != 0).then_some(u32::from(alloc)),
        });
    }
    f.usage = (flags & 4 != 0).then_some(Usage::Both);
    f.byte_len = get_u32(b, 18);
    f.offset = get_u32(b, 22);
    f.ccsid = Some(get_u16(b, 26)).filter(|c| *c != 0);
    if data_type == DataType::Date {
        f.date_fmt = Some(date_of(b[28]).ok_or_else(|| corrupt("date format"))?);
        f.date_sep = Some(char::from(b[29])).filter(|c| *c != '\0');
    }
    if data_type == DataType::Time {
        f.time_fmt = Some(time_of(b[30]).ok_or_else(|| corrupt("time format"))?);
        f.time_sep = Some(char::from(b[31])).filter(|c| *c != '\0');
    }
    f.precision = if b[32] == 1 {
        FloatPrecision::Double
    } else {
        FloatPrecision::Single
    };
    if b[33] != 0 {
        f.edit_code = Some(dds::EditCode {
            code: char::from(b[33]),
            modifier: None,
        });
    }
    f.text = get_text(b, 38, 50);
    f.alias = get_text(b, 88, 30);
    f.colhdg = (0..3)
        .filter_map(|i| get_text(b, 118 + i * 20, 20))
        .collect();
    let n = usize::from(b[179]);
    let dft = &b[180..180 + n.min(DFT_MAX)];
    f.default = match b[178] {
        0 => None,
        1 => Some(DefaultValue::Null),
        2 => Some(DefaultValue::Quoted(ccsid::from_ebcdic(dft))),
        3 => Some(DefaultValue::Hex(dft.to_vec())),
        4 => Some(DefaultValue::Number(
            String::from_utf8_lossy(dft).into_owned(),
        )),
        5 => Some(DefaultValue::Expr(
            String::from_utf8_lossy(dft).into_owned(),
        )),
        k => return Err(corrupt(format!("default kind {k}"))),
    };
    if b[244] & 1 != 0 {
        let i32_at = |o: usize| i32::from_le_bytes(b[o..o + 4].try_into().expect("4"));
        f.identity = Some(dds::IdentityDef {
            always: b[244] & 2 != 0,
            cycle: b[244] & 4 != 0,
            start: i64::from(i32_at(245)),
            increment: i64::from(i32_at(249)),
        });
    }
    Ok(f)
}

fn decode_key(b: &[u8], fields: &[FieldDef]) -> Result<KeyField> {
    if b.len() != KEY_LEN {
        return Err(corrupt("key entry size"));
    }
    let index = usize::from(get_u16(b, 0));
    let field = fields
        .get(index)
        .ok_or_else(|| corrupt(format!("key field index {index}")))?;
    let flags = get_u16(b, 2);
    let transform = match (flags >> 1) & 7 {
        0 => KeyTransform::Signed,
        1 => KeyTransform::Unsigned,
        2 => KeyTransform::Absval,
        3 => KeyTransform::Zone,
        4 => KeyTransform::Digit,
        t => return Err(corrupt(format!("key transform {t}"))),
    };
    Ok(KeyField {
        name: field.name,
        field: index,
        descend: flags & 1 != 0,
        transform,
        noaltseq: flags & 16 != 0,
        line: 0,
    })
}

fn decode_basedon(b: &[u8]) -> Result<(ObjectId, Name)> {
    if b.len() != BASEDON_LEN {
        return Err(corrupt("based-on entry size"));
    }
    Ok((ObjectId::from_bytes(&b[..16]), name_of(&b[16..26])))
}

fn decode_source(b: &[u8]) -> Result<FieldSource> {
    if b.len() < SOURCE_HEAD {
        return Err(corrupt("source entry size"));
    }
    let count = usize::from(get_u16(b, 2));
    if b.len() != SOURCE_HEAD + count * 10 || count == 0 {
        return Err(corrupt("source entry names"));
    }
    let names: Vec<Name> = (0..count)
        .map(|i| name_of(&b[SOURCE_HEAD + i * 10..]))
        .collect();
    let file = usize::from(b[1]);
    Ok(match b[0] {
        0 => FieldSource::Base {
            file,
            field: names[0],
        },
        1 => FieldSource::Concat {
            file,
            fields: names,
        },
        2 => FieldSource::Substring {
            file,
            field: names[0],
            start: u32::from(get_u16(b, 4)),
            length: u32::from(get_u16(b, 6)),
        },
        k => return Err(corrupt(format!("source kind {k}"))),
    })
}

fn decode_value(b: &[u8], at: &mut usize) -> Result<Literal> {
    if *at + VALUE_HEAD > b.len() {
        return Err(corrupt("select/omit value cut short"));
    }
    let kind = b[*at];
    let len = usize::from(get_u16(b, *at + 2));
    *at += VALUE_HEAD;
    if *at + len > b.len() {
        return Err(corrupt("select/omit value bytes cut short"));
    }
    let bytes = &b[*at..*at + len];
    *at += len;
    Ok(match kind {
        0 => Literal::Word(String::from_utf8_lossy(bytes).into_owned()),
        1 => Literal::Quoted(ccsid::from_ebcdic(bytes)),
        2 => Literal::Hex(bytes.to_vec()),
        k => return Err(corrupt(format!("select/omit value kind {k}"))),
    })
}

fn decode_group(format: &RecordFormat, b: &[u8]) -> Result<SelectOmit> {
    if b.len() < GROUP_LEN {
        return Err(corrupt("select/omit group size"));
    }
    let count = usize::from(get_u16(b, 2));
    let mut at = GROUP_LEN;
    let mut conditions = Vec::with_capacity(count);
    for _ in 0..count {
        if at + CONDITION_LEN > b.len() {
            return Err(corrupt("select/omit condition cut short"));
        }
        let index = usize::from(get_u16(b, at));
        let field = format
            .fields
            .get(index)
            .ok_or_else(|| corrupt(format!("select/omit field index {index}")))?;
        let test = b[at + 2];
        let op = b[at + 3];
        let values = usize::from(get_u16(b, at + 4));
        at += CONDITION_LEN;
        let mut lits = Vec::with_capacity(values);
        for _ in 0..values {
            lits.push(decode_value(b, &mut at)?);
        }
        let test = match (test, lits.len()) {
            (0, _) => Validity::Values(lits),
            (1, 2) => {
                let hi = lits.pop().expect("two");
                let lo = lits.pop().expect("two");
                Validity::Range(lo, hi)
            }
            (2, 1) => Validity::Comp(
                op_of(op).ok_or_else(|| corrupt("select/omit operator"))?,
                lits.pop().expect("one"),
            ),
            _ => return Err(corrupt("select/omit test")),
        };
        conditions.push(Condition {
            field: field.name,
            test,
            line: 0,
        });
    }
    Ok(SelectOmit {
        select: b[0] != 0,
        all: b[1] != 0,
        conditions,
        line: 0,
    })
}

fn decode_join(b: &[u8]) -> Result<JoinDef> {
    if b.len() < JOIN_HEAD {
        return Err(corrupt("join entry size"));
    }
    let pairs = usize::from(get_u16(b, 2));
    if b.len() != JOIN_HEAD + pairs * 20 {
        return Err(corrupt("join entry pairs"));
    }
    let dupseq = (b[4] != 0).then(|| (name_of(&b[8..18]), b[5] != 0));
    let fields = (0..pairs)
        .map(|i| {
            let at = JOIN_HEAD + i * 20;
            (name_of(&b[at..at + 10]), name_of(&b[at + 10..at + 20]))
        })
        .collect();
    Ok(JoinDef {
        from: b[0],
        to: b[1],
        fields,
        dupseq,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(FORMAT_LAYOUT.size, Some(FORMAT_LEN));
        assert_eq!(FIELD_LAYOUT.size, Some(FIELD_LEN));
        assert_eq!(KEY_LAYOUT.size, Some(KEY_LEN));
        assert_eq!(BASEDON_LAYOUT.size, Some(BASEDON_LEN));
    }
}
