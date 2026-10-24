// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Fixed-form program calls and the program's own parameters: `CALL`
//! with the `PARM` lines after it or a named `PLIST`, the `*ENTRY PLIST`
//! (or a global `DCL-PI`) that the program entry procedure binds, and
//! that entry procedure itself.

use std::collections::HashMap;
use std::fmt::Write as _;

use rpgfront::ast::{Expr, Lit, Module, Op, Stmt};

use super::expr::{Vk, Want};
use super::{Body, Gen};
use crate::diag::ids;
use crate::names::mangle;

/// The `PARM` lines of the module: those after each `CALL` (by its
/// line) and those of each named `PLIST`.
#[derive(Clone, Debug, Default)]
pub struct Plists {
    /// The `PARM`s following a `CALL`, by the `CALL`'s line.
    pub after: HashMap<u32, Vec<Op>>,
    /// Named lists.
    pub named: HashMap<String, Vec<Op>>,
}

/// Collect the module's parameter lists.
#[must_use]
pub fn plists(m: &Module) -> Plists {
    let mut out = Plists::default();
    let all: Vec<&Stmt> = m.calcs.iter().chain(m.procs.iter().flat_map(|p| p.calcs.iter())).collect();
    let mut i = 0;
    while i < all.len() {
        let s = all[i];
        let collect = |from: usize| -> Vec<Op> {
            all[from..].iter().take_while(|x| x.op.code == "PARM").map(|x| x.op.clone()).collect()
        };
        match s.op.code.as_str() {
            "CALL" | "CALLB" => {
                out.after.insert(s.at.line, collect(i + 1));
            }
            "PLIST" => {
                if let Some(n) = s.op.f1.first().and_then(name_of) {
                    out.named.insert(n, collect(i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// The names `*ENTRY PLIST`'s `PARM` lines give, in order.
#[must_use]
pub fn entry_names(m: &Module) -> Vec<String> {
    let p = plists(m);
    p.named
        .get("*ENTRY")
        .map(|ops| ops.iter().filter_map(|o| o.result.first().and_then(name_of)).collect())
        .unwrap_or_default()
}

fn name_of(e: &Expr) -> Option<String> {
    match e {
        Expr::Special { word, .. } => Some(word.to_ascii_uppercase()),
        _ => e.name().map(str::to_ascii_uppercase),
    }
}

impl Gen<'_> {
    /// `CALL pgm [plist]`: the `PARM` factor 2s into their result
    /// fields, the dynamic call with the result fields by reference,
    /// then the result fields into the factor 1s.
    pub fn call_op(&mut self, s: &Stmt) {
        let op = &s.op;
        let mut parms = self.plists.after.get(&s.at.line).cloned().unwrap_or_default();
        if parms.is_empty()
            && let Some(n) = op.result.first().and_then(name_of)
        {
            match self.plists.named.get(&n) {
                Some(p) => parms.clone_from(p),
                None => {
                    self.err(ids::NOT_DEFINED, 30, n);
                    return;
                }
            }
        }
        let mut args = Vec::new();
        for p in &parms {
            let Some(r) = p.result.first() else { continue };
            let Some(t) = self.target(r) else { return };
            if let Some(src) = p.f2.first() {
                let Some(v) = self.expr(src, Want::default()) else { return };
                self.assign(&t, &v, false);
            }
            args.push(t);
        }
        let list = args.iter().map(|a| a.op.clone()).collect::<Vec<_>>().join(", ");
        match op.f2.first() {
            Some(Expr::Lit(Lit::Char(name), _)) => {
                let up = name.trim().to_ascii_uppercase();
                let target = match up.split_once('/') {
                    Some((l, n)) => format!("{}/{}", mangle(l), mangle(n)),
                    None => mangle(&up),
                };
                self.b.ins(format!("call {target}({list})"));
            }
            Some(e) => {
                let Some(v) = self.expr(e, Want::default()) else { return };
                if v.kind != Vk::Place || !v.ty.is_char() {
                    self.err(ids::TYPE_MISMATCH, 30, String::new());
                    return;
                }
                self.b.ins(format!("call {}({list})", v.op));
            }
            None => {
                self.err(ids::BAD_EXPRESSION_TYPE, 30, op.code.clone());
                return;
            }
        }
        for (p, t) in parms.iter().zip(&args) {
            if let Some(dst) = p.f1.first() {
                let Some(d) = self.target(dst) else { return };
                self.assign(&d, t, false);
            }
        }
    }
}

/// The program entry procedure, `_QRNP_PEP_name`, as IBM names it:
/// it takes the program's parameters by reference (each may be left
/// out), points the parameter fields at them, keeps the count for
/// `%PARMS`, and calls the main procedure, so an escape the main
/// procedure sends to its caller reaches the program's caller.
pub fn pep(g: &mut Gen<'_>, main: &str) -> String {
    let mut sig = Vec::new();
    let mut b = Body::new();
    b.ins("%n = parms");
    b.ins("store $entry_n, %n");
    for (i, &id) in g.t.entry.clone().iter().enumerate() {
        let s = g.t.syms[id].clone();
        let ty = g.t.ir_storage(&s);
        sig.push(format!("$p{}: {ty} *NOPASS", i + 1));
        let ptr = &g.t.syms[g.t.entry_ptrs[i]].ir;
        let (yes, no, next) = (b.label(), b.label(), b.label());
        let f = b.reg();
        b.ins(format!("{f} = passed $p{}", i + 1));
        b.term(format!("brif {f}, {yes}, {no}"));
        b.place(&yes);
        let a = b.reg();
        b.ins(format!("{a} = addr $p{}", i + 1));
        b.ins(format!("store {ptr}, {a}"));
        b.term(format!("br {next}"));
        b.place(&no);
        let z = b.reg();
        b.ins(format!("{z} = const.ptr *NULL"));
        b.ins(format!("store {ptr}, {z}"));
        b.term(format!("br {next}"));
        b.place(&next);
    }
    b.ins(format!("callb {main}()"));
    let mut head = String::new();
    let _ = write!(head, "proc _QRNP_PEP_{}({}) entry", mangle(&g.opts.name), sig.join(", "));
    b.finish(&head, "ret")
}
