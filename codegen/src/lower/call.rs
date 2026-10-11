// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calls to bound procedures.

use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::{StackSlotData, StackSlotKind, Value, types};
use qir::{Arg, CallRet, CallTarget, Const, Linkage, Operand, Param, Type};

use super::Lowerer;
use crate::abi;
use crate::error::Result;

impl Lowerer<'_, '_> {
    /// Lower a call.
    pub(crate) fn call(
        &mut self,
        target: &CallTarget,
        args: &[Arg],
        ret: Option<&CallRet>,
    ) -> Result<()> {
        let (fref, params, ret_ty): (_, Vec<Param>, Option<Type>) = match target {
            CallTarget::Local(pr) => {
                let callee = &self.m.procs[pr.0 as usize];
                (
                    self.local_ref(pr.0),
                    callee.params.clone(),
                    callee.ret.clone(),
                )
            }
            CallTarget::Proto(pr) => {
                let proto = &self.m.protos[pr.0 as usize];
                if proto.linkage != Linkage::Import {
                    return Err(
                        self.unsupported(format!("call of {} through the runtime", proto.name))
                    );
                }
                let proto = proto.clone();
                (
                    self.import_ref(&proto),
                    proto.params.clone(),
                    proto.ret.clone(),
                )
            }
            CallTarget::PgmName { .. } | CallTarget::PgmVar(_) => {
                return Err(self.unsupported("a dynamic program call"));
            }
            CallTarget::ProcPtr { .. } => {
                return Err(self.unsupported("a call through a procedure pointer"));
            }
            CallTarget::SysPtr(_) => {
                return Err(self.unsupported("a call through a system pointer"));
            }
        };
        let mut vals: Vec<Value> = Vec::with_capacity(args.len() + 4);
        vals.push(self.env);
        let count = i64::try_from(args.len()).expect("fewer than 2^63 arguments");
        vals.push(self.b.ins().iconst(types::I64, count));
        vals.push(self.b.ins().iconst(types::I64, 0));
        let in_register = abi::returns_in_register(ret_ty.as_ref());
        if !in_register {
            let size = ret_ty.as_ref().map_or(0, Type::size);
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
        let inst = self.b.ins().call(fref, &vals);
        let results = self.b.inst_results(inst).to_vec();
        self.check_pending();
        if let Some(CallRet::Reg(r)) = ret {
            self.b.def_var(self.vars[r.0 as usize], results[0]);
        }
        Ok(())
    }

    /// The address passed for a by-reference argument: the place, or a
    /// temporary holding a literal.
    fn arg_address(&mut self, op: &Operand, param: &Param) -> Result<Value> {
        match op {
            Operand::Place(p) => Ok(self.addr(p)?.0),
            Operand::Const(c) => {
                let ty = match (c, &param.ty) {
                    (Const::Str(s), Type::Char(_)) if param.varsize => {
                        Type::Char(u32::try_from(s.len().max(1)).expect("literal fits"))
                    }
                    _ => param.ty.clone(),
                };
                let bytes = qir::inst::literal::encode(&ty, c).map_err(|e| self.unsupported(e))?;
                let t = self.temp(bytes.len() as u64);
                self.store_bytes(t, &bytes);
                Ok(t)
            }
            Operand::Reg(_) => unreachable!("verified: registers are not passed by reference"),
        }
    }

    /// A temporary of `size` bytes in the frame.
    fn temp(&mut self, size: u64) -> Value {
        let slot = self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            u32::try_from(size.max(1)).expect("frame fits"),
            4,
        ));
        self.b.ins().stack_addr(types::I64, slot, 0)
    }
}
