// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Definite assignment: every register is written on every path before
//! it is read.

use super::monitor::Region;
use crate::inst::{
    Arg, CallRet, CallTarget, Debug, DtInst, DtaqInst, Index, Inst, IoInst, MsgInst, MsgQueue,
    Operand, Place, PlaceBase, PtrInst, Step, StrInst,
};
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
        other => ext_uses(other, &mut out),
    }
    out
}

fn opt_uses(o: Option<&Operand>, out: &mut Vec<Reg>) {
    if let Some(o) = o {
        operand_uses(o, out);
    }
}

fn opt_place_uses(p: Option<&Place>, out: &mut Vec<Reg>) {
    if let Some(p) = p {
        place_uses(p, out);
    }
}

fn queue_uses(q: &MsgQueue, out: &mut Vec<Reg>) {
    if let MsgQueue::Up(n) = q {
        operand_uses(n, out);
    }
}

#[allow(clippy::too_many_lines)]
fn ext_uses(i: &Inst, out: &mut Vec<Reg>) {
    match i {
        Inst::DecBin { dst, a, b, .. } => {
            place_uses(dst, out);
            operand_uses(a, out);
            operand_uses(b, out);
        }
        Inst::DecUn { dst, src, .. } | Inst::Convert { dst, src, .. } => {
            place_uses(dst, out);
            operand_uses(src, out);
        }
        Inst::DecCmp { a, b, .. } => {
            operand_uses(a, out);
            operand_uses(b, out);
        }
        Inst::Str(s) => match s {
            StrInst::Cat { dst, a, b } => {
                place_uses(dst, out);
                operand_uses(a, out);
                operand_uses(b, out);
            }
            StrInst::Subst {
                dst,
                src,
                start,
                len,
            } => {
                place_uses(dst, out);
                operand_uses(src, out);
                operand_uses(start, out);
                operand_uses(len, out);
            }
            StrInst::Scan {
                needle: a,
                hay: b,
                start,
                ..
            }
            | StrInst::Check {
                set: a,
                src: b,
                start,
                ..
            } => {
                operand_uses(a, out);
                operand_uses(b, out);
                opt_uses(start.as_ref(), out);
            }
            StrInst::Xlate {
                dst,
                from,
                to,
                src,
                start,
            } => {
                place_uses(dst, out);
                operand_uses(from, out);
                operand_uses(to, out);
                operand_uses(src, out);
                opt_uses(start.as_ref(), out);
            }
            StrInst::Trim {
                dst, src, chars, ..
            } => {
                place_uses(dst, out);
                operand_uses(src, out);
                opt_uses(chars.as_ref(), out);
            }
            StrInst::Cmp { a, b, .. } => {
                operand_uses(a, out);
                operand_uses(b, out);
            }
            StrInst::Len { src, .. } => operand_uses(src, out),
            StrInst::SetLen { dst, n } => {
                place_uses(dst, out);
                operand_uses(n, out);
            }
            StrInst::Replace {
                dst,
                src,
                repl,
                start,
                len,
            } => {
                place_uses(dst, out);
                operand_uses(src, out);
                operand_uses(repl, out);
                operand_uses(start, out);
                opt_uses(len.as_ref(), out);
            }
        },
        Inst::Dt(d) => match d {
            DtInst::Add { dst, src, n, .. } => {
                place_uses(dst, out);
                operand_uses(src, out);
                operand_uses(n, out);
            }
            DtInst::Diff { a, b, .. } => {
                operand_uses(a, out);
                operand_uses(b, out);
            }
            DtInst::Extract { src, .. } | DtInst::Test { src, .. } => operand_uses(src, out),
            DtInst::Now { dst } => place_uses(dst, out),
        },
        Inst::Ptr(pt) => match pt {
            PtrInst::Addr { place, .. } => place_uses(place, out),
            PtrInst::PtrAdd { ptr: a, n: b, .. } | PtrInst::PtrDiff { p: a, q: b, .. } => {
                operand_uses(a, out);
                operand_uses(b, out);
            }
            PtrInst::ProcAddr { .. } => {}
            PtrInst::Rslvsp { name, lib, .. } => {
                operand_uses(name, out);
                operand_uses(lib, out);
            }
        },
        Inst::Io(io) => match io {
            IoInst::DbOpen { infds, .. } => opt_place_uses(infds.as_ref(), out),
            IoInst::DbClose { .. } => {}
            IoInst::DbIo { rec, key, rrn, .. } => {
                opt_place_uses(rec.as_ref(), out);
                opt_uses(key.as_ref(), out);
                opt_uses(rrn.as_ref(), out);
            }
            IoInst::DevIo { rec, .. } => opt_place_uses(rec.as_ref(), out),
        },
        Inst::Msg(m) => match m {
            MsgInst::Snd {
                msgid,
                msgf,
                msgf_lib,
                data,
                to,
                key,
                ..
            } => {
                operand_uses(msgid, out);
                operand_uses(msgf, out);
                opt_uses(msgf_lib.as_ref(), out);
                opt_uses(data.as_ref(), out);
                queue_uses(to, out);
                opt_place_uses(key.as_ref(), out);
            }
            MsgInst::Rcv {
                pgmq,
                key,
                msg,
                msgid,
                data,
                keyout,
                sev,
                ..
            } => {
                queue_uses(pgmq, out);
                opt_uses(key.as_ref(), out);
                for pl in [msg, msgid, data, keyout, sev] {
                    opt_place_uses(pl.as_ref(), out);
                }
            }
            MsgInst::Rmv { pgmq, key, .. } => {
                queue_uses(pgmq, out);
                opt_uses(key.as_ref(), out);
            }
        },
        Inst::Dsply { src, reply } => {
            operand_uses(src, out);
            opt_place_uses(reply.as_ref(), out);
        }
        Inst::Dtaara {
            name, lib, data, ..
        } => {
            operand_uses(name, out);
            operand_uses(lib, out);
            opt_place_uses(data.as_ref(), out);
        }
        Inst::Alloc { n, .. } => operand_uses(n, out),
        Inst::Realloc { ptr, n, .. } => {
            operand_uses(ptr, out);
            operand_uses(n, out);
        }
        Inst::Dealloc { ptr } => operand_uses(ptr, out),
        Inst::Dtaq(DtaqInst::Snd {
            name,
            lib,
            data,
            len,
        }) => {
            for o in [name, lib, data, len] {
                operand_uses(o, out);
            }
        }
        Inst::Dtaq(DtaqInst::Rcv {
            name,
            lib,
            data,
            len,
            wait,
            ..
        }) => {
            operand_uses(name, out);
            operand_uses(lib, out);
            place_uses(data, out);
            operand_uses(len, out);
            operand_uses(wait, out);
        }
        _ => {}
    }
}

/// Check definite assignment. An error names the block, the
/// instruction (from 1) and the register.
///
/// A monitor handler may be entered from any block its region overlaps,
/// before that block assigns anything, so it sees the intersection of
/// those blocks' entry states.
pub fn check_assigned(p: &Proc, regions: &[Region]) -> Result<(), (String, usize, String)> {
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
    // Handler predecessors contribute their entry state, not their exit state.
    // A pair of the block and the registers assigned in it before the
    // region opens (only the block holding the `monitor` has any).
    let mut entry_preds: Vec<Vec<(usize, Vec<bool>)>> = vec![Vec::new(); nb];
    for r in regions {
        let mut before = vec![false; n];
        for i in &p.blocks[r.start.0].insts[..r.start.1] {
            if let Some(d) = i.def() {
                before[d.0 as usize] = true;
            }
        }
        for h in &r.handlers {
            for b in 0..nb {
                if r.overlaps(b) {
                    let extra = if b == r.start.0 {
                        before.clone()
                    } else {
                        vec![false; n]
                    };
                    entry_preds[h.0 as usize].push((b, extra));
                }
            }
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
            if preds[b].is_empty() && entry_preds[b].is_empty() {
                v = vec![false; n];
            }
            for &pr in &preds[b] {
                let o = out_of(&inn[pr], &defs[pr]);
                for k in 0..n {
                    v[k] = v[k] && o[k];
                }
            }
            for (pr, extra) in &entry_preds[b] {
                for k in 0..n {
                    v[k] = v[k] && (inn[*pr][k] || extra[k]);
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
