// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Edit codes `1`-`4`, `A`-`D`, `J`-`Q`, `X`, `Y`, `Z`.

use super::{DateEditFmt, EditOpts, Fill, bad, digit_char, digits_of};
use crate::decimal::{Decimal, Shape};
use crate::error::Result;

/// How a negative value shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sign {
    None,
    Cr,
    Trailing,
    Leading,
}

impl Sign {
    fn width(self) -> usize {
        match self {
            Self::None => 0,
            Self::Cr => 2,
            Self::Trailing | Self::Leading => 1,
        }
    }
}

/// What a code does.
#[derive(Clone, Copy)]
struct Spec {
    commas: bool,
    zero_balance: bool,
    sign: Sign,
}

fn spec(code: char) -> Option<Spec> {
    let (commas, zero_balance, sign) = match code {
        '1' => (true, true, Sign::None),
        '2' => (true, false, Sign::None),
        '3' => (false, true, Sign::None),
        '4' => (false, false, Sign::None),
        'A' => (true, true, Sign::Cr),
        'B' => (true, false, Sign::Cr),
        'C' => (false, true, Sign::Cr),
        'D' => (false, false, Sign::Cr),
        'J' => (true, true, Sign::Trailing),
        'K' => (true, false, Sign::Trailing),
        'L' => (false, true, Sign::Trailing),
        'M' => (false, false, Sign::Trailing),
        'N' => (true, true, Sign::Leading),
        'O' => (true, false, Sign::Leading),
        'P' => (false, true, Sign::Leading),
        'Q' => (false, false, Sign::Leading),
        _ => return None,
    };
    Some(Spec {
        commas,
        zero_balance,
        sign,
    })
}

fn comma_count(integer: u8) -> usize {
    usize::from(integer.saturating_sub(1)) / 3
}

/// Characters the edited value takes.
pub fn code_width(shape: Shape, code: char, opts: &EditOpts) -> Result<usize> {
    let digits = usize::from(shape.digits);
    if let Some(s) = spec(code) {
        let commas = if s.commas {
            comma_count(shape.integer())
        } else {
            0
        };
        let point = usize::from(shape.scale > 0);
        let currency = usize::from(matches!(opts.fill, Fill::Currency(_)));
        return Ok(digits + commas + point + s.sign.width() + currency);
    }
    match code {
        'X' | 'Z' => Ok(digits),
        'Y' => Ok(digits + date_groups(shape.digits, opts.date_fmt)?.len() - 1),
        _ => Err(bad(format!("{code:?} is not an edit code"))),
    }
}

/// Edit `value` as a field of `shape` with `code`.
pub fn edit_code(value: &Decimal, shape: Shape, code: char, opts: &EditOpts) -> Result<String> {
    let (neg, digits) = digits_of(value, shape)?;
    if let Some(s) = spec(code) {
        return Ok(numeric(&digits, neg, shape, s, opts));
    }
    match code {
        'X' => Ok(digits.iter().map(|&d| digit_char(d)).collect()),
        'Z' => Ok(suppress(&digits)),
        'Y' => date(&digits, opts),
        _ => Err(bad(format!("{code:?} is not an edit code"))),
    }
}

/// Leading zeros as blanks, nothing else.
fn suppress(digits: &[u8]) -> String {
    let mut out = String::with_capacity(digits.len());
    let mut significant = false;
    for &d in digits {
        significant |= d != 0;
        out.push(if significant { digit_char(d) } else { ' ' });
    }
    out
}

/// Codes `1`-`4`, `A`-`D`, `J`-`Q`.
fn numeric(digits: &[u8], neg: bool, shape: Shape, s: Spec, opts: &EditOpts) -> String {
    let width = code_width(shape, '1', opts).expect("valid") + s.sign.width()
        - usize::from(!s.commas) * comma_count(shape.integer());
    let zero = digits.iter().all(|&d| d == 0);
    if zero && !s.zero_balance {
        return " ".repeat(width);
    }
    let (fill, currency) = match opts.fill {
        Fill::Blank => (' ', None),
        Fill::Asterisk => ('*', None),
        Fill::Currency(c) => (' ', Some(c)),
    };
    let integer = usize::from(shape.integer());
    let (int, frac) = digits.split_at(integer);
    let mut body = String::with_capacity(width);
    let mut significant = false;
    for (i, &d) in int.iter().enumerate() {
        if i > 0 && s.commas && (integer - i).is_multiple_of(3) {
            body.push(if significant { ',' } else { fill });
        }
        let units = i == integer - 1;
        if d != 0 || significant || (units && shape.scale == 0) {
            significant = true;
            body.push(digit_char(d));
        } else {
            body.push(fill);
        }
    }
    if shape.scale > 0 {
        body.push('.');
        body.extend(frac.iter().map(|&d| digit_char(d)));
    }
    // The first printed character: a digit or the decimal point.
    let first = body.find(|c: char| c != fill).unwrap_or(0);
    if let Some(c) = currency {
        body.insert(first, c);
    }
    match s.sign {
        Sign::None => {}
        Sign::Cr => body.push_str(if neg { "CR" } else { "  " }),
        Sign::Trailing => body.push(if neg { '-' } else { ' ' }),
        Sign::Leading => body.insert(first, if neg { '-' } else { ' ' }),
    }
    body
}

/// Digit group sizes of edit code `Y` for a field of `n` digits.
fn date_groups(n: u8, fmt: DateEditFmt) -> Result<Vec<usize>> {
    Ok(match (n, fmt) {
        (3, _) => vec![1, 2],
        (4, _) => vec![2, 2],
        (5, _) => vec![1, 2, 2],
        (6, _) => vec![2, 2, 2],
        (7, _) => vec![3, 2, 2],
        (8, DateEditFmt::Ymd) => vec![4, 2, 2],
        (8, _) => vec![2, 2, 4],
        (9, DateEditFmt::Ymd) => vec![5, 2, 2],
        (9, _) => vec![3, 2, 4],
        _ => return Err(bad(format!("edit code Y takes 3 to 9 digits, not {n}"))),
    })
}

/// Edit code `Y`: separators between the groups, the first zero blank.
fn date(digits: &[u8], opts: &EditOpts) -> Result<String> {
    let n = u8::try_from(digits.len()).expect("at most 63");
    let groups = date_groups(n, opts.date_fmt)?;
    let mut out = String::new();
    let mut at = 0;
    for (g, &len) in groups.iter().enumerate() {
        if g > 0 {
            out.push(opts.separator());
        }
        for &d in &digits[at..at + len] {
            out.push(if at == 0 && d == 0 && out.is_empty() {
                ' '
            } else {
                digit_char(d)
            });
            at += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(v: &str, digits: u8, scale: u8, code: char) -> String {
        let shape = Shape::new(digits, scale).unwrap();
        edit_code(
            &Decimal::parse(v).unwrap(),
            shape,
            code,
            &EditOpts::default(),
        )
        .unwrap()
    }

    fn edit_with(v: &str, digits: u8, scale: u8, code: char, fill: Fill) -> String {
        let shape = Shape::new(digits, scale).unwrap();
        let opts = EditOpts {
            fill,
            ..EditOpts::default()
        };
        edit_code(&Decimal::parse(v).unwrap(), shape, code, &opts).unwrap()
    }

    #[test]
    fn commas_points_and_signs() {
        assert_eq!(edit("1234567.89", 9, 2, '1'), "1,234,567.89");
        assert_eq!(edit("-1234567.89", 9, 2, '1'), "1,234,567.89");
        assert_eq!(edit("12.5", 9, 2, '1'), "       12.50");
        assert_eq!(edit("0.05", 7, 2, '1'), "      .05");
        assert_eq!(edit("0", 7, 2, '1'), "      .00");
        assert_eq!(edit("0", 7, 2, '2'), "         ");
        assert_eq!(edit("0", 5, 0, '3'), "    0");
        assert_eq!(edit("0", 5, 0, '4'), "     ");
        assert_eq!(edit("-1234.5", 7, 1, 'A'), "  1,234.5CR");
        assert_eq!(edit("1234.5", 7, 1, 'A'), "  1,234.5  ");
        assert_eq!(edit("-1234.5", 7, 1, 'C'), "  1234.5CR");
        assert_eq!(edit("-12.34", 7, 2, 'J'), "    12.34-");
        assert_eq!(edit("12.34", 7, 2, 'J'), "    12.34 ");
        assert_eq!(edit("-12.34", 7, 2, 'L'), "   12.34-");
        assert_eq!(edit("-12.34", 7, 2, 'N'), "    -12.34");
        assert_eq!(edit("12.34", 7, 2, 'N'), "     12.34");
        assert_eq!(edit("-1234567", 7, 0, 'N'), "-1,234,567");
        assert_eq!(edit("-5", 1, 0, 'P'), "-5");
    }

    #[test]
    fn widths_match() {
        for code in "1234ABCDJKLMNOPQXZ".chars() {
            for &(d, s) in &[(1u8, 0u8), (7, 2), (9, 0), (15, 5), (63, 30)] {
                let shape = Shape::new(d, s).unwrap();
                for fill in [Fill::Blank, Fill::Asterisk, Fill::Currency('$')] {
                    let opts = EditOpts {
                        fill,
                        ..EditOpts::default()
                    };
                    for v in ["0", "-1", "9"] {
                        let s = edit_code(&Decimal::parse(v).unwrap(), shape, code, &opts).unwrap();
                        assert_eq!(
                            s.chars().count(),
                            code_width(shape, code, &opts).unwrap(),
                            "{code} {shape} {v:?} {s:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn fills() {
        assert_eq!(
            edit_with("12.34", 9, 2, 'J', Fill::Asterisk),
            "*******12.34 "
        );
        assert_eq!(
            edit_with("12.34", 9, 2, 'J', Fill::Currency('$')),
            "       $12.34 "
        );
        assert_eq!(
            edit_with("0.05", 7, 2, '1', Fill::Currency('$')),
            "      $.05"
        );
        assert_eq!(
            edit_with("1234567", 7, 0, '1', Fill::Currency('$')),
            "$1,234,567"
        );
        assert_eq!(
            edit_with("-12.34", 7, 2, 'N', Fill::Currency('$')),
            "    -$12.34"
        );
        assert_eq!(edit_with("0", 7, 2, '2', Fill::Asterisk), "         ");
    }

    #[test]
    fn x_y_z() {
        assert_eq!(edit("-123", 5, 0, 'X'), "00123");
        assert_eq!(edit("12.3", 5, 1, 'Z'), "  123");
        assert_eq!(edit("0", 5, 1, 'Z'), "     ");
        assert_eq!(edit("10526", 6, 0, 'Y'), " 1/05/26");
        assert_eq!(edit("123199", 6, 0, 'Y'), "12/31/99");
        assert_eq!(edit("20260101", 8, 0, 'Y'), "20/26/0101");
        let shape = Shape::new(8, 0).unwrap();
        let opts = EditOpts {
            date_fmt: DateEditFmt::Ymd,
            date_sep: Some('-'),
            ..EditOpts::default()
        };
        assert_eq!(
            edit_code(&Decimal::parse("20260101").unwrap(), shape, 'Y', &opts).unwrap(),
            "2026-01-01"
        );
        assert!(
            edit_code(
                &Decimal::parse("1").unwrap(),
                Shape::new(2, 0).unwrap(),
                'Y',
                &opts
            )
            .is_err()
        );
        assert!(edit_code(&Decimal::parse("1").unwrap(), shape, 'E', &opts).is_err());
    }
}
