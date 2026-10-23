// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The layout rules of `docs/UIM.md` §7 as pure functions: centering,
//! the item flow with its column alignment (R5 to R7), word wrapping,
//! dot leaders (R14) and the command line prompt (R11).

/// Blanks between flowed items.
pub const GAP: usize = 3;

/// The column a centered text starts at: `1 + (width - len) / 2` (R1).
#[must_use]
pub fn center(width: usize, len: usize) -> usize {
    1 + width.saturating_sub(len) / 2
}

/// Items flowed left to right from `start`, `GAP` blanks apart, none
/// passing `limit`; an item that would wraps to a new row. Returns rows
/// of `(item index, column)`. The first two rows are column-aligned
/// (R7).
#[must_use]
pub fn flow(widths: &[usize], start: usize, limit: usize) -> Vec<Vec<(usize, usize)>> {
    let mut rows: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut col = start;
    for (i, &w) in widths.iter().enumerate() {
        let fits = col + w <= limit + 1;
        if rows.is_empty() || (!fits && col != start) {
            rows.push(Vec::new());
            col = start;
        }
        rows.last_mut().unwrap().push((i, col));
        col += w + GAP;
    }
    // R7: the first two rows align; a third row stays as flowed.
    if rows.len() >= 2 {
        align(&mut rows[..2], widths, limit);
    }
    rows
}

/// R7: for each item index both rows hold, the row whose item starts
/// earlier moves it (and what follows) to the other's column when the
/// row still ends within `limit`; otherwise both stay.
fn align(rows: &mut [Vec<(usize, usize)>], widths: &[usize], limit: usize) {
    let n = rows[0].len().min(rows[1].len());
    for k in 1..n {
        let (a, b) = (rows[0][k].1, rows[1][k].1);
        if a == b {
            continue;
        }
        let (r, delta) = if a < b { (0, b - a) } else { (1, a - b) };
        let row = &rows[r];
        let (last, lc) = *row.last().unwrap();
        if lc + delta + widths[last] <= limit + 1 {
            for item in &mut rows[r][k..] {
                item.1 += delta;
            }
        }
    }
}

/// Words packed into rows no wider than `width`; a word longer than
/// the width stands alone, cut by the caller. Blank runs inside a row
/// are kept as written.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut row = String::new();
    // Tokenize into (blanks, word) pairs.
    let mut tokens: Vec<(usize, String)> = Vec::new();
    let mut blanks = 0;
    let mut word = String::new();
    for ch in text.chars() {
        if ch == ' ' {
            if !word.is_empty() {
                tokens.push((blanks, std::mem::take(&mut word)));
                blanks = 0;
            }
            blanks += 1;
        } else {
            word.push(ch);
        }
    }
    if !word.is_empty() {
        tokens.push((blanks, word));
    }
    for (blanks, word) in tokens {
        let wlen = word.chars().count();
        if row.is_empty() {
            row = word;
        } else if row.chars().count() + blanks + wlen <= width {
            row.extend(std::iter::repeat_n(' ', blanks));
            row.push_str(&word);
        } else {
            rows.push(std::mem::replace(&mut row, word));
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

/// The text from the column after a label's end to the colon: dots
/// at the colon's parity from the first such column two past the
/// label, then the colon (R14).
#[must_use]
pub fn leaders(label_end: usize, colon: usize) -> String {
    let mut out = String::new();
    for c in label_end + 1..=colon {
        if c == colon {
            out.push(':');
        } else if c >= label_end + 2 && c % 2 == colon % 2 {
            out.push('.');
        } else {
            out.push(' ');
        }
    }
    out
}

/// The colon column of a data area: the longest label's end plus 12
/// (R14).
#[must_use]
pub fn colon_col(longest_label_end: usize) -> usize {
    longest_label_end + 12
}

/// The command line prompt naming the options that take parameters:
/// `Parameters for options 5, 7 and 13 or command` (R11).
#[must_use]
pub fn options_prompt(options: &[&str]) -> String {
    match options {
        [] => "Parameters or command".to_string(),
        [one] => format!("Parameters for option {one} or command"),
        [init @ .., last] => {
            let joined = init.join(", ");
            format!("Parameters for options {joined} and {last} or command")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(rows: &[Vec<(usize, usize)>]) -> Vec<Vec<usize>> {
        rows.iter()
            .map(|r| r.iter().map(|&(_, c)| c).collect())
            .collect()
    }

    #[test]
    fn titles_center() {
        assert_eq!(center(80, 17), 32);
        assert_eq!(center(80, 26), 28);
        assert_eq!(center(80, 15), 33);
        assert_eq!(center(80, 22), 30);
    }

    #[test]
    fn legends_align_the_upper_row_when_it_fits() {
        // wrkobj: 16 6 8 19 8 / 21 21.
        let rows = flow(&[16, 6, 8, 19, 8, 21, 21], 4, 79);
        assert_eq!(cols(&rows), [vec![4, 28, 37, 48, 70], vec![4, 28]]);
    }

    #[test]
    fn keys_align_column_by_column() {
        // wrkjobd, wrkcls, wrkbnddir: the third column cannot move.
        let rows = flow(&[7, 9, 10, 11, 22, 10, 22, 15], 2, 79);
        assert_eq!(cols(&rows), [vec![2, 15, 27, 40, 54], vec![2, 15, 40]]);
        // wrkusrjob: the lower row moves at the third column.
        let rows = flow(&[7, 9, 10, 11, 25, 10, 7, 10, 27], 2, 79);
        assert_eq!(cols(&rows), [vec![2, 15, 27, 40, 54], vec![2, 15, 27, 40]]);
        // wrkobj: the upper row would pass column 79, so nothing moves.
        let rows = flow(&[7, 9, 10, 11, 27, 10, 22, 15], 2, 79);
        assert_eq!(cols(&rows), [vec![2, 12, 24, 37, 51], vec![2, 15, 40]]);
        // wrkrpyle, wrksbs: one item on the lower row aligns nothing.
        let rows = flow(&[7, 10, 6, 24, 10, 13], 2, 79);
        assert_eq!(cols(&rows), [vec![2, 12, 25, 34, 61], vec![2]]);
        let rows = flow(&[15, 31, 26], 4, 79);
        assert_eq!(cols(&rows), [vec![4, 22], vec![4]]);
        // main-menu-help: one row of help keys.
        let rows = flow(&[12, 15, 10, 25, 14], 2, 79);
        assert_eq!(cols(&rows), [vec![2, 17, 35, 48], vec![2]]);
    }

    #[test]
    fn wrapping() {
        assert_eq!(wrap("Job Description", 10), ["Job", "Description"]);
        assert_eq!(wrap("Binding Directory", 10), ["Binding", "Directory"]);
        assert_eq!(
            wrap("press Enter.  For help", 14),
            ["press Enter.", "For help"]
        );
        assert_eq!(wrap("press Enter.  For", 20), ["press Enter.  For"]);
        assert!(wrap("", 10).is_empty());
    }

    #[test]
    fn dot_leaders() {
        // dspsysval: label 2-13, colon 25.
        assert_eq!(colon_col(13), 25);
        assert_eq!(leaders(13, 25), " . . . . . :");
        // dspusrprf: label ending at 32 needs two blanks first.
        assert_eq!(colon_col(33), 45);
        assert_eq!(leaders(32, 45), "  . . . . . :");
        assert_eq!(leaders(33, 45), " . . . . . :");
    }

    #[test]
    fn prompts() {
        assert_eq!(
            options_prompt(&["5", "7", "13"]),
            "Parameters for options 5, 7 and 13 or command"
        );
        assert_eq!(options_prompt(&["2"]), "Parameters for option 2 or command");
        assert_eq!(options_prompt(&[]), "Parameters or command");
    }
}
