// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Definite assignment: every register is written on every path before
//! it is read.

use crate::inst::{Arg, CallRet, CallTarget, Debug, Index, Inst, Operand, Place, PlaceBase, Step};
use crate::module::{Proc, Reg};

fn place_uses(p: &Place, out: &mut Vec<Reg>) {
    if let PlaceBase::Deref { ptr, .. } = &p.base {
        out.push(*ptr);
    }
    for s in &p.path {
        if let Step::Index(Index::Reg(r)) = s {
            out.push(*r);
        }
    }
}

fn operand_uses(o: &Operand, out: &mut Vec<Reg>) {
    match o {
        Operand::Reg(r) => out.push(*r),
        Operand::Place(p) => place_uses(p, out),
        Operand::Const(_) => {}
    }
}

/// The registers an instruction reads.
#[must_use]
pub fn uses(i: &Inst) -> Vec<Reg> {
    let mut out = Vec::new();
    match i {
        Inst::Const { .. }
        | Inst::Br(_)
        | Inst::Ret(None)
        | Inst::Parms { .. }
        | Inst::Passed { .. }
        | Inst::Omitted { .. }
        | Inst::Debug(Debug::Stmt(_) | Debug::ScopeBegin | Debug::ScopeEnd)
        | Inst::Unreachable => {}
        Inst::Load { src, .. } => place_uses(src, &mut out),
        Inst::Store { dst, src } | Inst::Mov { dst, src } => {
            place_uses(dst, &mut out);
            operand_uses(src, &mut out);
        }
        Inst::Bin { a, b, .. } | Inst::Cmp { a, b, .. } => {
            operand_uses(a, &mut out);
            operand_uses(b, &mut out);
        }
        Inst::Un { a, .. } => operand_uses(a, &mut out),
        Inst::Select { c, a, b, .. } => {
            operand_uses(c, &mut out);
            operand_uses(a, &mut out);
            operand_uses(b, &mut out);
        }
        Inst::Cvt { src, .. }
        | Inst::BrIf { c: src, .. }
        | Inst::Switch { v: src, .. }
        | Inst::Ret(Some(src)) => {
            operand_uses(src, &mut out);
        }
        Inst::Call { target, args, ret } => {
            match target {
                CallTarget::PgmVar(p) => place_uses(p, &mut out),
                CallTarget::ProcPtr { ptr, .. } | CallTarget::SysPtr(ptr) => out.push(*ptr),
                CallTarget::Local(_) | CallTarget::Proto(_) | CallTarget::PgmName { .. } => {}
            }
            for a in args {
                if let Arg::Value(v) = a {
                    operand_uses(v, &mut out);
                }
            }
            if let Some(CallRet::Place(p)) = ret {
                place_uses(p, &mut out);
            }
        }
        Inst::Debug(Debug::Var { place, .. }) => place_uses(place, &mut out),
    }
    out
}

/// Check definite assignment. An error names the block, the
/// instruction (from 1) and the register.
pub fn check_assigned(p: &Proc) -> Result<(), (String, usize, String)> {
    let n = p.regs.len();
    let nb = p.blocks.len();
    // Registers each block defines, and its successors.
    let mut defs: Vec<Vec<bool>> = Vec::with_capacity(nb);
    let mut succ: Vec<Vec<usize>> = Vec::with_capacity(nb);
    for b in &p.blocks {
        let mut g = vec![false; n];
        for i in &b.insts {
            if let Some(d) = i.def() {
                g[d.0 as usize] = true;
            }
        }
        defs.push(g);
        succ.push(
            b.insts
                .last()
                .map(|i| i.targets().into_iter().map(|l| l.0 as usize).collect())
                .unwrap_or_default(),
        );
    }
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); nb];
    for (b, ss) in succ.iter().enumerate() {
        for &s in ss {
            preds[s].push(b);
        }
    }
    // in[b] = intersection of out[pred]; out[b] = in[b] | gen[b].
    // Start from "everything assigned" except the entry, and shrink.
    let mut inn: Vec<Vec<bool>> = vec![vec![true; n]; nb];
    inn[0] = vec![false; n];
    let out_of = |inn: &Vec<bool>, g: &Vec<bool>| -> Vec<bool> {
        inn.iter().zip(g).map(|(a, b)| *a || *b).collect()
    };
    let mut changed = true;
    while changed {
        changed = false;
        for b in 1..nb {
            let mut v = vec![true; n];
            if preds[b].is_empty() {
                v = vec![false; n];
            }
            for &pr in &preds[b] {
                let o = out_of(&inn[pr], &defs[pr]);
                for k in 0..n {
                    v[k] = v[k] && o[k];
                }
            }
            if v != inn[b] {
                inn[b] = v;
                changed = true;
            }
        }
    }
    for (bi, b) in p.blocks.iter().enumerate() {
        let mut have = inn[bi].clone();
        for (ii, i) in b.insts.iter().enumerate() {
            for r in uses(i) {
                if !have[r.0 as usize] {
                    return Err((
                        b.label.clone(),
                        ii + 1,
                        format!("%{} may be read before it is assigned", p.reg_name(r)),
                    ));
                }
            }
            if let Some(d) = i.def() {
                have[d.0 as usize] = true;
            }
        }
    }
    Ok(())
}
