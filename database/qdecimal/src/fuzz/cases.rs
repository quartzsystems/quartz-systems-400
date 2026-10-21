// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Case generation: a systematic set over every shape pair, every edit
//! code and word, then random operands from a seeded generator.

use super::{Case, EDIT_CODES, EDIT_SHAPES, FillKind, Op, SHAPES, WORDS};

/// Xorshift64, enough for operand digits.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(n.max(1)).expect("fits")).expect("fits")
    }
}

/// `digits` as text at `scale`, with a leading `0` before a bare point.
fn render(neg: bool, digits: &[u8], scale: u8) -> String {
    let scale = usize::from(scale);
    let int = &digits[..digits.len() - scale];
    let frac = &digits[digits.len() - scale..];
    let int: String = int.iter().map(|d| char::from(b'0' + d)).collect();
    let int = int.trim_start_matches('0');
    let mut s = String::new();
    if neg && digits.iter().any(|d| *d != 0) {
        s.push('-');
    }
    s.push_str(if int.is_empty() { "0" } else { int });
    if scale > 0 {
        s.push('.');
        s.extend(frac.iter().map(|d| char::from(b'0' + d)));
    }
    s
}

/// The largest value of a shape.
fn max(shape: usize) -> String {
    let (d, s) = SHAPES[shape];
    render(false, &vec![9u8; usize::from(d)], s)
}

/// A value with a few named kinds and otherwise random digits.
fn operand(rng: &mut Rng, shape: usize) -> String {
    let (d, s) = SHAPES[shape];
    let n = usize::from(d);
    match rng.below(12) {
        0 => render(false, &vec![0u8; n], s),
        1 => max(shape),
        2 => format!("-{}", max(shape)),
        3 => render(false, &one(n, n - 1), s),
        4 => render(true, &one(n, n - 1), s),
        5 => render(false, &one(n, rng.below(n)), s),
        _ => {
            let used = 1 + rng.below(n);
            let mut digits = vec![0u8; n];
            for d in &mut digits[n - used..] {
                *d = u8::try_from(rng.below(10)).expect("digit");
            }
            if rng.below(3) == 0 {
                let zeros = rng.below(used);
                for d in &mut digits[n - zeros..] {
                    *d = 0;
                }
            }
            render(rng.below(2) == 0, &digits, s)
        }
    }
}

/// A single 1 at position `at` of `n` digits.
fn one(n: usize, at: usize) -> Vec<u8> {
    let mut v = vec![0u8; n];
    v[at] = 1;
    v
}

/// The whole case list: systematic cases first, then `random` more.
#[must_use]
pub fn generate(seed: u64, random: usize) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut push = |op: Op, sa: usize, a: String, sb: usize, b: String| {
        cases.push(Case {
            id: u32::try_from(cases.len() + 1).expect("fits"),
            op,
            shape_a: sa,
            a,
            shape_b: sb,
            b,
        });
    };
    for sa in 0..SHAPES.len() {
        for sb in 0..SHAPES.len() {
            for op in [Op::Add, Op::Sub, Op::Mul, Op::Div] {
                let pairs: &[(&str, &str)] = &[
                    (&max(sa), &max(sb)),
                    (&max(sa), "-1"),
                    ("0", "1"),
                    ("1", "3"),
                    ("-1", "7"),
                ];
                for (a, b) in pairs {
                    push(op, sa, fit(a, sa), sb, fit(b, sb));
                }
                if op == Op::Div {
                    push(op, sa, fit("1", sa), sb, fit("0", sb));
                }
            }
        }
    }
    let values = ["0", "1", "-1", "12.34", "-1234567.89", "max", "-max"];
    for &sa in &EDIT_SHAPES {
        for code in EDIT_CODES.chars() {
            if code == 'Y' && !matches!(SHAPES[sa], (3..=9, 0)) {
                continue;
            }
            let fills: &[FillKind] = if "XYZ".contains(code) {
                &[FillKind::Blank]
            } else {
                &[FillKind::Blank, FillKind::Asterisk, FillKind::Currency]
            };
            for &fill in fills {
                let op = Op::EditCode { code, fill };
                for v in values {
                    let a = edit_value(v, sa);
                    push(op, sa, a, 0, op.edit_text().expect("edit"));
                }
            }
        }
    }
    for (word, &(sa, _)) in WORDS.iter().enumerate() {
        let op = Op::EditWord { word };
        for v in values {
            push(op, sa, edit_value(v, sa), 0, op.edit_text().expect("edit"));
        }
    }
    let mut rng = Rng(seed.max(1));
    for _ in 0..random {
        let sa = rng.below(SHAPES.len());
        let sb = rng.below(SHAPES.len());
        let op = [Op::Add, Op::Sub, Op::Mul, Op::Div][rng.below(4)];
        let a = operand(&mut rng, sa);
        let b = operand(&mut rng, sb);
        push(op, sa, a, sb, b);
    }
    cases
}

fn edit_value(v: &str, shape: usize) -> String {
    match v {
        "max" => max(shape),
        "-max" => format!("-{}", max(shape)),
        v => fit(v, shape),
    }
}

/// `text` written at the shape's scale, truncated to what fits.
fn fit(text: &str, shape: usize) -> String {
    let (d, s) = SHAPES[shape];
    let neg = text.starts_with('-');
    let t = text.trim_start_matches('-');
    let (int, frac) = t.split_once('.').unwrap_or((t, ""));
    let mut frac: String = frac.chars().take(usize::from(s)).collect();
    while frac.len() < usize::from(s) {
        frac.push('0');
    }
    let max_int = usize::from(d - s);
    let int = if int.len() > max_int {
        &int[int.len() - max_int..]
    } else {
        int
    };
    let int = int.trim_start_matches('0');
    let int = if int.is_empty() { "0" } else { int };
    let zero = int == "0" && frac.chars().all(|c| c == '0');
    let mut out = String::new();
    if neg && !zero {
        out.push('-');
    }
    out.push_str(int);
    if s > 0 {
        out.push('.');
        out.push_str(&frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arith::Typed;
    use crate::decimal::Decimal;

    #[test]
    fn deterministic_and_fitting() {
        let a = generate(42, 500);
        let b = generate(42, 500);
        assert_eq!(a, b);
        assert_ne!(generate(43, 500), a);
        assert!(a.len() > 500);
        for (i, c) in a.iter().enumerate() {
            assert_eq!(c.id, u32::try_from(i + 1).unwrap());
            let v = Decimal::parse(&c.a).unwrap();
            Typed::new(&v, super::super::shape(c.shape_a)).unwrap();
            assert_eq!(v.scale(), SHAPES[c.shape_a].1, "{c:?}");
            if c.op.is_arithmetic() {
                let w = Decimal::parse(&c.b).unwrap();
                Typed::new(&w, super::super::shape(c.shape_b)).unwrap();
            }
        }
    }

    #[test]
    fn fit_writes_the_scale() {
        assert_eq!(fit("12.34", 1), "12");
        assert_eq!(fit("-1", 0), "-1.00");
        assert_eq!(fit("0", 2), "0.0000");
        assert_eq!(fit("-1234567.89", 0), "-567.89");
        assert_eq!(max(0), "999.99");
        assert_eq!(render(true, &[0, 0, 5, 0], 2), "-0.50");
    }
}
