// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Printing a DDS record format: the record buffer's fields and the
//! format's constants laid out at their positions, conditioned by
//! option indicators, edited, on one line or on the lines the format
//! names. A format with no spacing keyword and no line numbers spaces
//! one line after (`SPACEA(1)`).
//!
//! Without `INDARA` the buffer starts with one byte (`'1'` on) for each
//! option indicator the format uses, in ascending order; with it the
//! indicators come apart from the buffer.

use dds::prtf::{Edit, PrtField, PrtFormat, PrtKind};
use qdecimal::{EditOpts, Fill, Shape, Zoned, edit_code, edit_word};

use crate::printer::Printer;

/// The date and time `DATE` and `TIME` print, as digits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    /// `MMDDYY`.
    pub date: String,
    /// `HHMMSS`.
    pub time: String,
}

const ON: u8 = 0xF1;

struct Item {
    col: usize,
    bytes: Vec<u8>,
    underline: bool,
    highlight: bool,
}

struct Group {
    line: u16,
    before: (u16, u16),
    after: (u16, u16),
    fields: Vec<(usize, usize)>,
}

fn ebcdic(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(0x40))
        .collect()
}

/// A numeric field's text, edited as the field says.
fn numeric(bytes: &[u8], digits: u8, scale: u8, edit: &Edit) -> Vec<u8> {
    let shape = Shape { digits, scale };
    let value = Zoned::new(digits, scale)
        .ok()
        .and_then(|z| z.decode(bytes).ok());
    let (Some(v), false) = (value, matches!(edit, Edit::None)) else {
        return bytes.to_vec();
    };
    let text = match edit {
        Edit::Code(c, fill) => {
            let opts = EditOpts {
                fill: match fill {
                    Some('*') => Fill::Asterisk,
                    Some(c) => Fill::Currency(*c),
                    None => Fill::Blank,
                },
                ..EditOpts::default()
            };
            edit_code(&v, shape, *c, &opts).ok()
        }
        Edit::Word(w) => edit_word(&v, shape, w).ok(),
        Edit::None => None,
    };
    text.map_or_else(|| bytes.to_vec(), |t| ebcdic(&t))
}

/// What a field prints, given the record and the page.
fn content(f: &PrtField, data: &[u8], page: u32, clock: &Clock) -> Vec<u8> {
    match &f.kind {
        PrtKind::Char(_) => data.to_vec(),
        PrtKind::Zoned(d, s) => numeric(data, *d, *s, &f.edit),
        PrtKind::Constant(t) => ebcdic(t),
        PrtKind::Date => {
            let d = &clock.date;
            if matches!(f.edit, Edit::Code('Y', _)) && d.len() == 6 {
                ebcdic(&format!("{}/{}/{}", &d[..2], &d[2..4], &d[4..]))
            } else {
                ebcdic(d)
            }
        }
        PrtKind::Time => {
            let t = &clock.time;
            if t.len() == 6 {
                ebcdic(&format!("{}:{}:{}", &t[..2], &t[2..4], &t[4..]))
            } else {
                ebcdic(t)
            }
        }
        PrtKind::PagNbr => ebcdic(&format!("{:>4}", page % 10_000)),
    }
}

/// The width a field takes, known before the record arrives.
fn width(f: &PrtField) -> usize {
    match &f.kind {
        PrtKind::Char(n) => usize::from(*n),
        PrtKind::Zoned(d, s) => match &f.edit {
            Edit::None => usize::from(*d),
            Edit::Word(w) => w.chars().count(),
            Edit::Code(c, _) => qdecimal::code_width(
                Shape {
                    digits: *d,
                    scale: *s,
                },
                *c,
                &EditOpts::default(),
            )
            .unwrap_or(usize::from(*d)),
        },
        PrtKind::Constant(t) => t.chars().count(),
        PrtKind::Date => {
            if matches!(f.edit, Edit::Code('Y', _)) {
                8
            } else {
                6
            }
        }
        PrtKind::Time => 8,
        PrtKind::PagNbr => 4,
    }
}

/// Lines: a new one for each line number and each field that spaces
/// or skips before; each with the fields on it and their columns.
fn lines(fmt: &PrtFormat, slices: &[(usize, Vec<u8>)]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (i, f) in fmt.fields.iter().enumerate() {
        let starts = groups
            .last()
            .is_none_or(|g| (f.line != 0 && f.line != g.line) || f.spacing.before());
        if starts {
            groups.push(Group {
                line: f.line,
                before: (f.spacing.skipb, f.spacing.spaceb),
                after: (0, 0),
                fields: Vec::new(),
            });
        }
        let g = groups.last_mut().expect("a line");
        g.after = (
            g.after.0.max(f.spacing.skipa),
            g.after.1.max(f.spacing.spacea),
        );
        g.fields.push((i, slices[i].0));
    }
    if groups.is_empty() {
        groups.push(Group {
            line: 0,
            before: (0, 0),
            after: (0, 0),
            fields: Vec::new(),
        });
    }
    groups
}

/// Print one line of a record: its fields that are conditioned on, then
/// the highlight and underline overprints.
fn print_line(
    p: &mut Printer,
    fmt: &PrtFormat,
    g: &Group,
    slices: &[(usize, Vec<u8>)],
    on: &[bool],
    (page, clock): (u32, &Clock),
) {
    let mut items = Vec::new();
    for (i, col) in &g.fields {
        let f = &fmt.fields[*i];
        if !f.cond.iter().all(|(n, want)| on[usize::from(*n)] == *want) {
            continue;
        }
        items.push(Item {
            col: *col,
            bytes: content(f, &slices[*i].1, page, clock),
            underline: f.underline,
            highlight: f.highlight,
        });
    }
    let end = items
        .iter()
        .map(|x| x.col + x.bytes.len())
        .max()
        .unwrap_or(0);
    let mut line = vec![0x40u8; end];
    let mut under = vec![0x40u8; end];
    let mut bold = vec![0x40u8; end];
    for x in &items {
        line[x.col..x.col + x.bytes.len()].copy_from_slice(&x.bytes);
        if x.underline {
            under[x.col..x.col + x.bytes.len()].fill(0x6D);
        }
        if x.highlight {
            bold[x.col..x.col + x.bytes.len()].copy_from_slice(&x.bytes);
        }
    }
    p.write(&line, crate::Control::default());
    if bold.iter().any(|b| *b != 0x40) {
        p.overprint(&bold);
    }
    if under.iter().any(|b| *b != 0x40) {
        p.overprint(&under);
    }
}

/// Print one record of `fmt` from `buf`; `inds` are the indicators when
/// the file has `INDARA`. Whether the carriage is at or past the
/// overflow line afterwards.
pub fn print_format(
    p: &mut Printer,
    fmt: &PrtFormat,
    indara: bool,
    inds: &[bool],
    buf: &[u8],
    clock: &Clock,
) -> bool {
    let used = fmt.indicators();
    let mut at = 0;
    let on: Vec<bool> = if indara {
        (0..=99)
            .map(|n| inds.get(n).copied().unwrap_or(false))
            .collect()
    } else {
        let mut v = vec![false; 100];
        for n in &used {
            v[usize::from(*n)] = buf.get(at) == Some(&ON);
            at += 1;
        }
        v
    };
    // Where each field's data is, and where it prints.
    let mut slices = Vec::new();
    let mut prev_end = 0usize;
    let mut prev_line = 0u16;
    for f in &fmt.fields {
        let n = f.buffer_len();
        let data = buf.get(at..at + n).unwrap_or(&[]).to_vec();
        at += n;
        if f.line != 0 && f.line != prev_line {
            prev_end = 0;
            prev_line = f.line;
        }
        let col = if f.relative {
            prev_end + usize::from(f.pos)
        } else {
            usize::from(f.pos).saturating_sub(1)
        };
        prev_end = col + width(f);
        slices.push((col, data));
    }
    let groups = lines(fmt, &slices);
    let numbered = fmt.fields.iter().any(|f| f.line != 0);
    let s = fmt.spacing;
    let last = groups.len() - 1;
    for (gi, g) in groups.iter().enumerate() {
        if gi == 0 {
            if s.skipb > 0 {
                p.skip(s.skipb);
            }
            p.space(s.spaceb);
        }
        if g.before.0 > 0 {
            p.skip(g.before.0);
        }
        if g.line > 0 {
            p.skip(g.line);
        } else if gi > 0 && g.before.1 == 0 {
            p.space(1);
        }
        p.space(g.before.1);
        let page = p.pages.max(1);
        print_line(p, fmt, g, &slices, &on, (page, clock));
        if g.after.0 > 0 {
            p.skip(g.after.0);
        }
        p.space(g.after.1);
        if gi == last {
            let default = !s.any() && !numbered && !fmt.fields.iter().any(|f| f.spacing.any());
            p.space(if default { 1 } else { s.spacea });
            if s.skipa > 0 {
                p.skip(s.skipa);
            }
        }
    }
    p.line() >= p.fmt.ovrflw
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printer::PageFormat;
    use crate::scs::decode;
    use dds::prtf::{PrtfDefinition, Spacing};

    fn fld(kind: PrtKind, line: u16, pos: u16, relative: bool) -> PrtField {
        PrtField {
            name: String::new(),
            kind,
            line,
            pos,
            relative,
            cond: Vec::new(),
            edit: Edit::None,
            underline: false,
            highlight: false,
            spacing: Spacing::default(),
        }
    }

    fn zoned(v: &str) -> Vec<u8> {
        let mut b: Vec<u8> = v.bytes().map(|d| 0xF0 + (d - b'0')).collect();
        if let Some(l) = b.last_mut() {
            *l = (*l & 0x0F) | 0xF0;
        }
        b
    }

    #[test]
    fn header_detail_total() {
        let mut title = fld(PrtKind::Constant("REPORT".into()), 0, 1, false);
        title.underline = true;
        let mut date = fld(PrtKind::Date, 0, 20, false);
        date.edit = Edit::Code('Y', None);
        let header = PrtFormat {
            name: "HEADER".into(),
            text: None,
            spacing: Spacing {
                skipb: 1,
                spacea: 2,
                ..Spacing::default()
            },
            fields: vec![
                title,
                date,
                fld(PrtKind::Constant("PAGE".into()), 0, 40, false),
                fld(PrtKind::PagNbr, 0, 1, true),
            ],
        };
        let mut amount = fld(PrtKind::Zoned(7, 2), 0, 2, true);
        amount.edit = Edit::Code('J', None);
        let mut star = fld(PrtKind::Constant("*".into()), 0, 1, true);
        star.cond = vec![(50, true)];
        let detail = PrtFormat {
            name: "DETAIL".into(),
            text: None,
            spacing: Spacing::default(),
            fields: vec![fld(PrtKind::Char(6), 0, 5, false), amount, star],
        };
        let def = PrtfDefinition {
            indara: false,
            text: None,
            formats: vec![header, detail],
        };
        let clock = Clock {
            date: "102126".into(),
            time: "120000".into(),
        };
        let mut p = Printer::new(PageFormat::default());
        print_format(&mut p, &def.formats[0], false, &[], &[], &clock);
        let mut rec = vec![0xF1];
        rec.extend(ccsid::to_ebcdic("ACME  ").unwrap());
        rec.extend(zoned("0012345"));
        print_format(&mut p, &def.formats[1], false, &[], &rec, &clock);
        rec[0] = 0xF0;
        print_format(&mut p, &def.formats[1], false, &[], &rec, &clock);
        assert_eq!(p.records, 3, "overprints are not records");
        let (pages, _) = decode(&p.take());
        let l = &pages[0].lines;
        assert_eq!(l[&1], "REPORT             10/21/26            PAGE    1");
        assert_eq!(l[&3], "    ACME       123.45  *");
        assert_eq!(l[&4], "    ACME       123.45");
    }

    #[test]
    fn line_numbers_and_highlight() {
        let mut name = fld(PrtKind::Constant("NAME".into()), 5, 10, false);
        name.highlight = true;
        let f = PrtFormat {
            name: "F".into(),
            text: None,
            spacing: Spacing::default(),
            fields: vec![
                fld(PrtKind::Constant("TOP".into()), 2, 1, false),
                name,
                fld(PrtKind::Char(3), 5, 2, true),
            ],
        };
        let mut p = Printer::new(PageFormat::default());
        let over = print_format(
            &mut p,
            &f,
            true,
            &[],
            &ccsid::to_ebcdic("XYZ").unwrap(),
            &Clock::default(),
        );
        assert!(!over);
        let (pages, _) = decode(&p.take());
        assert_eq!(pages[0].lines[&2], "TOP");
        assert_eq!(pages[0].lines[&5], "         NAME  XYZ");
    }
}
