// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Control` field's commands, and find and change over the lines:
//! `T`/`TOP`, `B`/`BOTTOM`, a record number, `+n`/`-n`, `F`/`FIND text`,
//! `C`/`CHANGE old new [ALL]`, `S`/`SAVE`, `FILE`, `CAN`/`CANCEL`. Text
//! with blanks is quoted (`'a b'` or `"a b"`); find ignores case.

/// A control command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    /// The top of the file.
    Top,
    /// The end.
    Bottom,
    /// Show from this record.
    Record(usize),
    /// Move this many records (negative: back).
    Scroll(i64),
    /// Find the text.
    Find(String),
    /// Change the next occurrence, or all.
    Change {
        /// What to find.
        from: String,
        /// What it becomes.
        to: String,
        /// Every occurrence.
        all: bool,
    },
    /// Save and stay.
    Save,
    /// Save and leave.
    File,
    /// Leave without saving.
    Cancel,
}

/// The words of a command line, quotes keeping blanks together.
fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut had = false;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => {
                quote = None;
            }
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                had = true;
            }
            None if c == ' ' => {
                if !cur.is_empty() || had {
                    out.push(std::mem::take(&mut cur));
                    had = false;
                }
            }
            Some(_) | None => cur.push(c),
        }
    }
    if !cur.is_empty() || had {
        out.push(cur);
    }
    out
}

/// Parse the field. `Ok(None)` when it is empty, `Err(id)` with the
/// message for one that is not a command.
pub fn parse(s: &str) -> Result<Option<Control>, &'static str> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if let Some(n) = t.strip_prefix('+') {
        return n.trim().parse().map(|n: i64| Some(Control::Scroll(n))).map_err(|_| "CPDB605");
    }
    if let Some(n) = t.strip_prefix('-') {
        return n.trim().parse().map(|n: i64| Some(Control::Scroll(-n))).map_err(|_| "CPDB605");
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        return t.parse().map(|n| Some(Control::Record(n))).map_err(|_| "CPDB605");
    }
    let w = words(t);
    let verb = w[0].to_ascii_uppercase();
    let args = &w[1..];
    Ok(Some(match verb.as_str() {
        "T" | "TOP" => Control::Top,
        "B" | "BOT" | "BOTTOM" => Control::Bottom,
        "S" | "SAVE" => Control::Save,
        "FILE" => Control::File,
        "CAN" | "CANCEL" => Control::Cancel,
        "F" | "FIND" => match args.first() {
            Some(a) if !a.is_empty() => Control::Find(a.clone()),
            _ => return Err("CPFB612"),
        },
        "C" | "CHG" | "CHANGE" => match args {
            [from, to, rest @ ..] if !from.is_empty() => Control::Change {
                from: from.clone(),
                to: to.clone(),
                all: rest.iter().any(|r| r.eq_ignore_ascii_case("ALL")),
            },
            _ => return Err("CPFB612"),
        },
        _ => return Err("CPFB610"),
    }))
}

/// Where `what` is next, from line `from` (inclusive) onward, wrapping
/// to the top: `(line, character column)`. Case is ignored.
#[must_use]
pub fn find(lines: &[String], what: &str, from: usize) -> Option<(usize, usize)> {
    let w: Vec<char> = what.to_uppercase().chars().collect();
    if w.is_empty() || lines.is_empty() {
        return None;
    }
    let n = lines.len();
    for k in 0..n {
        let i = (from + k) % n;
        let l: Vec<char> = lines[i].to_uppercase().chars().collect();
        if let Some(c) = l.windows(w.len()).position(|win| win == w.as_slice()) {
            return Some((i, c));
        }
    }
    None
}

/// How many lines hold `what`.
#[must_use]
pub fn count(lines: &[String], what: &str) -> usize {
    let w = what.to_uppercase();
    lines
        .iter()
        .map(|l| l.to_uppercase().matches(&w).count())
        .sum()
}

/// `line` with the occurrence of `from` at character column `at`
/// (case ignored) replaced by `to`.
#[must_use]
pub fn replace_at(line: &str, at: usize, from: &str, to: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let n = from.chars().count();
    let mut out: String = chars[..at.min(chars.len())].iter().collect();
    out.push_str(to);
    out.extend(chars.iter().skip(at + n));
    out
}

/// Every occurrence of `from` (case ignored) replaced: the new line and
/// how many.
#[must_use]
pub fn replace_all(line: &str, from: &str, to: &str) -> (String, usize) {
    let f: Vec<char> = from.to_uppercase().chars().collect();
    let chars: Vec<char> = line.chars().collect();
    let up: Vec<char> = line.to_uppercase().chars().collect();
    if f.is_empty() || up.len() != chars.len() {
        return (line.to_string(), 0);
    }
    let mut out = String::new();
    let mut i = 0;
    let mut n = 0;
    while i < chars.len() {
        if up[i..].starts_with(&f) {
            out.push_str(to);
            i += f.len();
            n += 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    (out, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands() {
        assert_eq!(parse("t"), Ok(Some(Control::Top)));
        assert_eq!(parse("+5"), Ok(Some(Control::Scroll(5))));
        assert_eq!(parse("12"), Ok(Some(Control::Record(12))));
        assert_eq!(parse("f 'line t'"), Ok(Some(Control::Find("line t".into()))));
        assert_eq!(
            parse("c two TWO all"),
            Ok(Some(Control::Change {
                from: "two".into(),
                to: "TWO".into(),
                all: true
            }))
        );
        assert_eq!(parse("c x ''"), Ok(Some(Control::Change { from: "x".into(), to: String::new(), all: false })));
        assert_eq!(parse("zap"), Err("CPFB610"));
        assert_eq!(parse("f"), Err("CPFB612"));
    }

    #[test]
    fn find_and_change() {
        let l: Vec<String> = vec!["Quartz IFS probe".into(), "line two".into()];
        assert_eq!(find(&l, "TWO", 0), Some((1, 5)));
        assert_eq!(find(&l, "probe", 1), Some((0, 11)));
        assert_eq!(count(&l, "e"), 2);
        assert_eq!(replace_at("line two", 5, "TWO", "2"), "line 2");
        assert_eq!(replace_all("a-a-A", "a", "b"), ("b-b-b".into(), 3));
    }
}
