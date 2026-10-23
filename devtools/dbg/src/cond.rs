// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A conditional breakpoint's `WHEN`: `variable op value`, the
//! variable evaluated at the stop and compared as a number when both
//! sides read as numbers, else as text.

use std::cmp::Ordering;

/// A comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `=`, `==`.
    Eq,
    /// `<>`, `!=`.
    Ne,
    /// `<`.
    Lt,
    /// `<=`.
    Le,
    /// `>`.
    Gt,
    /// `>=`.
    Ge,
}

/// A parsed condition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cond {
    /// The variable.
    pub var: String,
    /// The comparison.
    pub op: Op,
    /// The value compared with, quotes removed.
    pub value: String,
}

/// Read `var op value`; `None` when it is not one.
#[must_use]
pub fn parse(text: &str) -> Option<Cond> {
    const OPS: &[(&str, Op)] = &[
        ("<>", Op::Ne),
        ("!=", Op::Ne),
        ("<=", Op::Le),
        (">=", Op::Ge),
        ("==", Op::Eq),
        ("=", Op::Eq),
        ("<", Op::Lt),
        (">", Op::Gt),
    ];
    for (s, op) in OPS {
        if let Some((l, r)) = text.split_once(s) {
            let var = l.trim();
            let value = unquote(r.trim());
            if var.is_empty() || r.trim().is_empty() {
                return None;
            }
            return Some(Cond {
                var: var.to_string(),
                op: *op,
                value,
            });
        }
    }
    None
}

fn unquote(s: &str) -> String {
    s.strip_prefix('\'')
        .and_then(|t| t.strip_suffix('\''))
        .map_or_else(|| s.to_string(), |t| t.replace("''", "'"))
}

impl Cond {
    /// Whether the condition holds for the variable's value as `EVAL`
    /// shows it.
    #[must_use]
    pub fn holds(&self, shown: &str) -> bool {
        let lhs = unquote(shown.trim());
        let ord = match (lhs.trim().parse::<f64>(), self.value.trim().parse::<f64>()) {
            (Ok(a), Ok(b)) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
            _ => lhs.trim_end().cmp(self.value.trim_end()),
        };
        match self.op {
            Op::Eq => ord == Ordering::Equal,
            Op::Ne => ord != Ordering::Equal,
            Op::Lt => ord == Ordering::Less,
            Op::Le => ord != Ordering::Greater,
            Op::Gt => ord == Ordering::Greater,
            Op::Ge => ord != Ordering::Less,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditions() {
        let c = parse("&COUNT >= 10").unwrap();
        assert_eq!(c.var, "&COUNT");
        assert_eq!(c.op, Op::Ge);
        assert!(c.holds("10"));
        assert!(c.holds("12.00"));
        assert!(!c.holds("9"));
        let c = parse("&NAME = 'ABC'").unwrap();
        assert!(c.holds("'ABC       '"));
        assert!(!c.holds("'ABD'"));
        assert!(parse("&X <> 3").unwrap().holds("4"));
        assert!(parse("nothing").is_none());
        assert!(parse("= 3").is_none());
    }
}
