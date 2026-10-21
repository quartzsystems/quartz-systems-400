// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The scalar and control-flow instructions.

use std::cmp::Ordering;

use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{TrapCode, Value, types};
use qir::inst::{BinOp, Cond as QCond, Debug, UnOp};
use qir::verify::typing::{operand_type, pair_type};
use qir::{Inst, Operand, Type, TypeClass};

use super::Lowerer;
use crate::abi;
use crate::error::Result;
use crate::rt::{Cond, RtFn};

impl Lowerer<'_, '_> {
    /// Lower one instruction.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn inst(&mut self, i: &Inst) -> Result<()> {
        match i {
            Inst::Const { dst, value } => {
                let ty = self.p.reg_type(*dst).expect("typed").clone();
                let v = self.const_value(value, &ty)?;
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Load { dst, src } => {
                let (addr, ty) = self.addr(src)?;
                let v = self.load_typed(addr, &ty);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Store { dst, src } => {
                let (addr, ty) = self.addr(dst)?;
                let v = self.value(src, &ty)?;
                self.store_typed(addr, &ty, v);
            }
            Inst::Mov { dst, src } => self.mov(dst, src)?,
            Inst::Bin { dst, op, a, b, chk } => {
                let ty = if matches!(op, BinOp::Shl | BinOp::Shr) {
                    operand_type(self.m, self.p, a, None).map_err(|e| self.unsupported(e))?
                } else {
                    pair_type(self.m, self.p, a, b).map_err(|e| self.unsupported(e))?
                };
                let av = self.value(a, &ty)?;
                let bv = if matches!(op, BinOp::Shl | BinOp::Shr) {
                    let bty = operand_type(self.m, self.p, b, Some(&Type::I32))
                        .map_err(|e| self.unsupported(e))?;
                    self.value(b, &bty)?
                } else {
                    self.value(b, &ty)?
                };
                let v = self.bin(*op, &ty, av, bv, *chk)?;
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Un { dst, op, a } => {
                let ty = operand_type(self.m, self.p, a, None).map_err(|e| self.unsupported(e))?;
                let av = self.value(a, &ty)?;
                let float = ty.class() == TypeClass::Float;
                let v = match op {
                    UnOp::Neg if float => self.b.ins().fneg(av),
                    UnOp::Neg => self.b.ins().ineg(av),
                    UnOp::Abs if float => self.b.ins().fabs(av),
                    UnOp::Abs => self.b.ins().iabs(av),
                    UnOp::Not => self.b.ins().bnot(av),
                };
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Cmp { dst, cond, a, b } => {
                let ty = pair_type(self.m, self.p, a, b).map_err(|e| self.unsupported(e))?;
                let av = self.value(a, &ty)?;
                let bv = self.value(b, &ty)?;
                let v = if ty.class() == TypeClass::Float {
                    let cc = match cond {
                        QCond::Eq => FloatCC::Equal,
                        QCond::Ne => FloatCC::NotEqual,
                        QCond::Lt => FloatCC::LessThan,
                        QCond::Le => FloatCC::LessThanOrEqual,
                        QCond::Gt => FloatCC::GreaterThan,
                        QCond::Ge => FloatCC::GreaterThanOrEqual,
                    };
                    self.b.ins().fcmp(cc, av, bv)
                } else {
                    let cc = int_cc(*cond, ty.class() != TypeClass::UInt);
                    self.b.ins().icmp(cc, av, bv)
                };
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Select { dst, c, a, b } => {
                let ty = pair_type(self.m, self.p, a, b).map_err(|e| self.unsupported(e))?;
                let cty = operand_type(self.m, self.p, c, Some(&Type::Ind))
                    .map_err(|e| self.unsupported(e))?;
                let cv = self.value(c, &cty)?;
                let av = self.value(a, &ty)?;
                let bv = self.value(b, &ty)?;
                let v = self.b.ins().select(cv, av, bv);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Cvt { dst, src, chk } => {
                let dty = self.p.reg_type(*dst).expect("typed").clone();
                let sty = operand_type(self.m, self.p, src, Some(&dty))
                    .map_err(|e| self.unsupported(e))?;
                let v = match src {
                    Operand::Place(p) if !sty.is_register() => {
                        self.cvt_from_storage(p, &dty, *chk)?
                    }
                    _ => {
                        let sv = self.value(src, &sty)?;
                        self.cvt(sv, &sty, &dty, *chk)?
                    }
                };
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Br(l) => {
                self.b.ins().jump(self.blocks[l.0 as usize], &[]);
            }
            Inst::BrIf { c, t, f } => {
                let cty = operand_type(self.m, self.p, c, Some(&Type::Ind))
                    .map_err(|e| self.unsupported(e))?;
                let cv = self.value(c, &cty)?;
                self.b.ins().brif(
                    cv,
                    self.blocks[t.0 as usize],
                    &[],
                    self.blocks[f.0 as usize],
                    &[],
                );
            }
            Inst::Switch { v, cases, default } => {
                let ty = operand_type(self.m, self.p, v, None).map_err(|e| self.unsupported(e))?;
                let val = self.value(v, &ty)?;
                let bits = ty.int_bits().unwrap_or(64);
                for (k, l) in cases {
                    let fits =
                        bits == 64 || (-(1i64 << (bits - 1))..(1i64 << (bits - 1))).contains(k);
                    if !fits {
                        continue;
                    }
                    let hit = self.b.ins().icmp_imm_s(IntCC::Equal, val, *k);
                    let next = self.b.create_block();
                    self.b
                        .ins()
                        .brif(hit, self.blocks[l.0 as usize], &[], next, &[]);
                    self.b.switch_to_block(next);
                }
                self.b.ins().jump(self.blocks[default.0 as usize], &[]);
            }
            Inst::Ret(v) => self.ret(v.as_ref())?,
            Inst::Call { target, args, ret } => self.call(target, args, ret.as_ref())?,
            Inst::Parms { dst } => {
                let v = self.b.ins().ireduce(types::I32, self.argc);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Passed { dst, param } | Inst::Omitted { dst, param } => {
                let k = *param as usize;
                let given =
                    self.b
                        .ins()
                        .icmp_imm_s(IntCC::SignedGreaterThan, self.argc, i64::from(*param));
                let by_value = self.p.params[k].by_value;
                let v = if by_value {
                    if matches!(i, Inst::Passed { .. }) {
                        given
                    } else {
                        self.b.ins().iconst(types::I8, 0)
                    }
                } else {
                    let ptr = self.param_vals[k];
                    let cc = if matches!(i, Inst::Passed { .. }) {
                        IntCC::NotEqual
                    } else {
                        IntCC::Equal
                    };
                    let null_test = self.b.ins().icmp_imm_s(cc, ptr, 0);
                    self.b.ins().band(given, null_test)
                };
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Debug(Debug::Stmt(n)) => self.stmt(*n),
            Inst::Debug(Debug::Var { name, place }) if self.debug => {
                let (a, d) = self.place_pair(place)?;
                let (na, _) = self.sop(
                    &Operand::Const(qir::Const::Str(name.clone())),
                    Some(&Type::Char(64)),
                )?;
                self.rt_call(RtFn::DbgVar, &[na, a, d]);
            }
            Inst::Debug(_) => {}
            Inst::Unreachable => {
                self.b.ins().trap(TrapCode::user(1).expect("nonzero"));
            }
            other => self.ext_inst(other)?,
        }
        Ok(())
    }

    /// An operand as a register value of a type.
    pub(crate) fn value(&mut self, op: &Operand, ty: &Type) -> Result<Value> {
        match op {
            Operand::Reg(r) => Ok(self.b.use_var(self.vars[r.0 as usize])),
            Operand::Place(p) => {
                let (addr, pty) = self.addr(p)?;
                if !pty.is_register() {
                    return Err(self.unsupported(format!("a {pty} as a register value")));
                }
                Ok(self.load_typed(addr, &pty))
            }
            Operand::Const(c) => self.const_value(c, ty),
        }
    }

    fn mov(&mut self, dst: &qir::Place, src: &Operand) -> Result<()> {
        let (daddr, dty) = self.addr(dst)?;
        match src {
            Operand::Const(c) => {
                let bytes = qir::inst::literal::encode(&dty, c).map_err(|e| self.unsupported(e))?;
                self.store_bytes(daddr, &bytes);
            }
            Operand::Reg(_) => {
                let v = self.value(src, &dty)?;
                self.store_typed(daddr, &dty, v);
            }
            Operand::Place(sp) => {
                let (saddr, sty) = self.addr(sp)?;
                if dty == sty || (dty.size() == sty.size() && dty.class() == sty.class()) {
                    self.copy(daddr, saddr, dty.size());
                } else if dty.is_string() && sty.is_string() {
                    let dd = self.i64(crate::abi::TypeDesc::of(&dty).word());
                    let sd = self.i64(crate::abi::TypeDesc::of(&sty).word());
                    self.rt_call(RtFn::MovStr, &[daddr, dd, saddr, sd]);
                } else {
                    return Err(self.unsupported(format!("mov of a {sty} into a {dty}")));
                }
            }
        }
        Ok(())
    }

    fn bin(&mut self, op: BinOp, ty: &Type, a: Value, b: Value, chk: bool) -> Result<Value> {
        let float = ty.class() == TypeClass::Float;
        let signed = ty.class() == TypeClass::Int;
        let cl = abi::reg_type(ty);
        Ok(match op {
            BinOp::Add if float => self.b.ins().fadd(a, b),
            BinOp::Sub if float => self.b.ins().fsub(a, b),
            BinOp::Mul if float => self.b.ins().fmul(a, b),
            BinOp::Div if float => {
                let zero = self.zero(cl);
                let bad = self.b.ins().fcmp(FloatCC::Equal, b, zero);
                self.guard(bad, Cond::Mch1211, cl, |s| s.b.ins().fdiv(a, b))
            }
            BinOp::Rem if float => return Err(self.unsupported("rem of floats")),
            BinOp::Add | BinOp::Sub | BinOp::Mul if chk => {
                let (v, of) = match (op, signed) {
                    (BinOp::Add, true) => self.b.ins().sadd_overflow(a, b),
                    (BinOp::Add, false) => self.b.ins().uadd_overflow(a, b),
                    (BinOp::Sub, true) => self.b.ins().ssub_overflow(a, b),
                    (BinOp::Sub, false) => self.b.ins().usub_overflow(a, b),
                    (_, true) => self.b.ins().smul_overflow(a, b),
                    (_, false) => self.b.ins().umul_overflow(a, b),
                };
                self.guard(of, Cond::Mch1210, cl, |_| v)
            }
            BinOp::Add => self.b.ins().iadd(a, b),
            BinOp::Sub => self.b.ins().isub(a, b),
            BinOp::Mul => self.b.ins().imul(a, b),
            BinOp::Div | BinOp::Rem => {
                let zero = self.b.ins().icmp_imm_s(IntCC::Equal, b, 0);
                self.guard(zero, Cond::Mch1211, cl, |s| {
                    if signed {
                        // MIN / -1 overflows; the quotient is a size error, the remainder 0.
                        let min = s.b.ins().iconst(cl, i64::MIN >> (64 - cl.bits()));
                        let is_min = s.b.ins().icmp(IntCC::Equal, a, min);
                        let is_m1 = s.b.ins().icmp_imm_s(IntCC::Equal, b, -1);
                        let over = s.b.ins().band(is_min, is_m1);
                        if op == BinOp::Div {
                            s.guard(over, Cond::Mch1210, cl, |s| s.b.ins().sdiv(a, b))
                        } else {
                            let one = s.b.ins().iconst(cl, 1);
                            let safe_b = s.b.ins().select(over, one, b);
                            s.b.ins().srem(a, safe_b)
                        }
                    } else if op == BinOp::Div {
                        s.b.ins().udiv(a, b)
                    } else {
                        s.b.ins().urem(a, b)
                    }
                })
            }
            BinOp::And => self.b.ins().band(a, b),
            BinOp::Or => self.b.ins().bor(a, b),
            BinOp::Xor => self.b.ins().bxor(a, b),
            BinOp::Shl => self.b.ins().ishl(a, b),
            BinOp::Shr if signed => self.b.ins().sshr(a, b),
            BinOp::Shr => self.b.ins().ushr(a, b),
        })
    }

    #[allow(clippy::too_many_lines)]
    fn cvt(&mut self, v: Value, sty: &Type, dty: &Type, chk: bool) -> Result<Value> {
        use TypeClass as C;
        let scl = abi::reg_type(sty);
        let dcl = abi::reg_type(dty);
        let s_signed = sty.class() == C::Int;
        let d_signed = dty.class() == C::Int;
        Ok(match (sty.class(), dty.class()) {
            _ if dty == sty => v,
            (C::Int | C::UInt | C::Ind, C::Ind) => self.b.ins().icmp_imm_s(IntCC::NotEqual, v, 0),
            (C::Int | C::UInt | C::Ind, C::Int | C::UInt) => {
                let widened = match dcl.bits().cmp(&scl.bits()) {
                    Ordering::Greater if s_signed => self.b.ins().sextend(dcl, v),
                    Ordering::Greater => self.b.ins().uextend(dcl, v),
                    Ordering::Less => self.b.ins().ireduce(dcl, v),
                    Ordering::Equal => v,
                };
                if !chk {
                    return Ok(widened);
                }
                // Does the value survive the round trip and keep its sign?
                let back = match dcl.bits().cmp(&scl.bits()) {
                    Ordering::Less if d_signed => self.b.ins().sextend(scl, widened),
                    Ordering::Less => self.b.ins().uextend(scl, widened),
                    Ordering::Greater => self.b.ins().ireduce(scl, widened),
                    Ordering::Equal => widened,
                };
                let differs = self.b.ins().icmp(IntCC::NotEqual, back, v);
                let sign_lost = if s_signed && !d_signed {
                    self.b.ins().icmp_imm_s(IntCC::SignedLessThan, v, 0)
                } else if !s_signed && d_signed && dcl.bits() <= scl.bits() {
                    self.b.ins().icmp_imm_s(IntCC::SignedLessThan, widened, 0)
                } else {
                    self.b.ins().iconst(types::I8, 0)
                };
                let bad = self.b.ins().bor(differs, sign_lost);
                self.guard(bad, Cond::Mch1210, dcl, |_| widened)
            }
            (C::Int | C::UInt | C::Ind, C::Float) => {
                let wide = if scl.bits() < 32 {
                    if s_signed {
                        self.b.ins().sextend(types::I32, v)
                    } else {
                        self.b.ins().uextend(types::I32, v)
                    }
                } else {
                    v
                };
                if s_signed {
                    self.b.ins().fcvt_from_sint(dcl, wide)
                } else {
                    self.b.ins().fcvt_from_uint(dcl, wide)
                }
            }
            (C::Float, C::Ind) => {
                let zero = self.zero(scl);
                self.b.ins().fcmp(FloatCC::NotEqual, v, zero)
            }
            (C::Float, C::Int | C::UInt) => {
                let wide_ty = if dcl.bits() < 32 { types::I32 } else { dcl };
                let wide = if d_signed {
                    self.b.ins().fcvt_to_sint_sat(wide_ty, v)
                } else {
                    self.b.ins().fcvt_to_uint_sat(wide_ty, v)
                };
                let narrow = if wide_ty == dcl {
                    wide
                } else {
                    self.b.ins().ireduce(dcl, wide)
                };
                if !chk {
                    return Ok(narrow);
                }
                // Out of range when the integer, converted back, is not within one of the float.
                let back_int = if wide_ty == dcl {
                    narrow
                } else if d_signed {
                    self.b.ins().sextend(wide_ty, narrow)
                } else {
                    self.b.ins().uextend(wide_ty, narrow)
                };
                let differs = self.b.ins().icmp(IntCC::NotEqual, back_int, wide);
                let back = if d_signed {
                    self.b.ins().fcvt_from_sint(scl, wide)
                } else {
                    self.b.ins().fcvt_from_uint(scl, wide)
                };
                let diff = self.b.ins().fsub(v, back);
                let one = if scl == types::F32 {
                    self.b.ins().f32const(1.0)
                } else {
                    self.b.ins().f64const(1.0)
                };
                let adiff = self.b.ins().fabs(diff);
                let far = self.b.ins().fcmp(FloatCC::GreaterThanOrEqual, adiff, one);
                let bad = self.b.ins().bor(differs, far);
                self.guard(bad, Cond::Mch1210, dcl, |_| narrow)
            }
            (C::Float, C::Float) if dcl == types::F64 => self.b.ins().fpromote(dcl, v),
            (C::Float, C::Float) => self.b.ins().fdemote(dcl, v),
            _ => return Err(self.unsupported(format!("cvt of a {sty} to a {dty}"))),
        })
    }

    fn ret(&mut self, v: Option<&Operand>) -> Result<()> {
        let leave = self.rt_ref(RtFn::Leave);
        match (v, &self.p.ret) {
            (None, _) => {
                self.b.ins().call(leave, &[self.env]);
                self.b.ins().return_(&[]);
            }
            (Some(op), Some(ty)) if ty.is_register() => {
                let ty = ty.clone();
                let val = self.value(op, &ty)?;
                self.b.ins().call(leave, &[self.env]);
                self.b.ins().return_(&[val]);
            }
            (Some(op), Some(ty)) => {
                let ty = ty.clone();
                let buf = self.retbuf.expect("storage return has a buffer");
                match op {
                    Operand::Place(p) => {
                        let (saddr, _) = self.addr(p)?;
                        self.copy(buf, saddr, ty.size());
                    }
                    Operand::Const(c) => {
                        let bytes =
                            qir::inst::literal::encode(&ty, c).map_err(|e| self.unsupported(e))?;
                        self.store_bytes(buf, &bytes);
                    }
                    Operand::Reg(_) => unreachable!("verified"),
                }
                self.b.ins().call(leave, &[self.env]);
                self.b.ins().return_(&[]);
            }
            (Some(_), None) => unreachable!("verified"),
        }
        Ok(())
    }
}

pub(crate) fn int_cc(c: QCond, signed: bool) -> IntCC {
    match (c, signed) {
        (QCond::Eq, _) => IntCC::Equal,
        (QCond::Ne, _) => IntCC::NotEqual,
        (QCond::Lt, true) => IntCC::SignedLessThan,
        (QCond::Le, true) => IntCC::SignedLessThanOrEqual,
        (QCond::Gt, true) => IntCC::SignedGreaterThan,
        (QCond::Ge, true) => IntCC::SignedGreaterThanOrEqual,
        (QCond::Lt, false) => IntCC::UnsignedLessThan,
        (QCond::Le, false) => IntCC::UnsignedLessThanOrEqual,
        (QCond::Gt, false) => IntCC::UnsignedGreaterThan,
        (QCond::Ge, false) => IntCC::UnsignedGreaterThanOrEqual,
    }
}
