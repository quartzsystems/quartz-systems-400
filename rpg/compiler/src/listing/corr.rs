// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The EVAL-CORR summary after the cross-reference: for each pair of
//! structures an `EVAL-CORR` assigns, its statements, then each target
//! subfield and whether it is assigned, then the subfields only one
//! structure has (`RNF7341` target only, `RNF7342` source only).

use rpgfront::ast::{Expr, Step};

use super::Out;
use super::xref::slot;
use crate::compile::Compiled;
use crate::types::Ty;

/// One summary: the target and source structures and the statements.
struct Summary {
    target: usize,
    source: usize,
    lines: Vec<u32>,
}

/// The structure an operand names, if any.
fn ds_of(c: &Compiled, scope: usize, e: &Expr) -> Option<usize> {
    let Expr::Ref { name, steps, .. } = e else { return None };
    let id = c.table.find(scope, name)?;
    let mut ty = c.table.syms[id].ty.clone();
    for s in steps {
        let Step::Field(f) = s else { continue };
        let Ty::Ds(d) = ty else { return None };
        ty = c.table.syms[c.table.subfield(d, &f.to_ascii_uppercase())?].ty.clone();
    }
    match ty {
        Ty::Ds(d) => Some(d),
        _ => None,
    }
}

fn summaries(c: &Compiled) -> Vec<Summary> {
    let mut out: Vec<Summary> = Vec::new();
    let scoped = c.module.calcs.iter().map(|s| (0, s)).chain(c.module.procs.iter().enumerate().flat_map(|(k, p)| p.calcs.iter().map(move |s| (k + 1, s))));
    for (scope, s) in scoped {
        if s.op.code != "EVAL-CORR" {
            continue;
        }
        let Some(Expr::Assign(_, t, v, _)) = &s.op.xf2 else { continue };
        let (Some(target), Some(source)) = (ds_of(c, scope, t), ds_of(c, scope, v)) else { continue };
        match out.iter_mut().find(|x| x.target == target && x.source == source) {
            Some(x) => x.lines.push(s.at.line),
            None => out.push(Summary { target, source, lines: vec![s.at.line] }),
        }
    }
    out
}

fn names(c: &Compiled, d: usize) -> Vec<String> {
    c.table.ds[d].subfields.iter().map(|&s| c.table.syms[s].name.clone()).filter(|n| !n.is_empty()).collect()
}

/// How many subfields are in only the target, and only the source.
pub(crate) fn counts(c: &Compiled) -> (usize, usize) {
    summaries(c).iter().fold((0, 0), |(a, b), s| {
        let (t, v) = (names(c, s.target), names(c, s.source));
        (a + t.iter().filter(|n| !v.contains(n)).count(), b + v.iter().filter(|n| !t.contains(n)).count())
    })
}

/// Whether two subfields' values can be assigned.
fn compatible(a: &Ty, b: &Ty) -> bool {
    a.is_numeric() == b.is_numeric() && a.is_string() == b.is_string()
}

pub(crate) fn summary(out: &mut Out, c: &Compiled) {
    let all = summaries(c);
    if all.is_empty() {
        return;
    }
    out.line('-', format!("{:23}{}", "", super::spaced("EVAL-CORR Summary")));
    for (k, s) in all.iter().enumerate() {
        let stmts: String = s.lines.iter().map(|l| slot(c, (*l, ' '))).collect();
        out.line(' ', format!("{:<45}{}", format!("     EVAL-CORR summary {}", k + 1), stmts.trim_end()));
        let source = names(c, s.source);
        let target = names(c, s.target);
        let ty = |d: usize, n: &str| c.table.subfield(d, n).map(|i| c.table.syms[i].ty.clone()).unwrap_or_default();
        for n in &target {
            if source.contains(n) {
                let text = if compatible(&ty(s.target, n), &ty(s.source, n)) { "Assigned: the types agree." } else { "Not assigned: the types differ." };
                out.line(' ', format!("{:9}{n:<18}{text}", ""));
            } else {
                out.line(' ', format!("{:<9}{n:<18}Only in the target.", "*RNF7341"));
            }
        }
        for n in source.iter().filter(|n| !target.contains(n)) {
            out.line(' ', format!("{:<9}{n:<18}Only in the source.", "*RNF7342"));
        }
    }
    out.line('-', "    * * * * *   E N D   O F   E V A L - C O R R   S U M M A R Y   * * * * *");
}
