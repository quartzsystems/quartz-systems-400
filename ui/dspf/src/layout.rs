// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The buffers a program exchanges with a record format. The output
//! buffer holds the fields with usage O, B, H, M and P in source order,
//! the input buffer those with I, B and H; without `INDARA` each starts
//! with one byte per indicator the format uses, in the order they
//! first appear, file-level keys first (option indicators in the output
//! buffer, response indicators in the input buffer); a subfile record's
//! input buffer holds its output fields too. Bytes follow the external type as the database lays its
//! records out. The format level identifier is the database's, over
//! the named fields in source order.

use dds::model::{DataType, FieldDef};
use qsname::Name;

use crate::model::{Buffer, BufferField, Field, Record, RecordLayout, TypeClass};

/// Fill in the record's buffers, indicator lists and level identifier.
/// `file_responses` are the response indicators file-level keys set;
/// they are in every record's input buffer.
pub fn lay_out(rec: &mut Record, indara: bool, file_responses: &[u8]) {
    let mut option: Vec<u8> = Vec::new();
    let mut response: Vec<u8> = file_responses.to_vec();
    for k in &rec.keywords {
        option.extend(k.cond.indicators());
    }
    for k in &rec.keys {
        option.extend(k.cond.indicators());
        response.extend(k.ind);
    }
    response.extend(rec.responses.iter().map(|r| r.0));
    for f in &rec.fields {
        option.extend(f.cond.indicators());
        for k in &f.keywords {
            option.extend(k.cond.indicators());
        }
        response.extend(f.responses.iter().map(|r| r.0));
    }
    // Each indicator once, where it first appears: file-level keys,
    // then the record's.
    first_seen(&mut option);
    first_seen(&mut response);
    let output = buffer(
        &rec.fields,
        if indara { &[] } else { &option },
        Field::in_output,
    );
    // A subfile record reads back its output fields too (CHAIN, READC).
    let input = buffer(
        &rec.fields,
        if indara { &[] } else { &response },
        if rec.kind == crate::model::RecordKind::Subfile {
            in_subfile_input
        } else {
            Field::in_input
        },
    );
    rec.layout = RecordLayout {
        output,
        input,
        option_inds: option,
        response_inds: response,
        level_id: level_id(&rec.fields),
    };
}

/// Keep each value's first appearance.
fn first_seen(v: &mut Vec<u8>) {
    let mut seen = [false; 256];
    v.retain(|n| !std::mem::replace(&mut seen[usize::from(*n)], true));
}

/// A subfile record's field in its input buffer: every one with bytes
/// in either buffer.
fn in_subfile_input(f: &Field) -> bool {
    f.in_input() || f.in_output() && !matches!(f.usage, crate::model::Usage::Message | crate::model::Usage::Program)
}

fn buffer(fields: &[Field], indicators: &[u8], keep: fn(&Field) -> bool) -> Buffer {
    let mut at = u32::try_from(indicators.len()).unwrap_or(0);
    let mut out = Vec::new();
    for (i, f) in fields.iter().enumerate() {
        if !keep(f) {
            continue;
        }
        out.push(BufferField {
            field: i,
            offset: at,
            len: f.bytes,
        });
        at += f.bytes;
    }
    Buffer {
        indicators: indicators.to_vec(),
        fields: out,
        len: at,
    }
}

/// A display field as the database describes a field, for the level
/// identifier: the name, the external type letter the database knows
/// the field's class by, its length and decimals. Display-only type
/// letters (`N`, `Y`, `X`, ...) map onto `A` or `S` by their buffer
/// representation, so a change of keyboard shift alone keeps the
/// identifier. Constants have no entry.
#[must_use]
pub fn as_field_def(f: &Field) -> Option<FieldDef> {
    let name = Name::parse(f.name.as_deref()?).ok()?;
    let data_type = match f.ty.class {
        TypeClass::Char => DataType::Char,
        TypeClass::Graphic => DataType::Graphic,
        TypeClass::Zoned => DataType::Zoned,
        TypeClass::Packed => DataType::Packed,
        TypeClass::Binary => DataType::Binary,
        TypeClass::Float => DataType::Float,
        TypeClass::Date => DataType::Date,
        TypeClass::Time => DataType::Time,
        TypeClass::Timestamp => DataType::Timestamp,
    };
    Some(FieldDef::new(name, data_type, f.length, f.decimals))
}

/// The 13-character level identifier of a format with these fields.
#[must_use]
pub fn level_id(fields: &[Field]) -> String {
    let defs: Vec<FieldDef> = fields.iter().filter_map(as_field_def).collect();
    db::fmt_level_id(&defs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cond::Cond;
    use crate::model::{CmdKey, Constant, FieldType, KeyName, Usage};

    fn field(name: &str, letter: char, len: u32, dec: u8, usage: Usage) -> Field {
        let ty = FieldType::from_letter(letter, dec > 0 || "SPBF".contains(letter)).unwrap();
        let mut f = Field::new(Some(name.into()), ty, len, dec, usage);
        f.bytes = ty.bytes(len);
        f
    }

    #[test]
    fn buffers_and_indicators() {
        let mut r = Record::new("REC".into(), 0);
        let mut c = Field::new(None, FieldType::char(), 5, 0, Usage::Output);
        c.constant = Some(Constant::Text("Title".into()));
        c.cond = Cond::ind(50, true);
        r.fields.push(c);
        r.fields.push(field("NAME", 'A', 20, 0, Usage::Both));
        r.fields.push(field("AMT", 'P', 7, 2, Usage::Output));
        r.fields.push(field("QTY", 'S', 5, 0, Usage::Input));
        r.fields.push(field("KEY", 'A', 4, 0, Usage::Hidden));
        r.fields.push(field("CNT", 'B', 9, 0, Usage::Program));
        r.fields[3].responses.push((44, 3));
        r.keys.push(CmdKey {
            key: KeyName::Cf(3),
            ind: Some(3),
            text: None,
            cond: Cond::ind(31, false),
            line: 0,
        });
        lay_out(&mut r, false, &[12]);
        let l = &r.layout;
        assert_eq!(l.option_inds, [31, 50]);
        // File-level keys' indicators come first.
        assert_eq!(l.response_inds, [12, 3, 44]);
        // Output: 2 indicator bytes, NAME 20, AMT 4, KEY 4, CNT 4.
        assert_eq!(l.output.indicators, [31, 50]);
        let offs: Vec<(usize, u32, u32)> = l
            .output
            .fields
            .iter()
            .map(|b| (b.field, b.offset, b.len))
            .collect();
        assert_eq!(offs, [(1, 2, 20), (2, 22, 4), (4, 26, 4), (5, 30, 4)]);
        assert_eq!(l.output.len, 34);
        // Input: 3 indicator bytes, NAME 20, QTY 5, KEY 4.
        let offs: Vec<(usize, u32, u32)> = l
            .input
            .fields
            .iter()
            .map(|b| (b.field, b.offset, b.len))
            .collect();
        assert_eq!(offs, [(1, 3, 20), (3, 23, 5), (4, 28, 4)]);
        assert_eq!(l.input.len, 32);
        assert_eq!(l.level_id.len(), 13);
        let id = l.level_id.clone();

        lay_out(&mut r, true, &[12]);
        assert!(r.layout.output.indicators.is_empty());
        assert_eq!(r.layout.output.len, 32);
        assert_eq!(r.layout.input.len, 29);
        assert_eq!(r.layout.input.field(3).unwrap().offset, 20);
        assert_eq!(r.layout.level_id, id, "INDARA does not change the level");
    }

    #[test]
    fn level_id_follows_the_database() {
        let a = [
            field("CUSNO", 'P', 7, 0, Usage::Both),
            field("NAME", 'A', 30, 0, Usage::Both),
        ];
        let defs: Vec<FieldDef> = a.iter().filter_map(as_field_def).collect();
        assert_eq!(level_id(&a), db::fmt_level_id(&defs));
        // The database's own test pins this value.
        assert_eq!(level_id(&a), "F138D74F083F9");
        let mut b = a.clone();
        b[1].ty = FieldType::from_letter('X', false).unwrap();
        assert_eq!(
            level_id(&b),
            level_id(&a),
            "keyboard shift is not part of the level"
        );
        b[1].length = 31;
        assert_ne!(level_id(&b), level_id(&a));
        let mut c = Field::new(None, FieldType::char(), 3, 0, Usage::Output);
        c.constant = Some(Constant::User);
        assert!(as_field_def(&c).is_none());
    }
}
