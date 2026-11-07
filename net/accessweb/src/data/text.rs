// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Delimited (CSV, TSV) and fixed-width records, and the text
//! encodings of transfer files. NULL and an empty string stay apart
//! both ways in delimited files: NULL is nothing, an empty string `""`.

use super::files::Encoding;
use crate::env::db::Val;

/// `s` in `enc`; a character the code page lacks is `?` (or the
/// substitute character in CCSID 37).
pub fn encode(s: &str, enc: Encoding) -> Vec<u8> {
    match enc {
        Encoding::Utf8 => s.as_bytes().to_vec(),
        Encoding::Cp1252 => s.chars().map(|c| cp1252_byte(c).unwrap_or(b'?')).collect(),
        Encoding::Cp37 => s.chars().map(|c| ccsid::byte_of(c).unwrap_or(0x3F)).collect(),
    }
}

/// Bytes in `enc` as text; UTF-8 with or without a byte order mark.
pub fn decode(b: &[u8], enc: Encoding) -> Result<String, String> {
    match enc {
        Encoding::Utf8 => {
            let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b);
            String::from_utf8(b.to_vec()).map_err(|e| format!("the file is not UTF-8 at byte {}", e.utf8_error().valid_up_to()))
        }
        Encoding::Cp1252 => Ok(b.iter().map(|x| cp1252_char(*x)).collect()),
        Encoding::Cp37 => Ok(ccsid::from_ebcdic(b)),
    }
}

/// Windows-1252's characters at 0x80-0x9F; the rest is Latin-1.
const HIGH: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2C6}', '\u{2030}', '\u{160}',
    '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}',
    '\u{2013}', '\u{2014}', '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
];

fn cp1252_char(b: u8) -> char {
    if (0x80..0xA0).contains(&b) { HIGH[usize::from(b - 0x80)] } else { char::from(b) }
}

fn cp1252_byte(c: char) -> Option<u8> {
    let n = u32::from(c);
    if n < 0x80 || (0xA0..0x100).contains(&n) {
        return u8::try_from(n).ok();
    }
    HIGH.iter().position(|h| *h == c).and_then(|i| u8::try_from(0x80 + i).ok())
}

/// One delimited field.
fn field(v: &Val, delim: char) -> String {
    match v {
        Val::Null => String::new(),
        Val::Num(s) => s.clone(),
        other => {
            let t = other.text();
            if t.is_empty() || t.contains([delim, '"', '\n', '\r']) || t.starts_with(' ') || t.ends_with(' ') {
                format!("\"{}\"", t.replace('"', "\"\""))
            } else {
                t
            }
        }
    }
}

/// Delimited text: an optional header row, then the rows.
pub fn delimited(header: Option<&[String]>, rows: &[Vec<Val>], delim: char, eol: &str) -> String {
    let mut out = String::new();
    if let Some(h) = header {
        let line: Vec<String> = h.iter().map(|n| field(&Val::Text(n.clone()), delim)).collect();
        out.push_str(&line.join(&delim.to_string()));
        out.push_str(eol);
    }
    for r in rows {
        let line: Vec<String> = r.iter().map(|v| field(v, delim)).collect();
        out.push_str(&line.join(&delim.to_string()));
        out.push_str(eol);
    }
    out
}

/// The records of delimited text: a quoted field is text (an empty one
/// the empty string), an unquoted empty field NULL.
pub fn parse_delimited(text: &str, delim: char) -> Result<Vec<Vec<Val>>, String> {
    let mut rows = Vec::new();
    let mut row: Vec<Val> = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut line = 1usize;
    let mut started = false;
    while let Some(c) = chars.next() {
        started = true;
        match c {
            '"' if cur.is_empty() && !quoted => {
                quoted = true;
                loop {
                    match chars.next() {
                        Some('"') if chars.peek() == Some(&'"') => {
                            chars.next();
                            cur.push('"');
                        }
                        Some('"') => break,
                        Some(ch) => {
                            if ch == '\n' {
                                line += 1;
                            }
                            cur.push(ch);
                        }
                        None => return Err(format!("a quoted field that starts on line {line} has no closing quote")),
                    }
                }
            }
            c if c == delim => {
                row.push(take(&mut cur, &mut quoted));
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                line += 1;
                row.push(take(&mut cur, &mut quoted));
                rows.push(std::mem::take(&mut row));
                started = false;
            }
            c => cur.push(c),
        }
    }
    if started {
        row.push(take(&mut cur, &mut quoted));
        rows.push(row);
    }
    Ok(rows)
}

fn take(cur: &mut String, quoted: &mut bool) -> Val {
    let s = std::mem::take(cur);
    let q = std::mem::replace(quoted, false);
    if s.is_empty() && !q { Val::Null } else { Val::Text(s) }
}

/// A fixed-width value: numbers right-aligned, the rest left.
fn pad(s: &str, width: usize, right: bool) -> String {
    let n = s.chars().count();
    let fill = " ".repeat(width.saturating_sub(n));
    if right { format!("{fill}{s}") } else { format!("{s}{fill}") }
}

/// Fixed-width text: each column as wide as its widest value or name,
/// one blank between columns; NULL is blanks.
pub fn fixed(header: Option<&[String]>, numeric: &[bool], rows: &[Vec<Val>], eol: &str) -> String {
    let cols = numeric.len();
    let mut widths = vec![0usize; cols];
    if let Some(h) = header {
        for (w, n) in widths.iter_mut().zip(h) {
            *w = (*w).max(n.chars().count());
        }
    }
    for r in rows {
        for (w, v) in widths.iter_mut().zip(r) {
            *w = (*w).max(v.text().chars().count());
        }
    }
    let mut out = String::new();
    let mut put = |cells: Vec<String>| {
        out.push_str(cells.join(" ").trim_end());
        out.push_str(eol);
    };
    if let Some(h) = header {
        put(h.iter().zip(&widths).map(|(n, w)| pad(n, *w, false)).collect());
    }
    for r in rows {
        put(r.iter().zip(&widths).zip(numeric).map(|((v, w), num)| pad(&v.text(), *w, *num)).collect());
    }
    out
}

/// Fixed-width records: the header row's names say where each column
/// starts; a field of blanks is NULL.
pub fn parse_fixed(text: &str) -> Result<(Vec<String>, Vec<Vec<Val>>), String> {
    let mut lines = text.lines();
    let head = lines.next().ok_or("the file is empty")?;
    let chars: Vec<char> = head.chars().collect();
    let mut starts = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        if *c != ' ' && (i == 0 || chars[i - 1] == ' ') {
            starts.push(i);
        }
    }
    if starts.is_empty() {
        return Err("the first row names no columns".into());
    }
    let cut = |line: &str| -> Vec<String> {
        let cs: Vec<char> = line.chars().collect();
        starts
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let e = starts.get(k + 1).copied().unwrap_or(cs.len()).min(cs.len());
                cs.get(*s..e.max(*s)).map(|x| x.iter().collect::<String>()).unwrap_or_default().trim().to_string()
            })
            .collect()
    };
    let names = cut(head);
    let rows = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| cut(l).into_iter().map(|s| if s.is_empty() { Val::Null } else { Val::Text(s) }).collect())
        .collect();
    Ok((names, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delimited_both_ways() {
        let rows = vec![vec![Val::Text("a,b".into()), Val::Null, Val::Text(String::new()), Val::Num("1200.00".into())]];
        let t = delimited(Some(&["x".into(), "y".into(), "z".into(), "n".into()]), &rows, ',', "\r\n");
        assert_eq!(t, "x,y,z,n\r\n\"a,b\",,\"\",1200.00\r\n");
        let back = parse_delimited(&t, ',').unwrap();
        assert_eq!(back[1], vec![Val::Text("a,b".into()), Val::Null, Val::Text(String::new()), Val::Text("1200.00".into())]);
        assert!(parse_delimited("\"open", ',').is_err());
        assert_eq!(parse_delimited("a\tb", '\t').unwrap(), vec![vec![Val::Text("a".into()), Val::Text("b".into())]]);
    }

    #[test]
    fn fixed_both_ways() {
        let rows = vec![vec![Val::Text("QS1".into()), Val::Num("4".into())], vec![Val::Null, Val::Num("12".into())]];
        let t = fixed(Some(&["TAG".into(), "N".into()]), &[false, true], &rows, "\n");
        assert_eq!(t, "TAG N\nQS1  4\n    12\n");
        let (names, back) = parse_fixed(&t).unwrap();
        assert_eq!(names, ["TAG", "N"]);
        assert_eq!(back[1], vec![Val::Null, Val::Text("12".into())]);
    }

    #[test]
    fn code_pages() {
        assert_eq!(encode("\u{20AC}é", Encoding::Cp1252), [0x80, 0xE9]);
        assert_eq!(decode(&[0x80, 0xE9], Encoding::Cp1252).unwrap(), "\u{20AC}é");
        assert_eq!(decode(&encode("AB", Encoding::Cp37), Encoding::Cp37).unwrap(), "AB");
        assert_eq!(decode(b"\xEF\xBB\xBFok", Encoding::Utf8).unwrap(), "ok");
    }
}
