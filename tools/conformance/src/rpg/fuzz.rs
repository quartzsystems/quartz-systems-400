// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The grammar fuzzer: random free-form programs that are valid by
//! construction and end (loops are bounded), from a seed. Declarations
//! of every common type, numeric and string expressions with the usual
//! built-ins, IF, SELECT, FOR, MONITOR and a subprocedure; every
//! assignment sits in a MONITOR, so an overflow prints `E` and the
//! program goes on. Each program prints its variables as it goes.

use std::fmt::Write as _;

/// A small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `lo..=hi`.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        let span = u64::try_from(hi - lo + 1).unwrap_or(1);
        lo + i64::try_from(self.next() % span).unwrap_or(0)
    }

    /// True with probability `pct` in a hundred.
    fn chance(&mut self, pct: u64) -> bool {
        self.next() % 100 < pct
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[usize::try_from(self.next() % xs.len() as u64).unwrap_or(0)]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Int,
    Packed(u8, u8),
    Zoned(u8, u8),
    Char(u8),
    VarChar(u8),
    Ind,
}

impl Kind {
    fn numeric(self) -> bool {
        matches!(self, Self::Int | Self::Packed(..) | Self::Zoned(..))
    }

    fn text(self) -> bool {
        matches!(self, Self::Char(_) | Self::VarChar(_))
    }

    fn decl(self) -> String {
        match self {
            Self::Int => "int(10)".into(),
            Self::Packed(d, s) => format!("packed({d}:{s})"),
            Self::Zoned(d, s) => format!("zoned({d}:{s})"),
            Self::Char(n) => format!("char({n})"),
            Self::VarChar(n) => format!("varchar({n})"),
            Self::Ind => "ind".into(),
        }
    }
}

struct Var {
    name: String,
    kind: Kind,
}

/// One program's generator state.
struct Gen {
    rng: Rng,
    vars: Vec<Var>,
    out: Vec<String>,
    depth: usize,
    loops: usize,
    prints: usize,
    proc_name: Option<String>,
}

const WORDS: &[&str] = &["abc", "Quartz", "x", "hello world", "  pad  ", "RPG", "0123", "zz"];

impl Gen {
    fn vars_of(&self, f: impl Fn(Kind) -> bool) -> Vec<usize> {
        (0..self.vars.len()).filter(|&i| f(self.vars[i].kind)).collect()
    }

    fn num_lit(&mut self) -> String {
        let whole = self.rng.range(0, 999);
        let neg = self.rng.chance(20);
        let s = if self.rng.chance(40) {
            format!("{whole}.{:02}", self.rng.range(0, 99))
        } else {
            whole.to_string()
        };
        if neg { format!("-{s}") } else { s }
    }

    fn num_expr(&mut self, depth: usize) -> String {
        let nums = self.vars_of(Kind::numeric);
        let r = self.rng.range(0, 99);
        if depth == 0 || r < 30 {
            if !nums.is_empty() && self.rng.chance(60) {
                let i = *self.rng.pick(&nums);
                return self.vars[i].name.clone();
            }
            return self.num_lit();
        }
        match r {
            30..=64 => {
                let op = *self.rng.pick(&["+", "-", "*", "+", "-"]);
                format!("({} {op} {})", self.num_expr(depth - 1), self.num_expr(depth - 1))
            }
            65..=69 => format!("({} / {})", self.num_expr(depth - 1), self.rng.range(1, 9)),
            70..=74 => format!("%abs({})", self.num_expr(depth - 1)),
            75..=79 => format!("%int({})", self.num_expr(depth - 1)),
            80..=83 => format!("%rem(%int({}) : {})", self.num_expr(depth - 1), self.rng.range(1, 9)),
            84..=87 => {
                let texts = self.vars_of(Kind::text);
                if texts.is_empty() {
                    return self.num_lit();
                }
                let i = *self.rng.pick(&texts);
                let name = self.vars[i].name.clone();
                if self.rng.chance(50) {
                    format!("%len({name})")
                } else {
                    let w = *self.rng.pick(WORDS);
                    format!("%scan('{}' : {name})", &w[..1])
                }
            }
            88..=91 => format!("%dec({} : 15 : 2)", self.num_expr(depth - 1)),
            _ => match self.proc_name.clone() {
                Some(p) => format!("{p}({})", self.num_expr(depth - 1)),
                None => self.num_lit(),
            },
        }
    }

    fn str_expr(&mut self, depth: usize) -> String {
        let texts = self.vars_of(Kind::text);
        let r = self.rng.range(0, 99);
        if depth == 0 || r < 30 {
            if !texts.is_empty() && self.rng.chance(60) {
                let i = *self.rng.pick(&texts);
                return self.vars[i].name.clone();
            }
            return format!("'{}'", self.rng.pick(WORDS));
        }
        match r {
            30..=54 => format!("{} + {}", self.str_expr(depth - 1), self.str_expr(depth - 1)),
            55..=64 => format!("%trim({})", self.str_expr(depth - 1)),
            65..=69 => format!("%triml({})", self.str_expr(depth - 1)),
            70..=74 => format!("%trimr({})", self.str_expr(depth - 1)),
            75..=84 => {
                let fixed = self.vars_of(|k| matches!(k, Kind::Char(_)));
                if fixed.is_empty() {
                    return format!("'{}'", self.rng.pick(WORDS));
                }
                let i = *self.rng.pick(&fixed);
                let Kind::Char(n) = self.vars[i].kind else { unreachable!() };
                let start = self.rng.range(1, i64::from(n));
                let len = self.rng.range(1, i64::from(n) - start + 1);
                format!("%subst({} : {start} : {len})", self.vars[i].name)
            }
            _ => format!("%char({})", self.num_expr(depth - 1)),
        }
    }

    fn cond(&mut self, depth: usize) -> String {
        let r = self.rng.range(0, 99);
        let inds = self.vars_of(|k| k == Kind::Ind);
        if depth > 0 && r < 15 {
            let op = *self.rng.pick(&["and", "or"]);
            return format!("({} {op} {})", self.cond(depth - 1), self.cond(depth - 1));
        }
        if depth > 0 && r < 22 {
            return format!("not ({})", self.cond(depth - 1));
        }
        if r < 32 && !inds.is_empty() {
            let i = *self.rng.pick(&inds);
            return self.vars[i].name.clone();
        }
        let rel = *self.rng.pick(&["=", "<>", "<", ">", "<=", ">="]);
        if r < 50 {
            return format!("{} {rel} {}", self.str_expr(1), self.str_expr(1));
        }
        format!("{} {rel} {}", self.num_expr(2), self.num_expr(2))
    }

    fn line(&mut self, s: impl Into<String>) {
        let pad = "  ".repeat(self.depth);
        for piece in wrap(&s.into(), 90 - pad.len()) {
            self.out.push(format!("{pad}{piece}"));
        }
    }

    fn print_var(&mut self, i: usize) {
        self.prints += 1;
        let v = &self.vars[i];
        let shown = match v.kind {
            Kind::Ind => v.name.clone(),
            k if k.numeric() => format!("%char({})", v.name),
            _ => format!("'[' + {} + ']'", v.name),
        };
        let text = format!("line = 'P{:03} {} ' + {shown};", self.prints, v.name);
        self.line(text);
        self.line("write QSYSPRT line;");
    }

    fn assign(&mut self) {
        let i = self.rng.range(0, i64::try_from(self.vars.len()).unwrap_or(1) - 1);
        let i = usize::try_from(i).unwrap_or(0);
        let name = self.vars[i].name.clone();
        let kind = self.vars[i].kind;
        let rhs = match kind {
            Kind::Ind => self.cond(1),
            k if k.numeric() => self.num_expr(3),
            _ => self.str_expr(2),
        };
        let op = if kind.numeric() && self.rng.chance(25) { "eval(h) " } else { "" };
        self.line("monitor;");
        self.depth += 1;
        self.line(format!("{op}{name} = {rhs};"));
        self.depth -= 1;
        self.line("on-error;");
        self.depth += 1;
        self.prints += 1;
        let t = format!("line = 'P{:03} E ' + %char(%status());", self.prints);
        self.line(t);
        self.line("write QSYSPRT line;");
        self.depth -= 1;
        self.line("endmon;");
        if self.rng.chance(50) {
            self.print_var(i);
        }
    }

    fn stmt(&mut self, budget: &mut usize) {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let r = self.rng.range(0, 99);
        if self.depth >= 3 || r < 55 {
            self.assign();
            return;
        }
        match r {
            55..=69 => {
                let c = self.cond(2);
                self.line(format!("if {c};"));
                self.block(budget);
                if self.rng.chance(50) {
                    let c = self.cond(1);
                    self.line(format!("elseif {c};"));
                    self.block(budget);
                }
                if self.rng.chance(50) {
                    self.line("else;");
                    self.block(budget);
                }
                self.line("endif;");
            }
            70..=79 => {
                self.line("select;");
                for _ in 0..self.rng.range(1, 3) {
                    let c = self.cond(1);
                    self.line(format!("when {c};"));
                    self.block(budget);
                }
                if self.rng.chance(50) {
                    self.line("other;");
                    self.block(budget);
                }
                self.line("endsl;");
            }
            _ => {
                if self.loops >= 3 {
                    self.assign();
                    return;
                }
                let counter = format!("I{}", self.loops);
                self.loops += 1;
                let n = self.rng.range(1, 4);
                let by = if self.rng.chance(30) { " by 2" } else { "" };
                self.line(format!("for {counter} = 1{by} to {n};"));
                self.block(budget);
                self.line("endfor;");
                self.loops -= 1;
            }
        }
    }

    fn block(&mut self, budget: &mut usize) {
        self.depth += 1;
        for _ in 0..self.rng.range(1, 3) {
            self.stmt(budget);
        }
        self.depth -= 1;
    }
}

/// Cut a statement at blanks outside literals into pieces of at most
/// `width` characters.
fn wrap(s: &str, width: usize) -> Vec<String> {
    if s.len() <= width {
        return vec![s.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut word = String::new();
    let mut quoted = false;
    for c in s.chars() {
        if c == '\'' {
            quoted = !quoted;
        }
        word.push(c);
        if c == ' ' && !quoted {
            if cur.len() + word.len() > width && !cur.is_empty() {
                out.push(cur.trim_end().to_string());
                cur.clear();
            }
            cur.push_str(&word);
            word.clear();
        }
    }
    if cur.len() + word.len() > width && !cur.is_empty() {
        out.push(cur.trim_end().to_string());
        cur.clear();
    }
    cur.push_str(&word);
    out.push(cur);
    out
}

/// The program for a seed.
#[must_use]
pub fn program(seed: u64) -> String {
    let mut g = Gen {
        rng: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
        vars: Vec::new(),
        out: Vec::new(),
        depth: 0,
        loops: 0,
        prints: 0,
        proc_name: None,
    };
    let mut head = vec![
        "**FREE".to_string(),
        "ctl-opt dftactgrp(*no);".to_string(),
        "dcl-f QSYSPRT printer(132);".to_string(),
        "dcl-ds line len(132) end-ds;".to_string(),
    ];
    for i in 0..3 {
        head.push(format!("dcl-s I{i} int(10);"));
    }
    let n = g.rng.range(4, 9);
    for i in 0..n {
        let kind = match g.rng.range(0, 9) {
            0 | 1 => Kind::Int,
            2 | 3 => {
                let d = u8::try_from(g.rng.range(1, 15)).unwrap_or(5);
                Kind::Packed(d, u8::try_from(g.rng.range(0, i64::from(d.min(4)))).unwrap_or(0))
            }
            4 => {
                let d = u8::try_from(g.rng.range(1, 11)).unwrap_or(5);
                Kind::Zoned(d, u8::try_from(g.rng.range(0, i64::from(d.min(3)))).unwrap_or(0))
            }
            5 | 6 => Kind::Char(u8::try_from(g.rng.range(1, 20)).unwrap_or(5)),
            7 | 8 => Kind::VarChar(u8::try_from(g.rng.range(1, 30)).unwrap_or(5)),
            _ => Kind::Ind,
        };
        let name = format!("V{i}");
        let inz = if g.rng.chance(50) {
            match kind {
                Kind::Int => format!(" inz({})", g.rng.range(-50, 50)),
                Kind::Ind => " inz(*on)".to_string(),
                k if k.text() => {
                    let w = *g.rng.pick(WORDS);
                    let max = match k {
                        Kind::Char(n) | Kind::VarChar(n) => usize::from(n),
                        _ => 1,
                    };
                    format!(" inz('{}')", &w[..w.len().min(max)])
                }
                _ => String::new(),
            }
        } else {
            String::new()
        };
        head.push(format!("dcl-s {name} {}{inz};", kind.decl()));
        g.vars.push(Var { name, kind });
    }
    let has_proc = g.rng.chance(40);
    if has_proc {
        g.proc_name = Some("FN".into());
    }
    let mut budget = usize::try_from(g.rng.range(8, 30)).unwrap_or(10);
    while budget > 0 {
        g.stmt(&mut budget);
    }
    for i in 0..g.vars.len() {
        g.print_var(i);
    }
    let mut lines = head;
    lines.append(&mut g.out);
    lines.push("*inlr = *on;".into());
    if has_proc {
        let k = g.rng.range(2, 9);
        let m = g.rng.range(0, 99);
        lines.extend([
            "dcl-proc FN;".to_string(),
            "  dcl-pi *n packed(15:2);".to_string(),
            "    a packed(15:2) value;".to_string(),
            "  end-pi;".to_string(),
            format!("  return a * {k} + {m};"),
            "end-proc;".to_string(),
        ]);
    }
    let mut text = String::new();
    for l in lines {
        let _ = writeln!(text, "{l}");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_are_deterministic_and_fit_the_record() {
        assert_eq!(program(7), program(7));
        assert_ne!(program(7), program(8));
        for s in 0..200 {
            for l in program(s).lines() {
                assert!(l.len() <= 100, "seed {s}: {l}");
            }
        }
    }

    #[test]
    fn long_statements_wrap_outside_literals() {
        let w = wrap("x = 'a b c d e f' + y + z + w;", 12);
        assert!(w.iter().all(|p| !p.is_empty()));
        assert_eq!(w.concat().replace(' ', ""), "x='abcdef'+y+z+w;");
    }
}
