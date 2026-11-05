// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The expressions directly under an expression, for walks that do not
//! enter subqueries.

use super::expr::Expr;

/// Every expression directly under `e`, subqueries left out.
#[must_use]
pub fn children(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Neg(a) | Expr::Not(a) | Expr::IsNull(a, _) | Expr::Cast(a, _) | Expr::Duration(a, _) | Expr::InSubquery(a, _, _) => {
            vec![a]
        }
        Expr::Binary(a, _, b) | Expr::And(a, b) | Expr::Or(a, b) | Expr::Compare(a, _, b, _) => vec![a, b],
        Expr::Between(a, b, c, _) => vec![a, b, c],
        Expr::InList(a, l, _) => std::iter::once(&**a).chain(l.iter()).collect(),
        Expr::Like(a, p, esc, _) => {
            let mut v: Vec<&Expr> = vec![a, p];
            if let Some(x) = esc {
                v.push(x);
            }
            v
        }
        Expr::Case { operand, whens, otherwise } => {
            let mut v: Vec<&Expr> = Vec::new();
            if let Some(o) = operand {
                v.push(o);
            }
            for (w, t) in whens {
                v.push(w);
                v.push(t);
            }
            if let Some(o) = otherwise {
                v.push(o);
            }
            v
        }
        Expr::Function(f) => f.args.iter().collect(),
        Expr::Row(items) => items.iter().collect(),
        Expr::WithinGroup(w) => w.func.args.iter().chain(w.order.iter().map(|o| &o.expr)).collect(),
        Expr::Over(o) => {
            let mut v = vec![&o.func];
            v.extend(o.window.partition.iter());
            v.extend(o.window.order.iter().map(|i| &i.expr));
            v
        }
        _ => Vec::new(),
    }
}

/// [`children`], mutable.
pub fn children_mut(e: &mut Expr) -> Vec<&mut Expr> {
    match e {
        Expr::Neg(a) | Expr::Not(a) | Expr::IsNull(a, _) | Expr::Cast(a, _) | Expr::Duration(a, _) | Expr::InSubquery(a, _, _) => {
            vec![a]
        }
        Expr::Binary(a, _, b) | Expr::And(a, b) | Expr::Or(a, b) | Expr::Compare(a, _, b, _) => vec![a, b],
        Expr::Between(a, b, c, _) => vec![a, b, c],
        Expr::InList(a, l, _) => std::iter::once(&mut **a).chain(l.iter_mut()).collect(),
        Expr::Like(a, p, esc, _) => {
            let mut v: Vec<&mut Expr> = vec![a, p];
            if let Some(x) = esc {
                v.push(x);
            }
            v
        }
        Expr::Case { operand, whens, otherwise } => {
            let mut v: Vec<&mut Expr> = Vec::new();
            if let Some(o) = operand {
                v.push(o);
            }
            for (w, t) in whens {
                v.push(w);
                v.push(t);
            }
            if let Some(o) = otherwise {
                v.push(o);
            }
            v
        }
        Expr::Function(f) => f.args.iter_mut().collect(),
        Expr::Row(items) => items.iter_mut().collect(),
        Expr::WithinGroup(w) => {
            let w = &mut **w;
            w.func.args.iter_mut().chain(w.order.iter_mut().map(|o| &mut o.expr)).collect()
        }
        Expr::Over(o) => {
            let o = &mut **o;
            let mut v = vec![&mut o.func];
            v.extend(o.window.partition.iter_mut());
            v.extend(o.window.order.iter_mut().map(|i| &mut i.expr));
            v
        }
        _ => Vec::new(),
    }
}
