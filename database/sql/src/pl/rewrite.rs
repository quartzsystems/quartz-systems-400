// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A statement of a routine as its program runs it: the routine's
//! variables and parameters become `?` markers, in order, and the
//! program passes their values. A name is a column when a table of the
//! statement has it, else a variable when one is in scope (IBM's rule:
//! columns first).

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::error::Result;
use crate::lex::{Tok, tokenize};

/// What the rewriting asks of the routine's scopes.
pub trait Names {
    /// The variable `name`, as the routine numbers its variables.
    fn variable(&self, name: &str) -> Option<usize>;
    /// The variable `name` of the compound or loop labelled `label`
    /// (or of the routine named `label`).
    fn qualified(&self, label: &str, name: &str) -> Option<usize>;
    /// A scalar variable's type as SQL writes it: `CHAR(v)` takes it, so
    /// the result has the type's width.
    fn type_of(&self, k: usize) -> Option<String> {
        let _ = k;
        None
    }
    /// An array variable's element type: `v[i]` is cast to it.
    fn element_type(&self, k: usize) -> Option<String> {
        let _ = k;
        None
    }
}

/// The statement text with markers, and the variables in marker order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rewritten {
    /// The text.
    pub text: String,
    /// The variables, one per marker added.
    pub vars: Vec<usize>,
}

/// A marker, as the previous token of the next.
static PARAM: Tok = Tok::Param;

/// One token as SQL text.
pub fn render(t: &Tok, out: &mut String) {
    match t {
        Tok::Word(w) => out.push_str(w),
        Tok::Delimited(d) => {
            let _ = write!(out, "\"{}\"", d.replace('"', "\"\""));
        }
        Tok::Str(s) => {
            let _ = write!(out, "'{}'", s.replace('\'', "''"));
        }
        Tok::Hex(h) | Tok::BinHex(h) => {
            if matches!(t, Tok::BinHex(_)) {
                out.push('B');
            }
            out.push_str("X'");
            for b in h {
                let _ = write!(out, "{b:02X}");
            }
            out.push('\'');
        }
        Tok::Num(n) => out.push_str(n),
        Tok::Param => out.push('?'),
        Tok::Host { name, ind } => {
            let _ = write!(out, ":{name}");
            if let Some(i) = ind {
                let _ = write!(out, ":{i}");
            }
        }
        Tok::Sym(s) => out.push_str(s),
        Tok::End => {}
    }
}

/// Rewrite `text`: words naming a variable (and not a column of
/// `columns`) become markers. Markers already there are an error the
/// caller reports.
///
/// # Errors
///
/// The text does not tokenize.
pub fn rewrite<S: std::hash::BuildHasher>(text: &str, names: &dyn Names, columns: &HashSet<String, S>) -> Result<Rewritten> {
    let toks: Vec<Tok> = tokenize(text)?.into_iter().map(|t| t.tok).collect();
    let mut out = Rewritten::default();
    let mut i = 0;
    let mut prev: Option<&Tok> = None;
    let mut before: Option<&Tok> = None;
    // `v[i]` of an array: the bracket depth of each open CAST, its type.
    let mut depth = 0usize;
    let mut casts: Vec<(usize, String)> = Vec::new();
    while i < toks.len() {
        let t = &toks[i];
        if matches!(t, Tok::End) {
            break;
        }
        let next = toks.get(i + 1);
        let after_dot = matches!(prev, Some(Tok::Sym(".")));
        let after_as = matches!(prev, Some(Tok::Word(w)) if w == "AS");
        let mut replaced = None;
        if let Tok::Word(w) | Tok::Delimited(w) = t
            && !after_dot
            && !after_as
        {
            if matches!(next, Some(Tok::Sym("."))) {
                // `label.var`: the qualifier is a label of the routine.
                if let Some(Tok::Word(v) | Tok::Delimited(v)) = toks.get(i + 2)
                    && let Some(k) = names.qualified(w, v)
                {
                    replaced = Some((k, 3));
                }
            } else if !matches!(next, Some(Tok::Sym("("))) && !columns.contains(w.as_str())
                && let Some(k) = names.variable(w)
            {
                replaced = Some((k, 1));
            }
        }
        if !out.text.is_empty() && needs_space(prev, t) {
            out.text.push(' ');
        }
        if let Some((k, n)) = replaced {
            let after = toks.get(i + n);
            let char_arg = matches!(prev, Some(Tok::Sym("("))) && matches!(before, Some(Tok::Word(w)) if w == "CHAR") && matches!(after, Some(Tok::Sym(")")));
            if matches!(after, Some(Tok::Sym("["))) && let Some(t) = names.element_type(k) {
                out.text.push_str("CAST(?");
                casts.push((depth, t));
            } else if char_arg && let Some(t) = names.type_of(k) {
                let _ = write!(out.text, "CAST(? AS {t})");
            } else {
                out.text.push('?');
            }
            out.vars.push(k);
            before = prev;
            prev = Some(&PARAM);
            i += n;
        } else {
            render(t, &mut out.text);
            match t {
                Tok::Sym("[") => depth += 1,
                Tok::Sym("]") => {
                    depth = depth.saturating_sub(1);
                    if casts.last().is_some_and(|(d, _)| *d == depth) && !matches!(toks.get(i + 1), Some(Tok::Sym("["))) {
                        let (_, ty) = casts.pop().expect("an open CAST");
                        let _ = write!(out.text, " AS {ty})");
                    }
                }
                _ => {}
            }
            before = prev;
            prev = Some(t);
            i += 1;
        }
    }
    Ok(out)
}

/// Whether two tokens need a blank between them.
fn needs_space(prev: Option<&Tok>, t: &Tok) -> bool {
    let call = matches!(t, Tok::Sym("(")) && matches!(prev, Some(Tok::Word(_)));
    !call && !matches!(prev, Some(Tok::Sym("." | "("))) && !matches!(t, Tok::Sym("." | ")" | ","))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Vars;
    impl Names for Vars {
        fn variable(&self, name: &str) -> Option<usize> {
            ["V_ID", "TOTAL", "NAME"].iter().position(|v| *v == name)
        }
        fn qualified(&self, label: &str, name: &str) -> Option<usize> {
            (label == "P1" && name == "NAME").then_some(2)
        }
    }

    #[test]
    fn variables_become_markers() {
        let cols: HashSet<String> = ["NAME".to_string(), "ID".to_string()].into();
        let r = rewrite("UPDATE T SET NAME = P1.NAME, AMT = TOTAL + 1 WHERE ID = v_id AND t.x = 'v_id'", &Vars, &cols).unwrap();
        assert_eq!(r.text, "UPDATE T SET NAME = ?, AMT = ? + 1 WHERE ID = ? AND T.X = 'v_id'");
        assert_eq!(r.vars, vec![2, 1, 0]);
        let r = rewrite("SELECT COUNT(*) AS TOTAL FROM T", &Vars, &HashSet::new()).unwrap();
        assert_eq!(r.text, "SELECT COUNT(*) AS TOTAL FROM T");
    }
}
