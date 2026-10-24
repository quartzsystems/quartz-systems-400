// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The structure of one procedure's calculations, checked as IBM
//! checks it before code generation: each group (`IF`, the loops,
//! `SELECT`, `MONITOR`, `CASxx`) ended by its own `ENDyy` or `END`,
//! the operations that belong inside one, and the subroutines.

use rpgfront::Diag;
use rpgfront::ast::{Expr, Pos, Stmt};
use rpgfront::parse::ops::split_suffix;

use crate::diag::ids;

/// An open group.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    If,
    Loop,
    Select { other: bool },
    Monitor { handled: bool },
    Cas,
}

/// Operations whose factor 2 must be given.
const NEED_F2: &[&str] = &["EXSR", "IF", "DOW", "DOU", "WHEN", "CAB", "CAS", "AND", "OR", "COMP", "ADD", "SUB", "MULT", "DIV", "Z-ADD", "Z-SUB", "MOVE", "MOVEL", "MOVEA", "CAT", "SCAN", "CHECK", "CHECKR", "XLATE", "SUBST", "LOOKUP", "XFOOT", "SQRT", "OCCUR"];

/// Operations whose result field must be given.
const NEED_RESULT: &[&str] = &["ADD", "SUB", "MULT", "DIV", "Z-ADD", "Z-SUB", "MOVE", "MOVEL", "MOVEA", "CAT", "XLATE", "SUBST", "XFOOT", "SQRT", "MVR"];

/// The return points `ENDSR` may name.
const RETURN_POINTS: &[&str] = &["*DETL", "*GETIN", "*TOTC", "*TOTL", "*OFL", "*DETC", "*CANCL"];

/// The diagnostics for a list of calculations.
#[must_use]
pub fn check(stmts: &[Stmt]) -> Vec<Diag> {
    let mut out = Vec::new();
    let mut open: Vec<(Group, Pos)> = Vec::new();
    let mut subr: Option<Pos> = None;
    let mut names: Vec<String> = Vec::new();
    let defined: Vec<String> = stmts.iter().filter(|s| s.op.code == "BEGSR").filter_map(|s| s.op.f1.first().and_then(name_of)).collect();
    for s in stmts {
        let at = s.at;
        let (base, sfx) = split_suffix(&s.op.code);
        let op = &s.op;
        // The entries the operation needs. A fixed-form compare takes
        // the suffix; a free-form condition its expression.
        let fixed_cmp = !sfx.is_empty() || !matches!(base, "IF" | "DOW" | "DOU" | "WHEN");
        if NEED_F2.contains(&base) && fixed_cmp && op.f2.is_empty() && op.xf2.is_none() {
            out.push(Diag::new(ids::NO_FACTOR2, 30, at));
        }
        if matches!(base, "IF" | "DOW" | "DOU" | "WHEN" | "ELSEIF") && sfx.is_empty() && op.xf2.is_none() {
            out.push(Diag::new(ids::NO_EXPRESSION, 30, at));
        }
        if NEED_RESULT.contains(&base) && op.result.is_empty() && !op.f2.is_empty() {
            out.push(Diag::new(ids::NO_RESULT, 30, at));
        }
        // A literal where a field is needed.
        if matches!(base, "TEST" | "DEALLOC") && matches!(op.result.first(), Some(Expr::Lit(..))) {
            out.push(Diag::new(ids::BAD_RESULT, 30, at));
        }
        // TEST of a date or time reports through (E) or an indicator.
        if base == "TEST" && op.ext.contains(['D', 'T', 'Z']) && !op.ext.contains('E') && op.resind[1].is_empty() {
            out.push(Diag::new(ids::TEST_NO_ERROR, 30, at));
        }
        if base == "ENDSR"
            && let Some(Expr::Lit(rpgfront::ast::Lit::Char(p), _)) = op.f2.first()
            && !RETURN_POINTS.contains(&p.to_ascii_uppercase().as_str())
        {
            out.push(Diag::new(ids::ENDSR_POINT, 20, at));
        }
        if base == "ON-ERROR" {
            let items: Vec<&Expr> = match &op.xf2 {
                Some(Expr::List(v, _)) => v.iter().collect(),
                Some(e) => vec![e],
                None => Vec::new(),
            };
            let big = |e: &&Expr| matches!(e, Expr::Lit(rpgfront::ast::Lit::Num(n), _) if n.parse::<u64>().is_ok_and(|c| c > 99_999));
            if items.iter().any(big) {
                out.push(Diag::new(ids::ON_ERROR_STATUS, 20, at));
            }
        }
        // A subroutine called is defined in the same procedure.
        let called = match base {
            "EXSR" => s.op.f2.first(),
            "CAS" => s.op.result.first(),
            _ => None,
        };
        if let Some(n) = called.and_then(name_of)
            && !defined.contains(&n)
        {
            out.push(Diag::new(ids::NOT_DEFINED, 30, at).arg(n.clone()));
            let id = if base == "CAS" { ids::CAS_NOT_SUBROUTINE } else { ids::NOT_A_SUBROUTINE };
            out.push(Diag::new(id, 30, at).arg(n));
        }
        match base {
            "IF" => open.push((Group::If, at)),
            "DOW" | "DOU" | "DO" | "FOR" | "FOR-EACH" => open.push((Group::Loop, at)),
            "SELECT" => open.push((Group::Select { other: false }, at)),
            "MONITOR" => open.push((Group::Monitor { handled: false }, at)),
            "CAS" => {
                if !matches!(open.last(), Some((Group::Cas, _))) {
                    open.push((Group::Cas, at));
                }
            }
            "ELSE" | "ELSEIF" => {
                if !matches!(open.last(), Some((Group::If, _))) {
                    out.push(Diag::new(ids::END_UNMATCHED, 30, at));
                }
            }
            "WHEN" | "WHEN-IS" | "WHEN-IN" => {
                if !matches!(open.last(), Some((Group::Select { other: false }, _))) {
                    out.push(Diag::new(ids::WHEN_OUTSIDE, 20, at));
                }
            }
            "OTHER" => match open.last_mut() {
                Some((Group::Select { other }, _)) if !*other => *other = true,
                _ => out.push(Diag::new(ids::OTHER_OUTSIDE, 20, at)),
            },
            "ON-ERROR" | "ON-EXCP" => match open.last_mut() {
                Some((Group::Monitor { handled }, _)) => *handled = true,
                _ => out.push(Diag::new(ids::ON_ERROR_OUTSIDE, 30, at)),
            },
            "LEAVE" | "ITER" => {
                if !open.iter().any(|(g, _)| *g == Group::Loop) {
                    out.push(Diag::new(ids::NOT_IN_LOOP, 30, at));
                }
            }
            "ENDIF" | "ENDDO" | "ENDFOR" | "ENDSL" | "ENDMON" | "ENDCS" | "END" => {
                let fits = |g: Group| match base {
                    "ENDIF" => g == Group::If,
                    "ENDDO" | "ENDFOR" => g == Group::Loop,
                    "ENDSL" => matches!(g, Group::Select { .. }),
                    "ENDMON" => matches!(g, Group::Monitor { .. }),
                    "ENDCS" => g == Group::Cas,
                    _ => !matches!(g, Group::Monitor { .. }),
                };
                if open.last().is_some_and(|(g, _)| fits(*g)) {
                    // A MONITOR group needs a handler.
                    if let Some((Group::Monitor { handled: false }, p)) = open.pop() {
                        out.push(Diag::new(ids::NO_ON_ERROR, 20, p));
                    }
                } else if open.pop().is_some() {
                    // The wrong ENDyy for the open group ends it.
                    out.push(Diag::new(ids::END_OUT_OF_SEQUENCE, 20, at));
                } else {
                    out.push(Diag::new(ids::END_UNMATCHED, 30, at));
                }
            }
            "BEGSR" => {
                let name = s.op.f1.first().and_then(name_of);
                // A subroutine still open is ended here.
                if let Some(p) = subr.take() {
                    out.push(Diag::new(ids::ENDSR_MISSING, 30, p));
                }
                if let Some(n) = name {
                    if names.contains(&n) {
                        out.push(Diag::new(ids::SR_TWICE, 20, at).arg(n));
                    } else {
                        names.push(n);
                    }
                }
                open_groups_end(&mut open, &mut out);
                subr = Some(at);
            }
            "ENDSR" => {
                open_groups_end(&mut open, &mut out);
                if subr.take().is_none() {
                    out.push(Diag::new(ids::ENDSR_UNMATCHED, 30, at));
                }
            }
            _ => {}
        }
    }
    open_groups_end(&mut open, &mut out);
    out
}

/// A subroutine name.
fn name_of(e: &Expr) -> Option<String> {
    match e {
        Expr::Ref { name, .. } => Some(name.clone()),
        Expr::Special { word, .. } => Some(word.clone()),
        _ => None,
    }
}

/// Groups left open: each one's `END` is assumed.
fn open_groups_end(open: &mut Vec<(Group, Pos)>, out: &mut Vec<Diag>) {
    for (g, at) in open.drain(..).rev() {
        // A CASxx group without ENDCS: IBM inserts the END.
        if g == Group::Cas {
            out.push(Diag::new(ids::ENDCS_MISSING, 20, at));
        } else {
            out.push(Diag::new(ids::END_MISSING, 30, at));
        }
    }
}
