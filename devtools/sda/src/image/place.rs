// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the operator typed on the design image, worked out by comparing
//! the image as it was shown with what came back:
//!
//! - on blank places, `+` starts a field: `+I`, `+O`, `+B` character
//!   input, output and both, `+3`, `+6`, `+9` numeric; the length as
//!   the letter repeated (`+IIII`) or in parentheses (`+I(10)`,
//!   `+6(7,2)`). The `+` takes the attribute place, the field starts
//!   after it.
//! - other words are constants, one per word; `'text'` keeps blanks.
//! - in the place before a field or constant (its attribute place):
//!   `H` `R` `U` `B` `C` `N` turn high intensity, reverse image,
//!   underline, blink, column separators and nondisplay on or off; a
//!   color code ending there (`CB` `CG` `CP` `CR` `CT` `CW` `CY`) sets
//!   the color; `D` deletes it; `*` asks for its keywords; `-` marks it
//!   to move, to the place of an `=` typed elsewhere.
//! - over a constant, the new text replaces it; a field or constant
//!   blanked out is deleted.

use crate::source::{Element, Kind, Record, Usage};

use super::{COLS, ROWS, element_text};

/// One change the operator asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// A new constant.
    Constant {
        /// Row.
        row: u16,
        /// Column.
        col: u16,
        /// Text.
        text: String,
    },
    /// A new field.
    Field {
        /// Row.
        row: u16,
        /// Column.
        col: u16,
        /// Input, output or both.
        usage: Usage,
        /// Numeric.
        numeric: bool,
        /// Length (digits for a number).
        len: u32,
        /// Decimal positions.
        dec: u8,
    },
    /// A constant's new text.
    Text {
        /// The element.
        index: usize,
        /// The text.
        text: String,
    },
    /// Delete an element.
    Delete {
        /// The element.
        index: usize,
    },
    /// Turn a display attribute on or off (`HI`, `RI`, `UL`, `BL`, `CS`,
    /// `ND`).
    Attr {
        /// The element.
        index: usize,
        /// The `DSPATR` value.
        value: &'static str,
    },
    /// Set the color (`BLU`, `GRN`, `PNK`, `RED`, `TRQ`, `WHT`, `YLW`).
    Color {
        /// The element.
        index: usize,
        /// The `COLOR` value.
        value: &'static str,
    },
    /// Show the element's keywords.
    Keywords {
        /// The element.
        index: usize,
    },
    /// Move an element so it starts at a place.
    Move {
        /// The element.
        index: usize,
        /// Row.
        row: u16,
        /// Column of the element (the place after the `=`).
        col: u16,
    },
}

/// Why the typing cannot be taken.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PlaceError {
    /// A field definition SDA does not know.
    #[error("Field definition {text} at row {row}, column {col} is not valid.")]
    BadField {
        /// What was typed.
        text: String,
        /// Row.
        row: u16,
        /// Column.
        col: u16,
    },
    /// A code in an attribute place SDA does not know.
    #[error("Code {code} at row {row}, column {col} is not valid.")]
    BadCode {
        /// What was typed.
        code: String,
        /// Row.
        row: u16,
        /// Column.
        col: u16,
    },
    /// A move without a target.
    #[error("A move at row {row:02}, column {col:03} has no target; type = where it goes.")]
    NoTarget {
        /// Row.
        row: u16,
        /// Column.
        col: u16,
    },
    /// A target without a move.
    #[error("A target at row {row:02}, column {col:03} has nothing to move; type - before what goes there.")]
    NoSource {
        /// Row.
        row: u16,
        /// Column.
        col: u16,
    },
}

#[allow(clippy::too_many_lines)] // one pass over the image
/// The changes in `typed` against the image `shown` of `record`.
///
/// # Errors
///
/// The first thing typed that cannot be taken.
pub fn parse(record: &Record, shown: &[String], typed: &[String]) -> Result<Vec<Edit>, PlaceError> {
    let old = grid(shown);
    let new = grid(typed);
    let mut edits = Vec::new();
    let mut claimed = vec![vec![false; COLS]; ROWS];
    let mut moves: Vec<(usize, u16, u16)> = Vec::new();
    let mut targets: Vec<(u16, u16)> = Vec::new();
    // Codes and overtyping on what the image shows.
    for (i, e) in record.elements.iter().enumerate() {
        if !e.displayed() {
            continue;
        }
        let Some((row, col)) = e.pos else { continue };
        let (r, c) = (row as usize - 1, col as usize - 1);
        if r >= ROWS {
            continue;
        }
        let w = element_text(e).chars().count().max(1);
        let end = (c + w).min(COLS);
        for cl in &mut claimed[r][c.saturating_sub(1)..end] {
            *cl = true;
        }
        let before: String = old[r][c..end].iter().collect();
        let after: String = new[r][c..end].iter().collect();
        if before != after {
            if after.trim().is_empty() {
                edits.push(Edit::Delete { index: i });
                continue;
            }
            if let Kind::Constant { keyword: None, .. } = &e.kind {
                // Typing may run on past the end into blanks.
                let mut text = after.clone();
                let mut k = end;
                while k < COLS && new[r][k] != ' ' && old[r][k] == ' ' {
                    text.push(new[r][k]);
                    claimed[r][k] = true;
                    k += 1;
                }
                edits.push(Edit::Text {
                    index: i,
                    text: text.trim_end().to_string(),
                });
            }
        }
        if c == 0 {
            continue;
        }
        let a = new[r][c - 1];
        if a == old[r][c - 1] || a == ' ' {
            continue;
        }
        // A two-letter color code ends on the attribute place.
        let pair: String = if c >= 2 {
            [new[r][c - 2], a].iter().collect()
        } else {
            String::new()
        };
        if let Some(value) = color(&pair.to_ascii_uppercase()) {
            if c >= 2 {
                claimed[r][c - 2] = true;
            }
            edits.push(Edit::Color { index: i, value });
            continue;
        }
        match a.to_ascii_uppercase() {
            'D' => edits.push(Edit::Delete { index: i }),
            '*' => edits.push(Edit::Keywords { index: i }),
            '-' => moves.push((i, row, col - 1)),
            ch => match attr(ch) {
                Some(value) => edits.push(Edit::Attr { index: i, value }),
                None => {
                    return Err(PlaceError::BadCode {
                        code: a.to_string(),
                        row,
                        col: col - 1,
                    });
                }
            },
        }
    }
    // New text on free places.
    for r in 0..ROWS {
        let mut c = 0;
        while c < COLS {
            if claimed[r][c] || new[r][c] == old[r][c] || new[r][c] == ' ' {
                c += 1;
                continue;
            }
            let row = u16::try_from(r + 1).unwrap_or(0);
            let ch = new[r][c];
            let col = u16::try_from(c + 1).unwrap_or(0);
            if ch == '=' {
                targets.push((row, col));
                c += 1;
                continue;
            }
            if ch == '\'' {
                let close = (c + 1..COLS).find(|k| new[r][*k] == '\'');
                let stop = close.unwrap_or(COLS);
                let text: String = new[r][c + 1..stop].iter().collect();
                edits.push(Edit::Constant {
                    row,
                    col: col + 1,
                    text: text.trim_end().to_string(),
                });
                c = stop + 1;
                continue;
            }
            let mut k = c;
            while k < COLS && new[r][k] != ' ' && !claimed[r][k] {
                k += 1;
            }
            let word: String = new[r][c..k].iter().collect();
            if let Some(spec) = word.strip_prefix('+') {
                edits.push(field(spec, row, col).ok_or(PlaceError::BadField {
                    text: word.clone(),
                    row,
                    col,
                })?);
            } else {
                edits.push(Edit::Constant {
                    row,
                    col,
                    text: word,
                });
            }
            c = k;
        }
    }
    match (moves.len(), targets.len()) {
        (0, 0) => {}
        (_, 0) => {
            let (_, row, col) = moves[0];
            return Err(PlaceError::NoTarget { row, col });
        }
        (0, _) => {
            let (row, col) = targets[0];
            return Err(PlaceError::NoSource { row, col });
        }
        _ => {
            // The first marked element goes to the target; the others
            // keep their places relative to it.
            let (_, r0, c0) = moves[0];
            let (tr, tc) = targets[0];
            for (index, row, col) in moves {
                let dr = i32::from(row) - i32::from(r0);
                let dc = i32::from(col) - i32::from(c0);
                let nr = u16::try_from(i32::from(tr) + dr).unwrap_or(1);
                let nc = u16::try_from(i32::from(tc) + dc + 1).unwrap_or(2);
                edits.push(Edit::Move {
                    index,
                    row: nr,
                    col: nc,
                });
            }
        }
    }
    Ok(edits)
}

fn grid(rows: &[String]) -> Vec<Vec<char>> {
    let mut g = vec![vec![' '; COLS]; ROWS];
    for (r, row) in rows.iter().take(ROWS).enumerate() {
        for (c, ch) in row.chars().take(COLS).enumerate() {
            g[r][c] = if ch.is_control() { ' ' } else { ch };
        }
    }
    g
}

fn attr(ch: char) -> Option<&'static str> {
    Some(match ch {
        'H' => "HI",
        'R' => "RI",
        'U' => "UL",
        'B' => "BL",
        'C' => "CS",
        'N' => "ND",
        _ => return None,
    })
}

fn color(code: &str) -> Option<&'static str> {
    Some(match code {
        "CB" => "BLU",
        "CG" => "GRN",
        "CP" => "PNK",
        "CR" => "RED",
        "CT" => "TRQ",
        "CW" => "WHT",
        "CY" => "YLW",
        _ => return None,
    })
}

/// A field definition after its `+`.
fn field(spec: &str, row: u16, col: u16) -> Option<Edit> {
    let up = spec.to_ascii_uppercase();
    let mut chars = up.chars();
    let kind = chars.next()?;
    let (usage, numeric) = match kind {
        'I' => (Usage::Input, false),
        'O' => (Usage::Output, false),
        'B' => (Usage::Both, false),
        '3' => (Usage::Input, true),
        '6' => (Usage::Output, true),
        '9' => (Usage::Both, true),
        _ => return None,
    };
    let rest: String = chars.collect();
    let (len, dec) = if let Some(inner) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        match inner.split_once(',') {
            Some((l, d)) => (l.trim().parse().ok()?, d.trim().parse().ok()?),
            None => (inner.trim().parse().ok()?, 0),
        }
    } else if rest.chars().all(|c| c == kind) {
        (u32::try_from(rest.len() + 1).ok()?, 0)
    } else {
        return None;
    };
    let max = if numeric { 63 } else { 3564 };
    if len == 0 || len > max || (dec > 0 && (!numeric || u32::from(dec) > len)) {
        return None;
    }
    Some(Edit::Field {
        row,
        col: col + 1,
        usage,
        numeric,
        len,
        dec,
    })
}

/// Apply the edits to the record. Returns the element whose keywords
/// were asked for, if any; generated field names avoid `taken`.
pub fn apply(record: &mut Record, edits: &[Edit], taken: &[String]) -> Option<usize> {
    let mut keywords = None;
    let mut delete = Vec::new();
    for e in edits {
        match e {
            Edit::Text { index, text } => {
                if let Some(el) = record.elements.get_mut(*index) {
                    el.kind = Kind::Constant {
                        text: text.clone(),
                        keyword: None,
                    };
                    el.width = u32::try_from(text.chars().count()).unwrap_or(0);
                }
            }
            Edit::Delete { index } => delete.push(*index),
            Edit::Attr { index, value } => {
                if let Some(el) = record.elements.get_mut(*index) {
                    if let Some(p) = el.attrs.iter().position(|a| a == value) {
                        el.attrs.remove(p);
                    } else {
                        el.attrs.push((*value).to_string());
                    }
                }
            }
            Edit::Color { index, value } => {
                if let Some(el) = record.elements.get_mut(*index) {
                    el.colors = vec![(*value).to_string()];
                }
            }
            Edit::Keywords { index } => keywords = Some(*index),
            Edit::Move { index, row, col } => {
                if let Some(el) = record.elements.get_mut(*index) {
                    el.pos = Some((*row, *col));
                }
            }
            Edit::Constant { .. } | Edit::Field { .. } => {}
        }
    }
    // New elements go in row and column order among the others.
    for e in edits {
        let el = match e {
            Edit::Constant { row, col, text } => Element::new(
                Kind::Constant {
                    text: text.clone(),
                    keyword: None,
                },
                Some((*row, *col)),
            ),
            Edit::Field {
                row,
                col,
                usage,
                numeric,
                len,
                dec,
            } => {
                let name = record.next_field_name(taken);
                Element::field(&name, *usage, *numeric, *len, *dec, Some((*row, *col)))
            }
            _ => continue,
        };
        let at = insert_at(record, el.pos);
        record.elements.insert(at, el);
        delete.iter_mut().filter(|d| **d >= at).for_each(|d| *d += 1);
        if let Some(k) = keywords.as_mut()
            && *k >= at
        {
            *k += 1;
        }
        record.changed = true;
    }
    delete.sort_unstable();
    delete.dedup();
    for d in delete.into_iter().rev() {
        record.elements.remove(d);
        record.changed = true;
        if let Some(k) = keywords {
            if k == d {
                keywords = None;
            } else if k > d {
                keywords = Some(k - 1);
            }
        }
    }
    keywords
}

/// Where an element at `pos` goes: before the first displayed element
/// placed after it.
fn insert_at(record: &Record, pos: Option<(u16, u16)>) -> usize {
    let Some(p) = pos else {
        return record.elements.len();
    };
    record
        .elements
        .iter()
        .position(|e| e.displayed() && e.pos.is_some_and(|q| q > p))
        .unwrap_or(record.elements.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{View, render};
    use crate::source::RecordType;

    fn typed(shown: &[String], at: &[(usize, usize, &str)]) -> Vec<String> {
        let mut g = grid(shown);
        for (r, c, t) in at {
            for (i, ch) in t.chars().enumerate() {
                g[r - 1][c - 1 + i] = ch;
            }
        }
        g.into_iter().map(|r| r.into_iter().collect()).collect()
    }

    fn sample() -> Record {
        let mut r = Record::new("SCREEN1", RecordType::Record);
        let shown = render(&r, &[], &View::default());
        let t = typed(
            &shown,
            &[
                (3, 2, "Customer"),
                (3, 20, "+I(10)"),
                (5, 2, "Amount due"),
                (5, 20, "+6(7,2)"),
                (7, 2, "'Two  words'"),
                (7, 20, "+BBB"),
            ],
        );
        let edits = parse(&r, &shown, &t).unwrap();
        apply(&mut r, &edits, &[]);
        r
    }

    #[test]
    fn placing_as_on_the_oracle() {
        let r = sample();
        let names: Vec<String> = r
            .elements
            .iter()
            .map(|e| match &e.kind {
                Kind::Constant { text, .. } => format!("{text}@{:?}", e.pos.unwrap()),
                Kind::Field { name, len, .. } => format!("{name}:{len}@{:?}", e.pos.unwrap()),
            })
            .collect();
        assert_eq!(
            names,
            [
                "Customer@(3, 2)",
                "FLD001:10@(3, 21)",
                "Amount@(5, 2)",
                "due@(5, 9)",
                "FLD002:7@(5, 21)",
                "Two  words@(7, 3)",
                "FLD003:3@(7, 21)",
            ]
        );
        let rows = render(&r, &[], &View::default());
        assert_eq!(&rows[4][20..28], "66666.66");
        assert_eq!(&rows[6][20..23], "BBB");
    }

    #[test]
    fn codes() {
        let mut r = sample();
        let shown = render(&r, &[], &View::default());
        let t = typed(&shown, &[(3, 1, "H"), (3, 20, "R"), (5, 8, "D"), (7, 19, "CY")]);
        let edits = parse(&r, &shown, &t).unwrap();
        assert!(edits.contains(&Edit::Attr { index: 0, value: "HI" }));
        assert!(edits.contains(&Edit::Attr { index: 1, value: "RI" }));
        assert!(edits.contains(&Edit::Delete { index: 3 }));
        assert!(edits.contains(&Edit::Color { index: 6, value: "YLW" }));
        apply(&mut r, &edits, &[]);
        assert_eq!(r.elements.len(), 6);
        assert_eq!(r.elements[0].attrs, ["HI"]);
        assert_eq!(r.elements[5].colors, ["YLW"]);
    }

    #[test]
    fn move_needs_a_target() {
        let mut r = sample();
        let shown = render(&r, &[], &View::default());
        let t = typed(&shown, &[(5, 1, "-")]);
        assert_eq!(
            parse(&r, &shown, &t),
            Err(PlaceError::NoTarget { row: 5, col: 1 })
        );
        let t = typed(&shown, &[(5, 1, "-"), (10, 5, "=")]);
        let edits = parse(&r, &shown, &t).unwrap();
        apply(&mut r, &edits, &[]);
        assert_eq!(r.elements[2].pos, Some((10, 6)));
    }

    #[test]
    fn overtype_and_blank_out() {
        let mut r = sample();
        let shown = render(&r, &[], &View::default());
        let t = typed(&shown, &[(3, 2, "Client  "), (5, 21, "        ")]);
        let edits = parse(&r, &shown, &t).unwrap();
        apply(&mut r, &edits, &[]);
        assert!(matches!(&r.elements[0].kind, Kind::Constant { text, .. } if text == "Client"));
        assert!(!r.elements.iter().any(|e| e.name() == Some("FLD002")));
    }

    #[test]
    fn bad_definitions() {
        let r = Record::new("X", RecordType::Record);
        let shown = render(&r, &[], &View::default());
        let t = typed(&shown, &[(2, 2, "+Q(3)")]);
        assert!(matches!(parse(&r, &shown, &t), Err(PlaceError::BadField { .. })));
        let t = typed(&shown, &[(2, 2, "+I(2,1)")]);
        assert!(parse(&r, &shown, &t).is_err());
    }

    #[test]
    fn keywords_asked_for() {
        let mut r = sample();
        let shown = render(&r, &[], &View::default());
        let t = typed(&shown, &[(5, 20, "*")]);
        let edits = parse(&r, &shown, &t).unwrap();
        assert_eq!(apply(&mut r, &edits, &[]), Some(4));
    }
}
