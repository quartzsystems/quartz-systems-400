// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing a member back. A record nothing changed in is written as it
//! was read, line for line; in a changed record each unchanged element
//! keeps its lines, a moved one has only its location rewritten, a
//! reshaped one its columns 19-38, and new ones are generated the way
//! SDA lays them out. A changed record's `%%TS` stamp is renewed.

use dds::lex::Keyword;

use super::line::Line;
use super::member::{Element, Kind, Member, Record};

/// Who saved, and when: the `%%TS` comment SDA keeps per record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// `yyyymmdd`.
    pub date: String,
    /// `hhmmss`.
    pub time: String,
    /// The user.
    pub user: String,
    /// The release, `VxRyMz`.
    pub release: String,
}

impl Stamp {
    /// The stamp line.
    #[must_use]
    pub fn line(&self) -> Line {
        let mut l = Line::spec();
        l.set(7, 12, "*%%TS");
        l.set(14, 15, "SD");
        l.set(18, 25, &self.date);
        l.set(28, 33, &self.time);
        l.set(36, 45, &self.user);
        l.set(48, 57, &format!("REL-{}", self.release));
        l.set(60, 67, "QS400");
        l
    }
}

fn is_stamp(l: &Line) -> bool {
    l.is_comment() && l.get(8, 11) == "%%TS"
}

/// The member's lines.
#[must_use]
pub fn write(m: &Member, stamp: &Stamp) -> Vec<String> {
    let mut out: Vec<Line> = Vec::new();
    if m.head.is_empty() {
        out.push(stamp.line());
        let mut ec = Line::spec();
        ec.set(7, 12, "*%%EC");
        out.push(ec);
        if m.keywords.is_empty() {
            out.extend(keyword_lines(&[dspsiz()]));
        } else {
            out.extend(keyword_lines(&m.keywords));
        }
    } else if m.orig_keywords.as_ref() != Some(&m.keywords) {
        let mut head = m.head.clone();
        edit_keywords(&mut head, 0, m.orig_keywords.as_deref().unwrap_or_default(), &m.keywords);
        for l in head {
            out.push(if is_stamp(&l) { stamp.line() } else { l });
        }
    } else {
        out.extend(m.head.iter().cloned());
    }
    for r in &m.records {
        out.extend(record_lines(r, stamp));
    }
    out.iter().map(Line::text).collect()
}

/// `DSPSIZ(24 80 *DS3)`, a new member's file-level keyword.
#[must_use]
pub fn dspsiz() -> Keyword {
    use dds::lex::Parm;
    Keyword {
        name: "DSPSIZ".into(),
        parms: vec![
            Parm::Word("24".into()),
            Parm::Word("80".into()),
            Parm::Word("*DS3".into()),
        ],
        line: 0,
    }
}

/// One line per keyword, from column 45, continued with `-` when long.
fn keyword_lines(keywords: &[Keyword]) -> Vec<Line> {
    let mut out = Vec::new();
    for k in keywords {
        out.extend(function_lines(&k.to_string()));
    }
    out
}

/// Text in the keyword area over as many lines as it needs.
pub(crate) fn function_lines(text: &str) -> Vec<Line> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        let mut l = Line::spec();
        let rest = chars.len() - i;
        if rest <= 36 {
            l.set(45, 80, &chars[i..].iter().collect::<String>());
            out.push(l);
            return out;
        }
        l.set(45, 79, &chars[i..i + 35].iter().collect::<String>());
        l.set(80, 80, "-");
        out.push(l);
        i += 35;
    }
}

fn record_lines(r: &Record, stamp: &Stamp) -> Vec<Line> {
    let mut out = Vec::new();
    if !r.is_changed() {
        out.extend(r.head.iter().cloned());
        for e in &r.elements {
            out.extend(e.lines.iter().cloned());
        }
        out.extend(r.tail.iter().cloned());
        return out;
    }
    if r.head.is_empty() {
        let mut l = Line::spec();
        l.set(17, 17, "R");
        l.set(19, 28, &r.name);
        out.push(l);
        out.extend(keyword_lines(&r.keywords));
        out.push(stamp.line());
    } else {
        let mut head = r.head.clone();
        let before = r.orig_keywords.as_deref().unwrap_or_default();
        if before != r.keywords.as_slice() {
            edit_keywords(&mut head, r.head_base, before, &r.keywords);
        }
        let mut stamped = false;
        for mut l in head {
            if is_stamp(&l) {
                l = stamp.line();
                stamped = true;
            } else if l.name_type() == 'R' {
                l.set(19, 28, &r.name);
            }
            out.push(l);
        }
        if !stamped {
            let at = out
                .iter()
                .rposition(|l| !l.is_comment())
                .map_or(out.len(), |i| i + 1);
            out.insert(at, stamp.line());
        }
    }
    for e in &r.elements {
        if e.is_changed() {
            out.extend(element_lines(e));
        } else {
            out.extend(e.lines.iter().cloned());
        }
    }
    out.extend(r.tail.iter().cloned());
    out
}

/// A changed element's lines: edits on its own lines where it has
/// some, else generated.
#[must_use]
pub fn element_lines(e: &Element) -> Vec<Line> {
    let Some(orig) = &e.orig else {
        return generate(e);
    };
    let mut lines = e.lines.clone();
    if orig.keywords != e.keywords {
        edit_keywords(&mut lines, e.base, &orig.keywords, &e.keywords);
    }
    let Some(f) = lines.iter().position(|l| !l.is_comment()) else {
        return generate(e);
    };
    match (&orig.kind, &e.kind) {
        (Kind::Field { .. }, Kind::Field { .. }) if orig.kind != e.kind => {
            shape(&mut lines[f], e);
        }
        (Kind::Constant { text: a, .. }, Kind::Constant { text: b, keyword: None }) if a != b => {
            replace_constant(&mut lines, f, b);
        }
        (a, b) if std::mem::discriminant(a) != std::mem::discriminant(b) || a != b => {
            let comments: Vec<Line> = lines[..f].to_vec();
            let mut out = comments;
            out.extend(generate(e));
            return out;
        }
        _ => {}
    }
    if orig.pos != e.pos || orig.kind != e.kind {
        locate(&mut lines[f], e);
    }
    edit_list(&mut lines, "DSPATR", &orig.attrs, &e.attrs);
    edit_list(&mut lines, "COLOR", &orig.colors, &e.colors);
    lines
}

/// Columns 39-44 from the element's position.
fn locate(l: &mut Line, e: &Element) {
    match e.pos {
        Some((r, c)) if e.displayed() => {
            l.set_right(39, 41, &r.to_string());
            l.set_right(42, 44, &c.to_string());
        }
        _ => l.set(39, 44, ""),
    }
}

/// Columns 19-38 from a field's shape.
fn shape(l: &mut Line, e: &Element) {
    let Kind::Field {
        name,
        usage,
        ty,
        len,
        dec,
        reference,
    } = &e.kind
    else {
        return;
    };
    l.set(19, 28, name);
    l.set(29, 29, if *reference { "R" } else { "" });
    if *reference && *len == 0 {
        l.set(30, 37, "");
    } else {
        l.set_right(30, 34, &len.to_string());
        l.set(35, 35, &ty.to_string());
        match dec {
            Some(d) => l.set_right(36, 37, &d.to_string()),
            None => l.set(36, 37, ""),
        }
    }
    l.set(38, 38, &usage.letter().to_string());
}

/// Keyword changes on existing lines: a line whose keywords all went
/// is dropped with its continuation lines (or, holding a name or a
/// location, keeps those and loses its keyword area); new keywords go
/// on lines of their own at the end. `base` is the member line of
/// `lines[0]`, the lexer's keyword lines count from the member's start.
pub fn edit_keywords(lines: &mut Vec<Line>, base: usize, before: &[Keyword], after: &[Keyword]) {
    let text = |k: &Keyword| k.to_string();
    let kept: Vec<String> = after.iter().map(text).collect();
    let had: Vec<String> = before.iter().map(text).collect();
    let starts = |i: usize| before.iter().filter(move |k| k.line.checked_sub(base) == Some(i));
    let mut drop = vec![false; lines.len()];
    let mut clear = vec![false; lines.len()];
    for i in 0..lines.len() {
        if starts(i).next().is_none() || starts(i).any(|k| kept.contains(&text(k))) {
            continue;
        }
        // The keyword's continuation lines: no keyword starts there.
        let mut j = i + 1;
        while j < lines.len()
            && starts(j).next().is_none()
            && !lines[j].is_comment()
            && lines[j].blank(17, 44)
            && !lines[j].functions().is_empty()
            && continues(&lines[j - 1])
        {
            j += 1;
        }
        let positional = !lines[i].blank(17, 44);
        for k in i..j {
            if k == i && positional {
                clear[k] = true;
            } else {
                drop[k] = true;
            }
        }
    }
    for (i, l) in lines.iter_mut().enumerate() {
        if clear[i] {
            l.set(45, 80, "");
        }
    }
    let mut i = 0;
    lines.retain(|_| {
        let keep = !drop[i];
        i += 1;
        keep
    });
    for k in after.iter().filter(|k| !had.contains(&text(k))) {
        lines.extend(function_lines(&text(k)));
    }
}

/// Whether a line's keyword area ends in a continuation character.
fn continues(l: &Line) -> bool {
    matches!(l.functions().chars().last(), Some('-' | '+'))
}

/// Swap a constant's quoted text on its lines, keeping what follows
/// the closing quote.
fn replace_constant(lines: &mut Vec<Line>, f: usize, text: &str) {
    // Find where the quoted text ends.
    let mut started = false;
    let mut end_line = f;
    let mut rest = String::new();
    'outer: for (k, l) in lines.iter().enumerate().skip(f) {
        let area: Vec<char> = l.functions().chars().collect();
        let mut i = 0;
        while i < area.len() {
            let c = area[i];
            if !started {
                started = c == '\'';
                i += 1;
                continue;
            }
            if c == '\'' {
                if area.get(i + 1) == Some(&'\'') {
                    i += 2;
                    continue;
                }
                end_line = k;
                rest = area[i + 1..].iter().collect::<String>().trim().to_string();
                break 'outer;
            }
            i += 1;
        }
        end_line = k;
    }
    let first = lines[f].clone();
    let mut new = function_lines(&quote(text));
    if let Some(n) = new.first_mut() {
        n.set(1, 44, &first.get(1, 44));
    }
    if !rest.is_empty() {
        new.extend(function_lines(&rest));
    }
    lines.splice(f..=end_line, new);
}

/// `'text'` with quotes doubled.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Add and drop single-value lines of `DSPATR` or `COLOR`.
fn edit_list(lines: &mut Vec<Line>, kw: &str, before: &[String], after: &[String]) {
    for gone in before.iter().filter(|a| !after.contains(a)) {
        let want = format!("{kw}({gone})");
        if let Some(i) = lines
            .iter()
            .position(|l| !l.is_comment() && l.blank(7, 44) && l.functions().trim() == want)
        {
            lines.remove(i);
        } else {
            // The value shares a line: take it out of the parameters.
            for l in lines.iter_mut().filter(|l| !l.is_comment() && l.blank(8, 16)) {
                let f = l.functions();
                if let Some(s) = strip_value(&f, kw, gone) {
                    l.set(45, 80, &s);
                    break;
                }
            }
        }
    }
    for new in after.iter().filter(|a| !before.contains(a)) {
        lines.extend(function_lines(&format!("{kw}({new})")));
    }
}

/// `KW(A B)` without `value`; the keyword gone when nothing is left.
fn strip_value(area: &str, kw: &str, value: &str) -> Option<String> {
    let start = area.find(&format!("{kw}("))?;
    let open = start + kw.len();
    let close = open + area[open..].find(')')?;
    let words: Vec<&str> = area[open + 1..close].split_whitespace().collect();
    if !words.contains(&value) {
        return None;
    }
    let left: Vec<&str> = words.into_iter().filter(|w| *w != value).collect();
    let mid = if left.is_empty() {
        String::new()
    } else {
        format!("{kw}({})", left.join(" "))
    };
    Some(format!("{}{}{}", &area[..start], mid, &area[close + 1..]).trim().to_string())
}

/// A new element's lines as SDA writes them.
#[must_use]
pub fn generate(e: &Element) -> Vec<Line> {
    let mut out = Vec::new();
    match &e.kind {
        Kind::Constant { text, keyword } => {
            let first = match keyword.as_deref() {
                None | Some("DFT") => quote(text),
                Some(k) => k.to_string(),
            };
            let mut lines = function_lines(&first);
            if let Some(l) = lines.first_mut() {
                locate(l, e);
            }
            out.extend(lines);
        }
        Kind::Field { .. } => {
            let mut l = Line::spec();
            shape(&mut l, e);
            locate(&mut l, e);
            out.push(l);
            // The first keyword shares the line when it fits, as SDA writes.
            let mut first = true;
            for k in e.keywords.iter().filter(|k| !matches!(k.name.as_str(), "DSPATR" | "COLOR")) {
                let text = k.to_string();
                if first && text.chars().count() <= 36 {
                    if let Some(l) = out.last_mut() {
                        l.set(45, 80, &text);
                    }
                } else {
                    out.extend(function_lines(&text));
                }
                first = false;
            }
        }
    }
    for a in &e.attrs {
        out.extend(function_lines(&format!("DSPATR({a})")));
    }
    for c in &e.colors {
        out.extend(function_lines(&format!("COLOR({c})")));
    }
    out
}

/// The edit word SDA gives a numeric output field: blanks for the
/// digits and the decimal point.
#[must_use]
pub fn edtwrd(len: u32, dec: u8) -> String {
    let int = len.saturating_sub(u32::from(dec)) as usize;
    format!("{}.{}", " ".repeat(int), " ".repeat(dec as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::member::{Usage, parse};
    use dds::lex::Keyword;

    fn stamp() -> Stamp {
        Stamp {
            date: "20261023".into(),
            time: "120000".into(),
            user: "QPGMR".into(),
            release: "V1R0M0".into(),
        }
    }

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    const SRC: &str = "\
     A*%%TS  SD  20250101  101010  QPGMR       REL-V1R0M0  QS400
     A*%%EC
     A                                      DSPSIZ(24 80 *DS3)
     A                                      CA03(03 'Exit')
     A* The entry screen.
     A          R SCREEN1
     A*%%TS  SD  20250101  101010  QPGMR       REL-V1R0M0  QS400
     A                                  3  2'Customer'
     A                                      DSPATR(HI)
     A            CUST          10A  B  3 21
     A  30                                  DSPATR(RI)
     A* The amount.
     A            AMT            7  2O  5 21EDTWRD('     .  ')
     A          R SCREEN2
     A                                  1  2'Second'
     A                                      COLOR(BLU)";

    #[test]
    fn round_trip_unchanged() {
        let src = lines(SRC);
        let m = parse(&src);
        assert_eq!(m.records.len(), 2);
        assert_eq!(write(&m, &stamp()), src);
    }

    #[test]
    fn parse_elements() {
        let m = parse(&lines(SRC));
        let r = &m.records[0];
        assert_eq!(r.elements.len(), 3);
        assert_eq!(r.elements[0].attrs, vec!["HI".to_string()]);
        assert_eq!(r.elements[0].pos, Some((3, 2)));
        assert_eq!(r.elements[0].width, 8);
        assert!(r.elements[1].attrs.is_empty(), "RI is conditioned");
        assert_eq!(r.elements[2].width, 8);
        assert!(r.elements[2].lines[0].is_comment());
        assert_eq!(m.records[1].elements[0].colors, vec!["BLU".to_string()]);
    }

    #[test]
    fn move_one_field() {
        let src = lines(SRC);
        let mut m = parse(&src);
        m.records[0].elements[1].pos = Some((4, 21));
        let out = write(&m, &stamp());
        assert_eq!(out.len(), src.len());
        // Only the field's line and the record's stamp differ.
        let diff: Vec<usize> = (0..src.len()).filter(|i| src[*i] != out[*i]).collect();
        assert_eq!(diff, vec![6, 9]);
        assert!(out[9].ends_with("B  4 21"));
        assert!(out[6].contains("20261023"));
        // The second record is as it was.
        assert_eq!(out[13..], src[13..]);
    }

    #[test]
    fn constant_text_and_attributes() {
        let src = lines(SRC);
        let mut m = parse(&src);
        let e = &mut m.records[0].elements[0];
        e.kind = Kind::Constant {
            text: "Customer's name".into(),
            keyword: None,
        };
        e.attrs.clear();
        e.attrs.push("RI".into());
        let out = write(&m, &stamp());
        assert!(out[7].ends_with("3  2'Customer''s name'"));
        assert!(out[8].ends_with("DSPATR(RI)"));
        assert!(!out.iter().any(|l| l.ends_with("DSPATR(HI)")));
        let again = parse(&out);
        assert_eq!(write(&again, &stamp()), out);
    }

    #[test]
    fn new_member() {
        let mut m = Member::default();
        let mut r = Record::new("SCREEN1", super::super::member::RecordType::Record);
        r.elements.push(Element::new(
            Kind::Constant {
                text: "Customer".into(),
                keyword: None,
            },
            Some((3, 2)),
        ));
        r.elements
            .push(Element::field("FLD001", Usage::Input, false, 10, 0, Some((3, 21))));
        r.elements
            .push(Element::field("FLD002", Usage::Output, true, 7, 2, Some((5, 21))));
        m.records.push(r);
        let out = write(&m, &stamp());
        let want = [
            "     A*%%TS  SD  20261023  120000  QPGMR       REL-V1R0M0  QS400",
            "     A*%%EC",
            "     A                                      DSPSIZ(24 80 *DS3)",
            "     A          R SCREEN1",
            "     A*%%TS  SD  20261023  120000  QPGMR       REL-V1R0M0  QS400",
            "     A                                  3  2'Customer'",
            "     A            FLD001        10   I  3 21",
            "     A            FLD002         7  2O  5 21EDTWRD('     .  ')",
        ];
        assert_eq!(out, want);
        // Read back, written again unchanged.
        let m2 = parse(&out);
        assert_eq!(write(&m2, &stamp()), out);
        assert_eq!(m2.records[0].elements[2].width, 8);
    }

    #[test]
    fn keywords_edited_in_place() {
        let src = lines(SRC);
        let mut m = parse(&src);
        // Record level: CA03 goes from the file, OVERLAY comes to SCREEN2.
        m.keywords.retain(|k| k.name != "CA03");
        let mut ov = m.records[0].keywords.clone();
        ov.push(Keyword {
            name: "OVERLAY".into(),
            parms: Vec::new(),
            line: 0,
        });
        m.records[1].keywords = ov;
        // Field level: CUST gets CHECK(ME).
        m.records[0].elements[1].keywords.push(Keyword {
            name: "CHECK".into(),
            parms: vec![dds::lex::Parm::Word("ME".into())],
            line: 0,
        });
        let out = write(&m, &stamp());
        assert!(!out.iter().any(|l| l.contains("CA03")));
        assert_eq!(out.len(), src.len() - 1 + 2 + 1);
        let at = out.iter().position(|l| l.ends_with("R SCREEN2")).unwrap();
        assert!(out[at + 1].ends_with("OVERLAY"));
        let cust = out.iter().position(|l| l.contains("CUST")).unwrap();
        assert!(out[cust + 2].ends_with("CHECK(ME)"), "{out:#?}");
        // The conditioned DSPATR(RI) stays.
        assert!(out[cust + 1].contains("30") && out[cust + 1].ends_with("DSPATR(RI)"));
        let again = parse(&out);
        assert_eq!(write(&again, &stamp()), out);
    }

    #[test]
    fn long_constant_continues() {
        let mut e = Element::new(
            Kind::Constant {
                text: "x".repeat(50),
                keyword: None,
            },
            Some((1, 2)),
        );
        e.attrs.push("HI".into());
        let ls = generate(&e);
        assert_eq!(ls.len(), 3);
        assert_eq!(ls[0].ch(80), '-');
        let m = parse(&ls.iter().map(Line::text).collect::<Vec<_>>());
        assert!(m.records.is_empty());
    }
}
