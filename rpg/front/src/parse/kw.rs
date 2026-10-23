// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Keyword areas: `NAME` or `NAME(arg:arg...)`, separated by blanks.
//! Each argument keeps its tokens and, when they form one, its
//! expression; arguments such as `*ISO-` are read by the analysis from
//! the tokens.

use crate::ast::{Arg, Keyword, Pos};
use crate::diag::{Diag, ids};
use crate::lex::{Kind, Tok};
use crate::parse::expr;

/// Parse a keyword area.
pub fn keywords(toks: &[Tok], end: Pos, diags: &mut Vec<Diag>) -> Vec<Keyword> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        let name = match &t.kind {
            Kind::Name(n) => n.clone(),
            Kind::Special(s) => s.clone(),
            _ => {
                diags.push(Diag::new(ids::BAD_KEYWORD, 30, t.at).arg(t.text()));
                i += 1;
                continue;
            }
        };
        let at = t.at;
        i += 1;
        let mut kw = Keyword {
            name,
            args: Vec::new(),
            parens: false,
            at,
        };
        if toks.get(i).is_some_and(|t| t.is("(")) {
            kw.parens = true;
            let start = i + 1;
            let mut depth = 1;
            let mut j = start;
            while j < toks.len() && depth > 0 {
                if toks[j].is("(") {
                    depth += 1;
                } else if toks[j].is(")") {
                    depth -= 1;
                }
                j += 1;
            }
            if depth != 0 {
                diags.push(Diag::new(ids::PAREN, 30, at));
            }
            let inner_end = if depth == 0 { j - 1 } else { j };
            let inner = &toks[start..inner_end];
            if !inner.is_empty() {
                for part in expr::split_colons(inner) {
                    let mut scratch = Vec::new();
                    let e = if part.is_empty() {
                        None
                    } else {
                        expr::expr_all(part, end, &mut scratch).filter(|_| scratch.is_empty())
                    };
                    kw.args.push(Arg {
                        toks: part.to_vec(),
                        expr: e,
                    });
                }
            }
            i = j;
        }
        out.push(kw);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::lex_text;

    #[test]
    fn keywords_and_arguments() {
        let mut d = Vec::new();
        let toks = lex_text(
            "DIM(10) INZ(*BLANKS) DATFMT(*ISO-) OVERLAY(DS:*NEXT) CONST EXTNAME('F':*ALL)",
            Pos { line: 1, col: 44 },
            &mut d,
        );
        let k = keywords(&toks, Pos::default(), &mut d);
        assert!(d.is_empty(), "{d:?}");
        let names: Vec<&str> = k.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, vec!["DIM", "INZ", "DATFMT", "OVERLAY", "CONST", "EXTNAME"]);
        assert_eq!(k[2].args[0].toks.len(), 2);
        assert!(k[2].args[0].expr.is_none());
        assert_eq!(k[3].args.len(), 2);
        assert!(!k[4].parens);
        assert_eq!(k[5].args.len(), 2);
    }
}
