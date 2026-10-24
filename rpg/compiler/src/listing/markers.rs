// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where a diagnostic's marker goes under its line, and how wide it is.
//! The compiler points a diagnostic at a keyword or an operation; IBM's
//! marker is often on a part of it: the keyword's value, a second
//! keyword it clashes with, the closing parenthesis, the operand after
//! the operation, or a column field of a fixed-form specification.

use rpgfront::Diag;
use rpgfront::reader::Line;

/// Columns `from..=to` of a fixed-form specification.
fn fixed_field(id: &str) -> Option<(usize, usize)> {
    Some(match id {
        "RNF5030" => (50, 63),
        "RNF5023" => (36, 49),
        "RNF5044" => (69, 70),
        "RNF5053" => (71, 76),
        "RNF3402" | "RNF3716" => (33, 39),
        "RNF5006" => (9, 9),
        "RNF5007" => (10, 11),
        _ => return None,
    })
}

/// How IBM chooses the marker's place.
enum Rule {
    /// The keyword's argument (1 or 2).
    Arg(usize),
    /// The compiler's own column.
    Raw,
    /// The last colon of the group: the first argument too many.
    LastColon,
    /// Just after the first word.
    AfterFirst,
    /// The member a copy directive names.
    Member,
    /// The parenthesis that closes the group.
    Close,
    /// The operand after the operation (or the semicolon).
    Operand,
}

fn rule(id: &str) -> Option<Rule> {
    Some(match id {
        "RNF0503" | "RNF3557" | "RNF0533" | "RNF3358" | "RNF3596" | "RNF0231" | "RNF3452" | "RNF0504" | "RNF3559"
        | "RNF3564" | "RNF3813" | "RNF0612" | "RNF0614" | "RNF1311" | "RNF1331" | "RNF1317" | "RNF3568" | "RNF1008"
        | "RNF1303" | "RNF0341" | "RNF1306" | "RNF0312" | "RNF5377" => Rule::Arg(1),
        "RNF0512" | "RNF3561" => Rule::Arg(2),
        "RNF3327" | "RNF3312" | "RNF3322" | "RNF3323" | "RNF3740" | "RNF2416" | "RNF3333" | "RNF3575" => Rule::Raw,
        "RNF0607" | "RNF0316" | "RNF0637" => Rule::Close,
        "RNF0317" => Rule::LastColon,
        "RNF0256" => Rule::AfterFirst,
        "RNF0273" => Rule::Member,
        "RNF5412" | "RNF5413" | "RNF5344" | "RNF5294" | "RNF5116" | "RNF3768" | "RNF7261" | "RNF3556" | "RNF3530" => Rule::Operand,
        _ => return None,
    })
}

fn is_name(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '#' | '@' | '$' | '*' | '%')
}

/// The end of the word at `i`: names with inner hyphens and dots.
fn word_end(t: &[char], i: usize) -> usize {
    let mut j = i;
    while j < t.len() {
        let c = t[j];
        let inner = matches!(c, '-' | '.') && j > i && t.get(j + 1).is_some_and(char::is_ascii_alphanumeric);
        if !(is_name(c) || inner) || (c == '*' && j > i) {
            break;
        }
        j += 1;
    }
    j
}

fn skip_blanks(t: &[char], mut i: usize) -> usize {
    while i < t.len() && t[i] == ' ' {
        i += 1;
    }
    i
}

/// The token at `i`: a literal, a number, a name, or one character.
pub(crate) fn token(t: &[char], i: usize) -> usize {
    let Some(&c) = t.get(i) else { return 1 };
    if c == '\'' {
        return t[i + 1..].iter().position(|x| *x == '\'').map_or(1, |p| p + 2);
    }
    if c.is_ascii_digit() || ((c == '-' || c == '+') && t.get(i + 1).is_some_and(char::is_ascii_digit)) {
        return 1 + t[i + 1..].iter().take_while(|x| x.is_ascii_digit() || matches!(x, '.' | ',')).count();
    }
    (word_end(t, i) - i).max(1)
}

/// The index after the parenthesized group starting at `i` (a `(`).
fn group_end(t: &[char], i: usize) -> usize {
    let mut depth = 0;
    for (k, c) in t.iter().enumerate().skip(i) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return k + 1;
                }
            }
            _ => {}
        }
    }
    t.len()
}

/// Column (from 1) and width of a diagnostic's marker under its line,
/// when a rule places it: from the compiler's column, else from
/// `alt`, the column of the name it is about.
pub(crate) fn place(d: &Diag, l: &Line, col: usize, alt: usize) -> Option<(usize, usize)> {
    // A column 0 is the statement's start.
    let first = l.text.chars().position(|c| c != ' ').map_or(1, |p| p + 1);
    let col = if col == 0 { first } else { col };
    place_at(d, l, col).or_else(|| if alt > 0 && alt != col { place_at(d, l, alt) } else { None })
}

fn place_at(d: &Diag, l: &Line, col: usize) -> Option<(usize, usize)> {
    if !l.fully_free
        && let Some((from, to)) = fixed_field(d.msgid)
    {
        return Some((from, to + 1 - from));
    }
    // A numeric entry not right-aligned: a file's whole entry, else
    // from the entry to its field's end.
    if !l.fully_free && d.msgid == "RNF0263" {
        let fields: &[(usize, usize)] = match l.text.chars().nth(5).map(|c| c.to_ascii_uppercase()) {
            Some('F') => &[(23, 27), (29, 33)],
            Some('D') => &[(26, 32), (33, 39), (41, 42)],
            Some('C') => &[(64, 68), (69, 70)],
            _ => &[],
        };
        let &(from, to) = fields.iter().find(|(f, t)| (*f..=*t).contains(&col))?;
        let start = if l.text.chars().nth(5).is_some_and(|c| c.eq_ignore_ascii_case(&'F')) { from } else { col };
        return Some((start, to + 1 - start));
    }
    let t: Vec<char> = l.text.chars().collect();
    let i = col.checked_sub(1)?;
    if i >= t.len() {
        return None;
    }
    // A built-in function only one operation takes, used elsewhere: the
    // marker is its name.
    if d.msgid == "RNF0637" && t[i] == '%' {
        return Some((col, token(&t, i)));
    }
    // A MONITOR without ON-ERROR: IBM marks six characters.
    if d.msgid == "RNF5296" && l.fully_free {
        return Some((i + 1, 6));
    }
    let at = match rule(d.msgid)? {
        Rule::Arg(n) => {
            let mut j = skip_blanks(&t, word_end(&t, i));
            if t.get(j) != Some(&'(') {
                return None;
            }
            j = skip_blanks(&t, j + 1);
            for _ in 1..n {
                let colon = t[j..].iter().position(|c| *c == ':')?;
                j = skip_blanks(&t, j + colon + 1);
            }
            j
        }
        Rule::Raw => i,
        Rule::Member => {
            let mut j = skip_blanks(&t, 0);
            while j < t.len() && t[j] != ' ' {
                j += 1;
            }
            let j = skip_blanks(&t, j);
            let n = t[j..].iter().take_while(|c| !c.is_whitespace()).count();
            return (n > 0).then_some((j + 1, n));
        }
        Rule::AfterFirst => return Some((word_end(&t, skip_blanks(&t, 0)) + 1, 1)),
        Rule::LastColon => {
            let open = t[i..].iter().position(|c| *c == '(')? + i;
            let end = group_end(&t, open);
            let mut depth = 0;
            let mut last = None;
            for (k, c) in t.iter().enumerate().take(end).skip(open) {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    ':' if depth == 1 => last = Some(k),
                    _ => {}
                }
            }
            return last.map(|k| (k + 1, 1));
        }
        Rule::Close => {
            let mut depth = 0;
            let mut found = None;
            for (k, c) in t.iter().enumerate().skip(i) {
                match c {
                    '(' => depth += 1,
                    ')' if depth <= 1 => {
                        found = Some(k);
                        break;
                    }
                    ')' => depth -= 1,
                    _ => {}
                }
            }
            return found.map(|k| (k + 1, 1));
        }
        Rule::Operand if l.fully_free => skip_blanks(&t, word_end(&t, i)),
        Rule::Operand => return None,
    };
    if at >= t.len() {
        return None;
    }
    Some((at + 1, token(&t, at)))
}
