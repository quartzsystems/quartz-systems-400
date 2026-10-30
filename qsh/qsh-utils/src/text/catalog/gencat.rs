// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `gencat catfile msgfile...`: build a message catalog from X/Open
//! source, merging into `catfile` when it already holds one (`-` is
//! standard output for the catalog and standard input for a source).
//!
//! Source lines: `$set n`, `$delset n`, `$quote c` (none when `c` is
//! missing), `$` followed by a blank (a comment), `number text` (an
//! empty text after the separator is an empty message; a number alone
//! deletes the message), blank lines. A line ending in `\` continues;
//! escapes are `\n \t \v \b \r \f \\ \ddd` and `\` before the quote.

use qsh::sys::UtilCtx;
use qsh::sys::stream::write_all;

use super::format::{Catalog, MSG_MAX, SET_DEFAULT, SET_MAX};
use crate::common::io::{open_write, read_input};
use crate::common::{complain, complain_sys, usage};

const NAME: &str = "gencat";

/// Resolve escapes in message text (and the quote, when one is set).
fn unescape(text: &[u8], quote: Option<u8>) -> Result<Vec<u8>, String> {
    let mut t = text;
    let quoted = quote.is_some_and(|q| t.first() == Some(&q));
    if quoted {
        t = &t[1..];
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let b = t[i];
        if quoted && Some(b) == quote {
            // The closing quote ends the text.
            return Ok(out);
        }
        if b != b'\\' || i + 1 == t.len() {
            out.push(b);
            i += 1;
            continue;
        }
        let e = t[i + 1];
        i += 2;
        match e {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            b'b' => out.push(0x08),
            b'r' => out.push(b'\r'),
            b'f' => out.push(0x0c),
            b'\\' => out.push(b'\\'),
            b'0'..=b'7' => {
                let mut v = u32::from(e - b'0');
                let mut n = 1;
                while n < 3 && i < t.len() && (b'0'..=b'7').contains(&t[i]) {
                    v = v * 8 + u32::from(t[i] - b'0');
                    i += 1;
                    n += 1;
                }
                out.push((v & 0xff) as u8);
            }
            other if Some(other) == quote => out.push(other),
            other => out.push(other),
        }
    }
    if quoted {
        return Err("The quoted text is not closed.".to_string());
    }
    Ok(out)
}

fn number(word: &[u8], max: u32, what: &str) -> Result<u32, String> {
    let s = String::from_utf8_lossy(word);
    match s.parse::<u32>() {
        Ok(n) if (1..=max).contains(&n) => Ok(n),
        _ => Err(format!("{s} is not a valid {what} number (1 to {max}).")),
    }
}

/// The lines of a source, continuations joined, with their first line
/// numbers.
fn logical_lines(src: &[u8]) -> Vec<(usize, Vec<u8>)> {
    let mut out = Vec::new();
    let mut cur: Option<(usize, Vec<u8>)> = None;
    for (n, raw) in src.split(|&b| b == b'\n').enumerate() {
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        let (start, mut text) = cur.take().unwrap_or((n + 1, Vec::new()));
        text.extend_from_slice(raw);
        let trailing = text.iter().rev().take_while(|&&b| b == b'\\').count();
        if trailing % 2 == 1 {
            text.pop();
            cur = Some((start, text));
        } else {
            out.push((start, text));
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out
}

fn is_blank(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Apply one source to a catalog.
pub fn apply(cat: &mut Catalog, src: &[u8]) -> Result<(), (usize, String)> {
    let mut set = SET_DEFAULT;
    let mut quote: Option<u8> = None;
    for (ln, line) in logical_lines(src) {
        let err = |m: String| (ln, m);
        if line.iter().all(|&b| is_blank(b)) {
            continue;
        }
        if let Some(rest) = line.strip_prefix(b"$") {
            let word_end = rest.iter().position(|&b| is_blank(b)).unwrap_or(rest.len());
            let (word, args) = rest.split_at(word_end);
            let args: Vec<&[u8]> = args
                .split(|&b| is_blank(b))
                .filter(|w| !w.is_empty())
                .collect();
            match word {
                b"set" => {
                    let w = args.first().ok_or_else(|| err("$set needs a number.".into()))?;
                    set = number(w, SET_MAX, "set").map_err(err)?;
                    cat.sets.entry(set).or_default();
                }
                b"delset" => {
                    let w = args.first().ok_or_else(|| err("$delset needs a number.".into()))?;
                    let n = number(w, SET_MAX, "set").map_err(err)?;
                    cat.sets.remove(&n);
                }
                b"quote" => {
                    let after = &rest[word_end..];
                    quote = after.iter().copied().find(|&b| !is_blank(b));
                }
                // `$ comment`, `$` alone and unknown words are comments.
                _ => {}
            }
            continue;
        }
        let digits = line.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 {
            return Err(err(format!(
                "The line does not start with a message number: {}",
                String::from_utf8_lossy(&line)
            )));
        }
        let num = number(&line[..digits], MSG_MAX, "message").map_err(err)?;
        let rest = &line[digits..];
        match rest.first() {
            None => {
                if let Some(m) = cat.sets.get_mut(&set) {
                    m.remove(&num);
                }
            }
            Some(&b) if is_blank(b) => {
                let text = unescape(&rest[1..], quote).map_err(err)?;
                cat.sets.entry(set).or_default().insert(num, text);
            }
            Some(_) => {
                return Err(err("A blank must follow the message number.".to_string()));
            }
        }
    }
    Ok(())
}

/// Run `gencat`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let args = &cx.argv[1..];
    let args = if args.first().is_some_and(|a| a == "--") {
        &args[1..]
    } else {
        args
    };
    if args.len() < 2 {
        return usage(cx, NAME, "A catalog and at least one source file are needed.");
    }
    let catfile = args[0].clone();
    let sources = args[1..].to_vec();
    let mut cat = Catalog::default();
    if catfile != "-" {
        let abs = cx.path(&catfile);
        if let Ok(st) = cx.sys.stat(&abs)
            && st.size > 0
        {
            match read_input(cx, &catfile) {
                Ok(b) => match Catalog::decode(&b) {
                    Some(c) => cat = c,
                    None => {
                        complain(cx, NAME, &format!("{catfile}: The file is not a message catalog."));
                        return 1;
                    }
                },
                Err(e) => {
                    complain_sys(cx, NAME, &catfile, e);
                    return 1;
                }
            }
        }
    }
    for s in &sources {
        let src = match read_input(cx, s) {
            Ok(b) => b,
            Err(e) => {
                complain_sys(cx, NAME, s, e);
                return 1;
            }
        };
        if let Err((ln, m)) = apply(&mut cat, &src) {
            let shown = if s == "-" { "standard input" } else { s };
            complain(cx, NAME, &format!("{shown}: line {ln}: {m}"));
            return 1;
        }
    }
    let bytes = cat.encode();
    let r = if catfile == "-" {
        match cx.fd(1) {
            Some(fd) => write_all(&**fd, &bytes).map_err(Into::into),
            None => Ok(()),
        }
    } else {
        open_write(cx, &catfile, false)
            .and_then(|fd| write_all(&*fd, &bytes).map_err(Into::into))
    };
    match r {
        Ok(()) => 0,
        Err(e) => {
            complain_sys(cx, NAME, &catfile, e);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source() {
        let mut c = Catalog::default();
        apply(
            &mut c,
            b"$ comment\n1 one\n2 tw\\\no\n$set 2\n$quote \"\n1 \"quoted \\\" x\" tail\n3 a\\tb\\101\n",
        )
        .unwrap();
        assert_eq!(c.get(1, 1), Some(&b"one"[..]));
        assert_eq!(c.get(1, 2), Some(&b"two"[..]));
        assert_eq!(c.get(2, 1), Some(&b"quoted \" x"[..]));
        assert_eq!(c.get(2, 3), Some(&b"a\tbA"[..]));
        apply(&mut c, b"$set 2\n1\n$delset 1\n").unwrap();
        assert_eq!(c.get(2, 1), None);
        assert_eq!(c.get(1, 1), None);
        assert!(apply(&mut c, b"x y\n").is_err());
        assert!(apply(&mut c, b"$set 0\n").is_err());
    }
}
