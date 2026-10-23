// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Report Layout Utility's page image: a printer file's DDS laid
//! out as the lines each record format prints, fields shown as masks
//! (`X` for characters, `9` for digits), and the DDS written back from
//! the edited image. Constants are what the operator types; a field
//! stays where its mask is left whole.

use std::fmt::Write as _;

use dds::prtf::{Edit, PrtField, PrtFormat, PrtKind, PrtfDefinition, Spacing};

/// Widest line the image keeps.
pub const MAX_WIDTH: usize = 198;

/// What sits on an image line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    /// Column, from 1.
    pub col: usize,
    /// The field it shows (an index into its format's fields), or a
    /// constant when `None`.
    pub field: Option<usize>,
    /// The characters shown.
    pub text: String,
}

/// One line of the image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The record format it belongs to (an index).
    pub format: usize,
    /// The characters, up to [`MAX_WIDTH`].
    pub text: String,
}

/// A printer file being edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// The definition the image came from: formats and their fields.
    pub def: PrtfDefinition,
    /// The lines, format by format.
    pub lines: Vec<Line>,
    /// Changed since it was read.
    pub changed: bool,
}

/// The mask a field shows.
#[must_use]
pub fn mask(f: &PrtField) -> String {
    match &f.kind {
        PrtKind::Char(n) => "X".repeat(usize::from(*n)),
        PrtKind::Zoned(d, s) => {
            let d = usize::from(*d);
            let s = usize::from(*s);
            match &f.edit {
                Edit::Word(w) => w.chars().map(|c| if c == ' ' { '9' } else { c }).collect(),
                _ if s > 0 => format!("{}.{}", "9".repeat(d - s), "9".repeat(s)),
                _ => "9".repeat(d),
            }
        }
        PrtKind::Constant(t) => t.clone(),
        PrtKind::Date => "99/99/99".into(),
        PrtKind::Time => "99:99:99".into(),
        PrtKind::PagNbr => "9999".into(),
    }
}

fn put(line: &mut Vec<char>, col: usize, text: &str) {
    let need = (col - 1 + text.chars().count()).min(MAX_WIDTH);
    if line.len() < need {
        line.resize(need, ' ');
    }
    for (i, ch) in text.chars().enumerate() {
        if let Some(c) = line.get_mut(col - 1 + i) {
            *c = ch;
        }
    }
}

/// Each format's lines with what is placed on them.
#[must_use]
pub fn layout(f: &PrtFormat) -> Vec<Vec<Placed>> {
    let mut lines: Vec<Vec<Placed>> = vec![Vec::new()];
    let mut line_no = 0u16;
    let mut end = 0usize;
    for (i, fld) in f.fields.iter().enumerate() {
        let new_line = (fld.spacing.before() || (fld.line != 0 && fld.line != line_no))
            && !lines.last().is_some_and(Vec::is_empty);
        if new_line {
            let gap = usize::from(fld.spacing.spaceb.max(1)).saturating_sub(1);
            for _ in 0..gap {
                lines.push(Vec::new());
            }
            lines.push(Vec::new());
            end = 0;
        }
        if fld.line != 0 {
            line_no = fld.line;
        }
        let col = if fld.relative {
            end + usize::from(fld.pos) + 1
        } else {
            usize::from(fld.pos.max(1))
        };
        let text = mask(fld);
        end = col + text.chars().count() - 1;
        let field = (!matches!(fld.kind, PrtKind::Constant(_))).then_some(i);
        lines.last_mut().expect("a line").push(Placed { col, field, text });
    }
    lines
}

impl Image {
    /// The image of a definition.
    #[must_use]
    pub fn new(def: PrtfDefinition) -> Self {
        let mut lines = Vec::new();
        for (fi, f) in def.formats.iter().enumerate() {
            for placed in layout(f) {
                let mut l = Vec::new();
                for p in &placed {
                    put(&mut l, p.col, &p.text);
                }
                lines.push(Line {
                    format: fi,
                    text: l.into_iter().collect(),
                });
            }
        }
        Self {
            def,
            lines,
            changed: false,
        }
    }

    /// Replace a line's characters from `from` (a column, from 1).
    pub fn type_over(&mut self, index: usize, from: usize, text: &str) {
        let Some(line) = self.lines.get_mut(index) else {
            return;
        };
        let mut l: Vec<char> = line.text.chars().collect();
        let old = l.clone();
        put(&mut l, from, text);
        while l.last() == Some(&' ') {
            l.pop();
        }
        let mut cmp = old;
        while cmp.last() == Some(&' ') {
            cmp.pop();
        }
        if l != cmp {
            line.text = l.into_iter().collect();
            self.changed = true;
        }
    }

    /// A blank line after `index`, in the same format.
    pub fn insert_after(&mut self, index: usize) {
        let format = self.lines.get(index).map_or(0, |l| l.format);
        self.lines.insert(
            (index + 1).min(self.lines.len()),
            Line {
                format,
                text: String::new(),
            },
        );
        self.changed = true;
    }

    /// Delete a line, keeping one line in each format.
    pub fn delete(&mut self, index: usize) {
        let Some(l) = self.lines.get(index) else {
            return;
        };
        let f = l.format;
        if self.lines.iter().filter(|l| l.format == f).count() > 1 {
            self.lines.remove(index);
            self.changed = true;
        }
    }

    /// The DDS of the image.
    #[must_use]
    pub fn dds(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.def.indara {
            out.push(spec("", "", "", "INDARA"));
        }
        if let Some(t) = &self.def.text {
            out.push(spec("", "", "", &format!("TEXT({})", quote(t))));
        }
        for (fi, f) in self.def.formats.iter().enumerate() {
            out.push(record(f));
            let lines: Vec<&Line> = self.lines.iter().filter(|l| l.format == fi).collect();
            let mut used = vec![false; f.fields.len()];
            let mut blank = 0u16;
            let mut first = true;
            for l in lines {
                let items = items(f, &l.text, &mut used);
                if items.is_empty() {
                    blank += 1;
                    continue;
                }
                for (n, (col, item)) in items.iter().enumerate() {
                    let mut kws = Vec::new();
                    if n == 0 && !first {
                        let mut s = blank + 1;
                        while s > 3 {
                            kws.push("SPACEB(3)".to_string());
                            s -= 3;
                        }
                        kws.insert(0, format!("SPACEB({s})"));
                    }
                    out.extend(item_lines(f, *col, item, &kws));
                }
                first = false;
                blank = 0;
            }
        }
        out
    }
}

/// What an image line holds, left to right.
enum Item {
    Field(usize),
    Constant(String),
}

/// The fields whose masks are whole on the line, the rest as constants
/// (runs of text split at two blanks or more).
fn items(f: &PrtFormat, text: &str, used: &mut [bool]) -> Vec<(usize, Item)> {
    let chars: Vec<char> = text.chars().collect();
    let mut taken = vec![false; chars.len()];
    let mut out = Vec::new();
    for placed in layout(f).iter().flatten() {
        let Some(i) = placed.field else { continue };
        if used[i] {
            continue;
        }
        let m: Vec<char> = placed.text.chars().collect();
        let s = placed.col - 1;
        if chars.get(s..s + m.len()) == Some(&m[..]) {
            used[i] = true;
            taken[s..s + m.len()].iter_mut().for_each(|t| *t = true);
            out.push((placed.col, Item::Field(i)));
        }
    }
    let mut c = 0;
    while c < chars.len() {
        if taken[c] || chars[c] == ' ' {
            c += 1;
            continue;
        }
        let start = c;
        let mut end = c;
        while c < chars.len() && !taken[c] {
            if chars[c] != ' ' {
                end = c;
            } else if chars.get(c + 1).is_none_or(|n| *n == ' ') {
                break;
            }
            c += 1;
        }
        let t: String = chars[start..=end].iter().collect();
        out.push((start + 1, Item::Constant(t)));
    }
    out.sort_by_key(|(c, _)| *c);
    out
}

fn quote(t: &str) -> String {
    format!("'{}'", t.replace('\'', "''"))
}

/// An A-spec: name type (17), name (19-28), the middle (29-44) and the
/// keywords (45-80).
fn spec(ty: &str, name: &str, mid: &str, kw: &str) -> String {
    let mut s = format!("     A          {ty:<1} {name:<10}{mid:<16}{kw}");
    while s.ends_with(' ') {
        s.pop();
    }
    s
}

fn spacing_kws(s: Spacing) -> Vec<String> {
    let mut v = Vec::new();
    if s.skipb > 0 {
        v.push(format!("SKIPB({})", s.skipb));
    }
    if s.spaceb > 0 {
        v.push(format!("SPACEB({})", s.spaceb));
    }
    if s.spacea > 0 {
        v.push(format!("SPACEA({})", s.spacea));
    }
    if s.skipa > 0 {
        v.push(format!("SKIPA({})", s.skipa));
    }
    v
}

fn record(f: &PrtFormat) -> String {
    let mut kws = spacing_kws(f.spacing);
    if let Some(t) = &f.text {
        kws.push(format!("TEXT({})", quote(t)));
    }
    let mut s = spec("R", &f.name, "", kws.first().map_or("", String::as_str));
    for k in kws.iter().skip(1) {
        s.push('\n');
        s.push_str(&spec("", "", "", k));
    }
    s
}

/// The lines of one field or constant at a column.
fn item_lines(f: &PrtFormat, col: usize, item: &Item, extra: &[String]) -> Vec<String> {
    let pos = format!("{:>3}", col.min(255));
    let mut kws: Vec<String> = extra.to_vec();
    let (name, mid, cond) = match item {
        Item::Constant(t) => {
            kws.insert(0, quote(t));
            (String::new(), format!("{:<13}{pos}", ""), String::new())
        }
        Item::Field(i) => {
            let fld = &f.fields[*i];
            let (len, ty, dec) = match &fld.kind {
                PrtKind::Char(n) => (n.to_string(), "A", String::new()),
                PrtKind::Zoned(d, s) => (d.to_string(), "S", s.to_string()),
                PrtKind::Date | PrtKind::Time | PrtKind::PagNbr => {
                    (String::new(), "", String::new())
                }
                PrtKind::Constant(_) => unreachable!("fields only"),
            };
            match fld.kind {
                PrtKind::Date => kws.insert(0, "DATE".into()),
                PrtKind::Time => kws.insert(0, "TIME".into()),
                PrtKind::PagNbr => kws.insert(0, "PAGNBR".into()),
                _ => {}
            }
            match &fld.edit {
                Edit::None => {}
                Edit::Code(c, None) => kws.push(format!("EDTCDE({c})")),
                Edit::Code(c, Some(fill)) => kws.push(format!("EDTCDE({c} {fill})")),
                Edit::Word(w) => kws.push(format!("EDTWRD({})", quote(w))),
            }
            if fld.underline {
                kws.push("UNDERLINE".into());
            }
            if fld.highlight {
                kws.push("HIGHLIGHT".into());
            }
            kws.extend(spacing_kws(Spacing {
                skipb: 0,
                spaceb: 0,
                ..fld.spacing
            }));
            let mut cond = String::new();
            for (n, on) in fld.cond.iter().take(3) {
                cond.push(if *on { ' ' } else { 'N' });
                let _ = write!(cond, "{n:02}");
            }
            (
                fld.name.clone(),
                format!(" {len:>5}{ty:<1}{dec:>2} {:<3}{pos}", ""),
                cond,
            )
        }
    };
    let first = kws.first().cloned().unwrap_or_default();
    let mut line = spec("", &name, &mid, &first);
    if !cond.is_empty() {
        let mut c: Vec<char> = line.chars().collect();
        for (i, ch) in cond.chars().enumerate() {
            if let Some(x) = c.get_mut(7 + i) {
                *x = ch;
            }
        }
        line = c.into_iter().collect();
    }
    let mut out = vec![line];
    for k in kws.iter().skip(1) {
        out.push(spec("", "", "", k));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dds::compile::CompileOpts;
    use dds::refs::NoRefs;
    use dds::source::Source;

    fn src() -> String {
        [
            spec("R", "HEAD", "", "SKIPB(1)"),
            spec("", "", &format!("{:<13}  5", ""), "'Customer list'"),
            spec("", "", &format!("{:<13} 40", ""), "DATE"),
            spec("R", "LINE", "", "SPACEB(1)"),
            spec("", "ID", &format!(" {:>5}S{:>2} {:<3}{:>3}", 5, 0, "", 5), ""),
            spec("", "NAME", &format!(" {:>5}A{:>2} {:<3}{:>3}", 20, "", "", "+2"), ""),
        ]
        .join("
")
    }

    fn def(text: &str) -> PrtfDefinition {
        let (s, _) = Source::from_text(text);
        dds::prtf::compile_prtf(&s, &NoRefs, &CompileOpts::default())
            .definition
            .expect("compiles")
    }

    #[test]
    fn lays_out() {
        let img = Image::new(def(&src()));
        assert_eq!(img.lines.len(), 2);
        assert_eq!(img.lines[0].text.trim_start(), format!("Customer list{}99/99/99", " ".repeat(22)));
        assert_eq!(&img.lines[1].text[4..9], "99999");
        assert_eq!(&img.lines[1].text[11..31], &"X".repeat(20));
    }

    #[test]
    fn writes_back() {
        let mut img = Image::new(def(&src()));
        img.type_over(0, 5, "Client list  ");
        assert!(img.changed);
        img.insert_after(1);
        img.type_over(2, 3, "End");
        let dds = img.dds().join("\n");
        let again = def(&dds);
        let img2 = Image::new(again);
        assert_eq!(img2.lines[0].text, img.lines[0].text);
        assert_eq!(img2.lines[1].text, img.lines[1].text);
        assert_eq!(img2.lines[2].text.trim(), "End");
        assert!(dds.contains("'Client list'"));
        assert!(dds.contains("NAME"));
    }

    #[test]
    fn broken_mask_is_text() {
        let mut img = Image::new(def(&src()));
        img.type_over(1, 12, "Y");
        let dds = img.dds().join("\n");
        assert!(!dds.contains(" NAME "));
        assert!(dds.contains("'YXXXXXXXXXXXXXXXXXXX'"));
    }
}
