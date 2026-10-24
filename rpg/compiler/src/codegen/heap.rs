// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The heap operations `ALLOC`, `REALLOC` and `DEALLOC` (the built-ins `%ALLOC`
//! and `%REALLOC` are with the others).

use rpgfront::ast::{Expr, Op};

use super::Gen;
use super::expr::Want;
use crate::diag::ids;
use crate::types::Ty;

impl Gen<'_> {
    /// `ALLOC length pointer`: storage from the default heap.
    pub fn alloc_op(&mut self, op: &Op) {
        let (Some(n), Some(t)) = (op.f2.first(), op.result.first()) else {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, op.code.clone());
            return;
        };
        let Some(nv) = self.expr(n, Want::default()) else { return };
        let Some(n) = self.int_reg(&nv) else { return };
        let Some(t) = self.target(t) else { return };
        self.rt_proc("_QRNX_ALLOC", "($p: ptr, byval $n: i64)");
        self.b.ins(format!("callb _QRNX_ALLOC({}, {n})", t.op));
    }

    /// `REALLOC length pointer`: the pointer's storage made that long.
    pub fn realloc_op(&mut self, op: &Op) {
        let (Some(n), Some(t)) = (op.f2.first(), op.result.first()) else {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, op.code.clone());
            return;
        };
        let Some(nv) = self.expr(n, Want::default()) else { return };
        let Some(n) = self.int_reg(&nv) else { return };
        let Some(t) = self.target(t) else { return };
        self.rt_proc("_QRNX_REALLOC", "($p: ptr, byval $n: i64)");
        self.b.ins(format!("callb _QRNX_REALLOC({}, {n})", t.op));
    }

    /// `DEALLOC pointer`; with `(N)` the pointer is also set to null.
    pub fn dealloc_op(&mut self, op: &Op) {
        let Some(e) = op.result.first().or_else(|| op.f2.first()) else {
            self.err(ids::BAD_EXPRESSION_TYPE, 30, op.code.clone());
            return;
        };
        // A literal was reported (`blocks`).
        if matches!(e, Expr::Lit(..)) {
            return;
        }
        let Some(t) = self.target(e) else { return };
        if t.ty != Ty::Ptr {
            self.err(ids::TYPE_MISMATCH, 30, String::new());
            return;
        }
        let r = self.b.reg();
        self.b.ins(format!("{r} = load {}", t.op));
        self.b.ins(format!("dealloc {r}"));
        if op.ext.contains('N') {
            let z = self.b.reg();
            self.b.ins(format!("{z} = const.ptr *NULL"));
            self.b.ins(format!("store {}, {z}", t.op));
        }
    }
}
