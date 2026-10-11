// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Addresses of places.

use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{Value, types};
use qir::verify::place::resolve;
use qir::{Place, Scope, StorageClass, Type, VarRef};

use super::{Lowerer, le};
use crate::error::Result;
use crate::rt::{Cond, RtFn};

impl Lowerer<'_, '_> {
    /// The native address of a place and its type.
    pub(crate) fn addr(&mut self, place: &Place) -> Result<(Value, Type)> {
        let acc = resolve(self.m, self.p, place).map_err(|e| self.unsupported(e))?;
        let mut a = match (acc.var, acc.deref) {
            (Some(r), _) => self.var_addr(r)?,
            (None, Some((reg, off))) => {
                let fat = self.b.use_var(self.vars[reg.0 as usize]);
                let base = self.deref(fat);
                self.b.ins().iadd_imm_s(base, off)
            }
            (None, None) => unreachable!("a place has a base"),
        };
        if acc.offset != 0 {
            #[allow(clippy::cast_possible_wrap)]
            let off = acc.offset as i64;
            a = self.b.ins().iadd_imm_s(a, off);
        }
        for term in &acc.index {
            let reg_ty = self.p.reg_type(term.reg).expect("verified").clone();
            let raw = self.b.use_var(self.vars[term.reg.0 as usize]);
            let idx = self.extend_i64(raw, &reg_ty);
            let bad = self.b.ins().icmp_imm_s(
                IntCC::UnsignedGreaterThanOrEqual,
                idx,
                i64::from(term.len),
            );
            let idx = self.guard(bad, Cond::Mch0603, types::I64, |_| idx);
            #[allow(clippy::cast_possible_wrap)]
            let stride = self.b.ins().iconst(types::I64, term.stride as i64);
            let off = self.b.ins().imul(idx, stride);
            a = self.b.ins().iadd(a, off);
        }
        Ok((a, acc.ty))
    }

    /// An integer register value as an `i64`, by its signedness.
    pub(crate) fn extend_i64(&mut self, v: Value, ty: &Type) -> Value {
        match ty {
            Type::I64 | Type::U64 => v,
            Type::I8 | Type::I16 | Type::I32 => self.b.ins().sextend(types::I64, v),
            _ => self.b.ins().uextend(types::I64, v),
        }
    }

    /// The native address a fat pointer addresses, through the runtime.
    pub(crate) fn deref(&mut self, fat: Value) -> Value {
        let (lo, hi) = self.b.ins().isplit(fat);
        let f = self.rt_ref(RtFn::Deref);
        let call = self.b.ins().call(f, &[self.env, lo, hi]);
        let addr = self.b.inst_results(call)[0];
        self.check_pending();
        addr
    }

    fn var_addr(&mut self, r: VarRef) -> Result<Value> {
        let v = self.m.var(r, Some(self.p)).expect("verified");
        Ok(match (&r.scope, &v.class) {
            (_, StorageClass::Static | StorageClass::Const) => {
                let slot = match r.scope {
                    Scope::Module => self.layout.module_var(r.index),
                    Scope::Proc => self.layout.proc_var(self.pi, r.index),
                }
                .expect("laid out");
                let base = self.static_base();
                #[allow(clippy::cast_possible_wrap)]
                let off = slot.offset as i64;
                self.b.ins().iadd_imm_s(base, off)
            }
            (_, StorageClass::Import) => self.import_data(&v.name),
            (_, StorageClass::Based(on)) => {
                let pa = self.var_addr(*on)?;
                let fat = self.b.ins().load(types::I128, le(), pa, 0);
                self.deref(fat)
            }
            (Scope::Proc, StorageClass::Auto) => {
                let slot = self.slots[r.index as usize].expect("auto has a slot");
                self.b.ins().stack_addr(types::I64, slot, 0)
            }
            (Scope::Proc, StorageClass::Param(k)) => {
                let param = &self.p.params[*k as usize];
                if param.by_value {
                    let slot = self.slots[r.index as usize].expect("by-value has a slot");
                    self.b.ins().stack_addr(types::I64, slot, 0)
                } else {
                    let ptr = self.param_vals[*k as usize];
                    let bad = self.b.ins().icmp_imm_s(IntCC::Equal, ptr, 0);
                    self.guard(bad, Cond::Mch3601, types::I64, |_| ptr)
                }
            }
            (Scope::Module, StorageClass::Auto | StorageClass::Param(_)) => {
                unreachable!("verified: no automatics at module scope")
            }
        })
    }
}
