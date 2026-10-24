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
    Monitor,
    Cas,
}

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
        let (base, _) = split_suffix(&s.op.code);
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
            out.push(Diag::new(ids::NOT_A_SUBROUTINE, 30, at).arg(n));
        }
        match base {
            "IF" => open.push((Group::If, at)),
            "DOW" | "DOU" | "DO" | "FOR" | "FOR-EACH" => open.push((Group::Loop, at)),
            "SELECT" => open.push((Group::Select { other: false }, at)),
            "MONITOR" => open.push((Group::Monitor, at)),
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
                Some((Group::Select { other }, _)) => *other = true,
                _ => out.push(Diag::new(ids::WHEN_OUTSIDE, 20, at)),
            },
            "ON-ERROR" | "ON-EXCP" => {
                if !matches!(open.last(), Some((Group::Monitor, _))) {
                    out.push(Diag::new(ids::ON_ERROR_OUTSIDE, 30, at));
                }
            }
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
                    "ENDMON" => g == Group::Monitor,
                    "ENDCS" => g == Group::Cas,
                    _ => g != Group::Monitor,
                };
                if open.last().is_some_and(|(g, _)| fits(*g)) {
                    open.pop();
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
    for (_, at) in open.drain(..).rev() {
        out.push(Diag::new(ids::END_MISSING, 30, at));
    }
}
