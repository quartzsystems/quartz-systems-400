// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `expr` against a reference evaluator over random flat expressions,
//! and `pr` pagination invariants over random inputs.

#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]

mod common;

use qsh_utils::proc::expr::{Value, eval};

/// A small deterministic generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const OPS: &[&str] = &["|", "&", "=", "!=", "<", "<=", ">", ">=", "+", "-", "*", "/", "%"];

fn prec(op: &str) -> u8 {
    match op {
        "|" => 1,
        "&" => 2,
        "=" | "!=" | "<" | "<=" | ">" | ">=" => 3,
        "+" | "-" => 4,
        _ => 5,
    }
}

/// Reference: shunting-yard over integers, `None` for an error.
fn reference(toks: &[String]) -> Option<i64> {
    fn apply(op: &str, a: i64, b: i64) -> Option<i64> {
        Some(match op {
            "|" => {
                if a != 0 {
                    a
                } else {
                    b
                }
            }
            "&" => {
                if a != 0 && b != 0 {
                    a
                } else {
                    0
                }
            }
            "=" => i64::from(a == b),
            "!=" => i64::from(a != b),
            "<" => i64::from(a < b),
            "<=" => i64::from(a <= b),
            ">" => i64::from(a > b),
            ">=" => i64::from(a >= b),
            "+" => a.checked_add(b)?,
            "-" => a.checked_sub(b)?,
            "*" => a.checked_mul(b)?,
            "/" => a.checked_div(b)?,
            _ => a.checked_rem(b)?,
        })
    }
    let mut vals: Vec<i64> = Vec::new();
    let mut ops: Vec<&str> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if i % 2 == 0 {
            vals.push(t.parse().ok()?);
            continue;
        }
        while let Some(&top) = ops.last() {
            if prec(top) < prec(t) {
                break;
            }
            ops.pop();
            let b = vals.pop()?;
            let a = vals.pop()?;
            vals.push(apply(top, a, b)?);
        }
        ops.push(t);
    }
    while let Some(top) = ops.pop() {
        let b = vals.pop()?;
        let a = vals.pop()?;
        vals.push(apply(top, a, b)?);
    }
    vals.pop()
}

#[test]
fn expr_matches_reference() {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let mut checked = 0;
    for _ in 0..20_000 {
        let n = 1 + r.below(6) as usize;
        let mut toks = Vec::new();
        for i in 0..n {
            if i > 0 {
                toks.push(OPS[r.below(OPS.len() as u64) as usize].to_string());
            }
            let v = match r.below(10) {
                0 => 0,
                1 => i64::MAX - r.below(3) as i64,
                2 => i64::MIN + r.below(3) as i64,
                _ => r.below(200) as i64 - 100,
            };
            toks.push(v.to_string());
        }
        let want = reference(&toks);
        let got = eval(&toks).ok().map(|v| match v {
            Value::Int(n) => n,
            Value::Str(s) => s.parse().expect("integer result"),
        });
        assert_eq!(got, want, "expr {}", toks.join(" "));
        checked += 1;
    }
    assert_eq!(checked, 20_000);
}

#[test]
fn expr_parentheses_group() {
    let mut r = Rng(42);
    for _ in 0..2_000 {
        let a = r.below(50) as i64;
        let b = r.below(50) as i64;
        let c = r.below(50) as i64 + 1;
        let toks: Vec<String> = ["(", &a.to_string(), "+", &b.to_string(), ")", "*", &c.to_string()]
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(eval(&toks).unwrap().text(), ((a + b) * c).to_string());
    }
}

#[test]
fn pr_pages_are_full_length() {
    let mut r = Rng(7);
    for _ in 0..40 {
        let lines = r.below(150) as usize;
        let len = 11 + r.below(60) as usize;
        let cols = 1 + r.below(3) as usize;
        let double = r.below(2) == 1;
        let script = format!(
            "i=1; while [ $i -le {lines} ]; do echo l$i; i=$((i+1)); done > f; pr -l {len} -{cols} {} f",
            if double { "-d" } else { "" }
        );
        let o = common::run(&script);
        assert_eq!(o.status, 0, "{script}: {}", o.err);
        let out: Vec<&str> = o.out.split_terminator('\n').collect();
        assert_eq!(out.len() % len, 0, "{script}");
        let pages = out.len() / len;
        let body = len - 10;
        let rows = if double { (body / 2).max(1) } else { body };
        assert_eq!(pages, lines.div_ceil(rows * cols), "{script}");
        for p in 0..pages {
            assert!(
                out[p * len + 2].ends_with(&format!("Page {}", p + 1)),
                "{script}: page {p}"
            );
        }
        // Every input line appears once (several to an output row).
        let words: usize = out
            .iter()
            .filter(|l| !l.contains("Page"))
            .map(|l| l.split_whitespace().count())
            .sum();
        assert_eq!(words, lines, "{script}");
    }
}
