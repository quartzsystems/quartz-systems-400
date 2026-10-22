// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calls: bound procedures directly, program entry procedures and
//! dynamic program calls through a parameter list, procedure pointers
//! through the runtime's validation.

#![allow(clippy::similar_names)]

use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{StackSlotData, StackSlotKind, Value, types};
use qir::verify::typing::operand_type;
use qir::{Arg, CallRet, CallTarget, Const, Linkage, Operand, Param, Type};

use super::Lowerer;
use crate::abi::{self, off32};
use crate::error::Result;
use crate::rt::{Cond, RtFn};

impl Lowerer<'_, '_> {
    /// Lower a call.
    pub(crate) fn call(
        &mut self,
        target: &CallTarget,
        args: &[Arg],
        ret: Option<&CallRet>,
    ) -> Result<()> {
        match target {
            CallTarget::Local(pr) => {
                let callee = self.m.procs[pr.0 as usize].clone();
                if callee.entry {
                    let (argc, argv) = self.argv(args, &callee.params)?;
                    let fref = self.local_ref(pr.0);
                    let zero = self.b.ins().iconst(types::I64, 0);
                    self.mark_region();
                    self.b.ins().call(fref, &[self.env, argc, zero, argv]);
                    self.check_pending();
                    return Ok(());
                }
                let fref = self.local_ref(pr.0);
                self.bound_call(
                    fref,
                    &callee.params,
                    callee.ret.as_ref(),
                    args,
                    ret,
                    callee.opdesc,
                )
            }
            CallTarget::Proto(pr) => {
                let proto = self.m.protos[pr.0 as usize].clone();
                match &proto.linkage {
                    // A bare signature called by name is an import of
                    // that name, resolved at bind time.
                    Linkage::Import | Linkage::Signature => {
                        let fref = self.import_ref(&proto);
                        self.bound_call(
                            fref,
                            &proto.params,
                            proto.ret.as_ref(),
                            args,
                            ret,
                            proto.opdesc,
                        )
                    }
                    Linkage::Pgm { name, lib } => {
                        let (argc, argv) = self.argv(args, &proto.params)?;
                        self.call_pgm(name, lib, argc, argv)
                    }
                    Linkage::SysApi => {
                        let (argc, argv) = self.argv(args, &proto.params)?;
                        self.call_pgm(&proto.name, "QSYS", argc, argv)
                    }
                }
            }
            CallTarget::PgmName { name, lib } => {
                let (argc, argv) = self.argv(args, &[])?;
                self.call_pgm(name, lib.as_deref().unwrap_or("*LIBL"), argc, argv)
            }
            CallTarget::PgmVar(place) => {
                let (argc, argv) = self.argv(args, &[])?;
                let (addr, ty) = self.addr(place)?;
                let len = self.i64(i64::try_from(ty.size()).expect("small"));
                self.rt_call(RtFn::CallPgmVar, &[addr, len, argc, argv]);
                Ok(())
            }
            CallTarget::SysPtr(r) => {
                let (argc, argv) = self.argv(args, &[])?;
                let (pa, _) = self.sop(&Operand::Reg(*r), Some(&Type::Ptr))?;
                self.rt_call(RtFn::CallPgmPtr, &[pa, argc, argv]);
                Ok(())
            }
            CallTarget::ProcPtr { ptr, proto } => {
                let proto = self.m.protos[proto.0 as usize].clone();
                let (pa, _) = self.sop(&Operand::Reg(*ptr), Some(&Type::ProcPtr))?;
                let code = self.rt_call(RtFn::ProcTarget, &[pa]).expect("result");
                let bad = self.b.ins().icmp_imm_s(IntCC::Equal, code, 0);
                let ok = self.b.create_block();
                let fail = self.b.create_block();
                self.b.ins().brif(bad, fail, &[], ok, &[]);
                self.b.switch_to_block(fail);
                self.raise(Cond::Mch3601);
                let unwind = self.unwind;
                self.b.ins().jump(unwind, &[]);
                self.b.switch_to_block(ok);
                let sig = self.b.import_signature(abi::signature(
                    &proto.params,
                    proto.ret.as_ref(),
                    false,
                ));
                let vals =
                    self.bound_args(&proto.params, proto.ret.as_ref(), args, ret, proto.opdesc)?;
                self.mark_region();
                let inst = self.b.ins().call_indirect(sig, code, &vals);
                let results = self.b.inst_results(inst).to_vec();
                self.check_pending();
                if let Some(CallRet::Reg(r)) = ret {
                    self.b.def_var(self.vars[r.0 as usize], results[0]);
                }
                Ok(())
            }
        }
    }

    /// A dynamic program call by name.
    fn call_pgm(&mut self, name: &str, lib: &str, argc: Value, argv: Value) -> Result<()> {
        let n = self.name_temp(name)?;
        let l = self.name_temp(lib)?;
        self.rt_call(RtFn::CallPgm, &[n, l, argc, argv]);
        Ok(())
    }

    /// A `char(10)` temporary holding a name.
    fn name_temp(&mut self, name: &str) -> Result<Value> {
        let bytes = qir::inst::literal::encode(&Type::Char(10), &Const::Str(name.to_string()))
            .map_err(|e| self.unsupported(e))?;
        let t = self.temp(10);
        self.store_bytes(t, &bytes);
        Ok(t)
    }

    /// A bound call with its arguments in registers and storage, and
    /// their operational descriptors when the callee declares `opdesc`.
    fn bound_call(
        &mut self,
        fref: cranelift_codegen::ir::FuncRef,
        params: &[Param],
        ret_ty: Option<&Type>,
        args: &[Arg],
        ret: Option<&CallRet>,
        opdesc: bool,
    ) -> Result<()> {
        let vals = self.bound_args(params, ret_ty, args, ret, opdesc)?;
        self.mark_region();
        let inst = self.b.ins().call(fref, &vals);
        let results = self.b.inst_results(inst).to_vec();
        self.check_pending();
        if let Some(CallRet::Reg(r)) = ret {
            self.b.def_var(self.vars[r.0 as usize], results[0]);
        }
        Ok(())
    }

    /// The argument values of a bound call: `env`, `argc`, `opdesc`
    /// (the descriptor list when asked for, else 0), the return buffer
    /// when the type needs one, then each parameter.
    fn bound_args(
        &mut self,
        params: &[Param],
        ret_ty: Option<&Type>,
        args: &[Arg],
        ret: Option<&CallRet>,
        opdesc: bool,
    ) -> Result<Vec<Value>> {
        let mut vals: Vec<Value> = Vec::with_capacity(args.len() + 4);
        vals.push(self.env);
        let count = i64::try_from(args.len()).expect("fewer than 2^63 arguments");
        vals.push(self.b.ins().iconst(types::I64, count));
        let list = if opdesc {
            self.descriptors(params, args)?
        } else {
            self.b.ins().iconst(types::I64, 0)
        };
        vals.push(list);
        if !abi::returns_in_register(ret_ty) {
            let size = ret_ty.map_or(0, Type::size);
            let buf = match ret {
                Some(CallRet::Place(p)) => self.addr(p)?.0,
                _ => self.temp(size),
            };
            vals.push(buf);
        }
        for (k, param) in params.iter().enumerate() {
            let v = match args.get(k) {
                None | Some(Arg::Omit) => {
                    if param.by_value && param.ty.is_register() {
                        self.zero(abi::reg_type(&param.ty))
                    } else {
                        self.b.ins().iconst(types::I64, 0)
                    }
                }
                Some(Arg::Value(op)) => {
                    if param.by_value && param.ty.is_register() {
                        self.value(op, &param.ty)?
                    } else {
                        self.arg_address(op, param)?
                    }
                }
            };
            vals.push(v);
        }
        Ok(vals)
    }

    /// The operational descriptor list of a call, in a frame slot:
    /// `abi::OPDESC_HEADER`, then one `abi::OPDESC_ENTRY` per argument.
    fn descriptors(&mut self, params: &[Param], args: &[Arg]) -> Result<Value> {
        let n = u32::try_from(args.len().max(params.len())).expect("few arguments");
        let slot = self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            abi::OPDESC_HEADER + abi::OPDESC_ENTRY * n,
            3,
        ));
        let list = self.b.ins().stack_addr(types::I64, slot, 0);
        let count = self.b.ins().iconst(types::I32, i64::from(n));
        self.b.ins().store(super::le(), count, list, 0);
        let zero = self.b.ins().iconst(types::I32, 0);
        self.b.ins().store(super::le(), zero, list, 4);
        for k in 0..n {
            let words = match (args.get(k as usize), params.get(k as usize)) {
                (Some(Arg::Value(op)), Some(param)) => abi::describe(&self.arg_type(op, param)?),
                (Some(Arg::Value(op)), None) => {
                    let ty = operand_type(self.m, self.p, op, None).map_err(|e| self.unsupported(e))?;
                    abi::describe(&ty)
                }
                _ => [abi::opdesc::NONE; 5],
            };
            let base = u64::from(abi::OPDESC_HEADER + abi::OPDESC_ENTRY * k);
            for (i, w) in words.iter().enumerate() {
                let v = self.b.ins().iconst(types::I32, i64::from(*w));
                let at = off32(base + 4 * u64::try_from(i).expect("five words"));
                self.b.ins().store(super::le(), v, list, at);
            }
            self.b.ins().store(super::le(), zero, list, off32(base + 20));
        }
        Ok(list)
    }

    /// The type an argument is passed as: a literal takes the
    /// parameter's type, cut to its own length for `*VARSIZE` text.
    fn arg_type(&self, op: &Operand, param: &Param) -> Result<Type> {
        match op {
            Operand::Const(Const::Str(s)) if param.varsize && matches!(param.ty, Type::Char(_)) => {
                Ok(Type::Char(u32::try_from(s.len().max(1)).expect("literal fits")))
            }
            Operand::Const(_) => Ok(param.ty.clone()),
            _ => operand_type(self.m, self.p, op, Some(&param.ty)).map_err(|e| self.unsupported(e)),
        }
    }

    /// A parameter list for a program call: every argument by address,
    /// `*OMIT` and unpassed ones null. Returns `argc` and `argv`.
    fn argv(&mut self, args: &[Arg], params: &[Param]) -> Result<(Value, Value)> {
        let n = args.len().max(params.len());
        let slot = self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            u32::try_from(8 * n.max(1)).expect("few parameters"),
            3,
        ));
        let argv = self.b.ins().stack_addr(types::I64, slot, 0);
        for k in 0..n {
            let v = match args.get(k) {
                None | Some(Arg::Omit) => self.b.ins().iconst(types::I64, 0),
                Some(Arg::Value(op)) => {
                    if let Some(p) = params.get(k) {
                        self.arg_address(op, p)?
                    } else {
                        // An untyped program call: each argument as its own type.
                        self.sop(op, None)?.0
                    }
                }
            };
            self.b
                .ins()
                .store(super::le(), v, argv, off32(8 * k as u64));
        }
        let argc = i64::try_from(args.len()).expect("few parameters");
        let argc = self.b.ins().iconst(types::I64, argc);
        Ok((argc, argv))
    }

    /// The address passed for a by-reference argument: the place, or a
    /// temporary holding a literal or a spilled register.
    fn arg_address(&mut self, op: &Operand, param: &Param) -> Result<Value> {
        match op {
            Operand::Place(p) => Ok(self.addr(p)?.0),
            Operand::Const(c) => {
                let ty = match (c, &param.ty) {
                    (Const::Str(s), Type::Char(_)) if param.varsize => {
                        Type::Char(u32::try_from(s.len().max(1)).expect("literal fits"))
                    }
                    (_, Type::Char(1)) if param.name.is_empty() => {
                        // An untyped program call: the literal's own type.
                        let (a, _) = self.sop(op, None)?;
                        return Ok(a);
                    }
                    _ => param.ty.clone(),
                };
                let bytes = qir::inst::literal::encode(&ty, c).map_err(|e| self.unsupported(e))?;
                let t = self.temp(bytes.len() as u64);
                self.store_bytes(t, &bytes);
                Ok(t)
            }
            Operand::Reg(_) => {
                let (a, _) = self.sop(op, None)?;
                Ok(a)
            }
        }
    }

    /// A temporary of `size` bytes in the frame.
    pub(crate) fn temp(&mut self, size: u64) -> Value {
        let slot = self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            u32::try_from(size.max(1)).expect("frame fits"),
            4,
        ));
        self.b.ins().stack_addr(types::I64, slot, 0)
    }
}
