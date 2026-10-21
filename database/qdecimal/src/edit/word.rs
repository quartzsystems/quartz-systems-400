// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Edit words: body (digit positions and insertion characters), status
//! (`CR` or `-`) and expansion.

use super::{bad, digit_char, digits_of};
use crate::decimal::{Decimal, Shape};
use crate::error::Result;

/// The parts of an edit word.
struct Parsed {
    body: Vec<char>,
    /// Index in `body` of the zero-suppression stop, if any.
    stop: Option<usize>,
    /// Index in `body` of a floating currency symbol, if any.
    floating: Option<usize>,
    status: Status,
    expansion: Vec<char>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    None,
    Cr,
    Minus,
}

fn is_digit_position(c: char) -> bool {
    matches!(c, ' ' | '0' | '*')
}

/// Split the word for a field of `digits` digit positions.
fn parse(word: &str, digits: usize) -> Result<Parsed> {
    let chars: Vec<char> = word.chars().collect();
    let mut body = Vec::new();
    let mut stop = None;
    let mut floating = None;
    let mut positions = 0;
    let mut at = 0;
    while at < chars.len() && positions < digits {
        let c = chars[at];
        if is_digit_position(c) {
            if c == '0' && stop.is_none() {
                stop = Some(body.len());
                if body.last() == Some(&'$') && body.len() > 1 {
                    floating = Some(body.len() - 1);
                }
            }
            positions += 1;
        }
        body.push(c);
        at += 1;
    }
    if positions < digits {
        return Err(bad(format!(
            "edit word {word:?} has {positions} digit positions for a {digits}-digit field"
        )));
    }
    // Status: blanks (`&`) may lead to the CR or minus.
    let mut status = Status::None;
    let mut probe = at;
    while probe < chars.len() && chars[probe] == '&' {
        probe += 1;
    }
    if chars.get(probe) == Some(&'C') && chars.get(probe + 1) == Some(&'R') {
        status = Status::Cr;
        body.extend(&chars[at..probe]);
        at = probe + 2;
    } else if chars.get(probe) == Some(&'-') {
        status = Status::Minus;
        body.extend(&chars[at..probe]);
        at = probe + 1;
    }
    Ok(Parsed {
        body,
        stop,
        floating,
        status,
        expansion: chars[at..].to_vec(),
    })
}

/// Edit `value` as a field of `shape` with `word`.
pub fn edit_word(value: &Decimal, shape: Shape, word: &str) -> Result<String> {
    let (neg, digits) = digits_of(value, shape)?;
    let p = parse(word, digits.len())?;
    // Any asterisk among the digit positions makes asterisks the fill.
    let fill = if p.body.contains(&'*') { '*' } else { ' ' };
    let mut out = String::with_capacity(word.len());
    let mut next = digits.iter().copied();
    let mut significant = false;
    let mut first_digit_at: Option<usize> = None;
    for (i, &c) in p.body.iter().enumerate() {
        if Some(i) == p.floating {
            // The symbol's own position is given to the digits; the
            // symbol lands just left of the first one printed.
            continue;
        }
        let at_stop = p.stop.is_some_and(|s| i >= s);
        if is_digit_position(c) {
            let d = next.next().expect("one digit per position");
            if d != 0 || significant || at_stop {
                significant = true;
                first_digit_at.get_or_insert(out.chars().count());
                out.push(digit_char(d));
            } else {
                out.push(fill);
            }
        } else if c == '&' {
            out.push(' ');
        } else if significant || at_stop || i == 0 {
            // Constants print once a digit has, and a leading constant
            // (a fixed currency symbol) always.
            out.push(c);
        } else {
            out.push(fill);
        }
    }
    if let Some(fi) = p.floating {
        let mut chars: Vec<char> = out.chars().collect();
        chars.insert(first_digit_at.unwrap_or(0), p.body[fi]);
        out = chars.into_iter().collect();
    }
    match p.status {
        Status::None => {}
        Status::Cr => out.push_str(if neg { "CR" } else { "  " }),
        Status::Minus => out.push(if neg { '-' } else { ' ' }),
    }
    for c in &p.expansion {
        out.push(if *c == '&' { ' ' } else { *c });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(v: &str, digits: u8, scale: u8, word: &str) -> String {
        let shape = Shape::new(digits, scale).unwrap();
        edit_word(&Decimal::parse(v).unwrap(), shape, word).unwrap()
    }

    #[test]
    fn bodies() {
        assert_eq!(edit("1234.56", 7, 2, "  ,   .  "), " 1,234.56");
        assert_eq!(edit("34.56", 7, 2, "  ,   .  "), "    34.56");
        assert_eq!(edit("0.56", 7, 2, "  ,   .  "), "       56");
        assert_eq!(edit("0.56", 7, 2, "  ,  0.  "), "     0.56");
        assert_eq!(edit("0", 7, 2, "  ,  0.  "), "     0.00");
        assert_eq!(edit("0", 7, 2, "  ,   .  "), "         ");
        assert_eq!(edit("12", 5, 0, "  0  "), "  012");
        assert_eq!(edit("12", 5, 0, "**   "), "***12");
        assert_eq!(edit("12", 5, 0, "0    "), "00012");
    }

    #[test]
    fn status_and_expansion() {
        assert_eq!(edit("-12.34", 7, 2, "  ,   .  CR"), "    12.34CR");
        assert_eq!(edit("12.34", 7, 2, "  ,   .  CR"), "    12.34  ");
        assert_eq!(edit("-12.34", 7, 2, "  ,   .  -"), "    12.34-");
        assert_eq!(edit("-12.34", 7, 2, "  ,   .  &-"), "    12.34 -");
        assert_eq!(edit("12.34", 7, 2, "  ,   .  &TOTAL"), "    12.34 TOTAL");
        assert_eq!(edit("5", 3, 0, "  0&DAYS"), "  5 DAYS");
    }

    #[test]
    fn currency() {
        assert_eq!(edit("12.34", 7, 2, "$  ,   .  "), "$    12.34");
        assert_eq!(edit("12.34", 7, 2, "  ,  $0.  "), "    $12.34");
        assert_eq!(edit("1234.56", 7, 2, "  ,  $0.  "), " $1,234.56");
        assert_eq!(edit("0.5", 7, 2, "  ,  $0.  "), "     $0.50");
        assert_eq!(edit("12345.67", 7, 2, "  ,  $0.  "), "$12,345.67");
    }

    #[test]
    fn errors() {
        let shape = Shape::new(5, 0).unwrap();
        assert!(edit_word(&Decimal::parse("1").unwrap(), shape, "   ").is_err());
    }
}
