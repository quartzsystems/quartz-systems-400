// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Per-instruction checks: operand types, call arguments, returns.

use super::place::place_type;
use super::typing::{const_fits, def_type, operand_type, pair_type, reg_type};
use crate::inst::{Arg, BinOp, CallRet, CallTarget, Cond, Debug, Inst, Operand};
use crate::module::{Module, Param, Proc, Reg};
use crate::types::{Type, TypeClass};

/// Check one instruction.
#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
pub fn check(m: &Module, p: &Proc, i: &Inst) -> Result<(), String> {
    // The defined register must have the type the operands give it.
    if let (Some(dst), Some(t)) = (i.def(), def_type(m, p, i)?) {
        let have = reg_type(p, dst)?;
        if have != t {
            return Err(format!(
                "%{} is a {have} but the result is a {t}",
                p.reg_name(dst)
            ));
        }
    }
    match i {
        Inst::Const { dst, value } => const_fits(value, &reg_type(p, *dst)?),
        Inst::Load { .. }
        | Inst::Parms { .. }
        | Inst::Unreachable
        | Inst::Br(_)
        | Inst::Un { .. }
        | Inst::Debug(Debug::Stmt(_) | Debug::ScopeBegin | Debug::ScopeEnd) => Ok(()),
        Inst::Store { dst, src } => {
            let t = place_type(m, p, dst)?;
            if !t.is_register() {
                return Err(format!("store to a {t}; only scalars store from registers"));
            }
            let u = operand_type(m, p, src, Some(&t))?;
            same(&t, &u)
        }
        Inst::Mov { dst, src } => {
            let t = place_type(m, p, dst)?;
            match src {
                Operand::Const(c) => const_fits(c, &t),
                Operand::Reg(r) => {
                    let u = reg_type(p, *r)?;
                    same(&t, &u)
                }
                Operand::Place(s) => {
                    let u = place_type(m, p, s)?;
                    if t == u {
                        return Ok(());
                    }
                    let ok = match (t.class(), u.class()) {
                        (TypeClass::Char, TypeClass::Char)
                        | (TypeClass::Ucs2, TypeClass::Ucs2)
                        | (TypeClass::Graphic, TypeClass::Graphic) => true,
                        (TypeClass::Aggregate, _) | (_, TypeClass::Aggregate) => {
                            t.size() == u.size()
                        }
                        _ => false,
                    };
                    if ok {
                        Ok(())
                    } else {
                        Err(format!("mov of a {u} into a {t}; convert it instead"))
                    }
                }
            }
        }
        Inst::Bin { op, a, b, chk, .. } => {
            let t = if matches!(op, BinOp::Shl | BinOp::Shr) {
                let t = operand_type(m, p, a, None)?;
                let u = operand_type(m, p, b, Some(&Type::I32))?;
                if !u.is_integer() {
                    return Err(format!("shift count is a {u}"));
                }
                t
            } else {
                pair_type(m, p, a, b)?
            };
            let float = t.class() == TypeClass::Float;
            if !(t.is_integer() || (float && op.allows_float())) {
                return Err(format!("{} of a {t}", op.name()));
            }
            if *chk && !(op.checkable() && t.is_integer()) {
                return Err(format!("{}.chk is not an operation", op.name()));
            }
            Ok(())
        }
        Inst::Cmp { cond, a, b, .. } => {
            let t = pair_type(m, p, a, b)?;
            let ordered = t.is_integer() || t.class() == TypeClass::Float;
            let equality = matches!(cond, Cond::Eq | Cond::Ne);
            if !t.is_register() {
                return Err(format!("cmp of a {t}; use the typed compare"));
            }
            if !ordered && !equality {
                return Err(format!("a {t} compares for equality only"));
            }
            Ok(())
        }
        Inst::Select { c, .. } | Inst::BrIf { c, .. } => condition(m, p, c),
        Inst::Cvt { dst, src, chk } => {
            let t = reg_type(p, *dst)?;
            let u = operand_type(m, p, src, Some(&t))?;
            let numeric =
                |x: &Type| x.is_integer() || x.class() == TypeClass::Float || *x == Type::Ind;
            let from_storage = matches!(src, Operand::Place(_)) && u.is_numeric();
            if !(numeric(&t) && (numeric(&u) || from_storage)) {
                return Err(format!("cvt of a {u} to a {t}"));
            }
            if *chk && !t.is_integer() {
                return Err("cvt.chk only narrows to an integer".into());
            }
            Ok(())
        }
        Inst::Switch { v, cases, .. } => {
            let t = operand_type(m, p, v, None)?;
            if !t.is_integer() {
                return Err(format!("switch on a {t}"));
            }
            let mut seen = std::collections::HashSet::new();
            for (k, _) in cases {
                if !seen.insert(*k) {
                    return Err(format!("case {k} appears twice"));
                }
            }
            Ok(())
        }
        Inst::Ret(v) => match (&p.ret, v) {
            (None, None) => Ok(()),
            (None, Some(_)) => Err("the procedure returns nothing".into()),
            (Some(_), None) => Err("the procedure returns a value".into()),
            (Some(t), Some(v)) => {
                let u = operand_type(m, p, v, Some(t))?;
                same(t, &u)?;
                if !t.is_register() && matches!(v, Operand::Reg(_)) {
                    return Err(format!("a {t} is returned from a place"));
                }
                Ok(())
            }
        },
        Inst::Call { target, args, ret } => call(m, p, target, args, ret.as_ref()),
        Inst::Passed { param, .. } | Inst::Omitted { param, .. } => {
            if *param as usize >= p.params.len() {
                return Err(format!("parameter {param} does not exist"));
            }
            Ok(())
        }
        Inst::Debug(Debug::Var { place, .. }) => place_type(m, p, place).map(|_| ()),
        _ => super::ext::check(m, p, i),
    }
}

fn same(t: &Type, u: &Type) -> Result<(), String> {
    if t == u {
        Ok(())
    } else {
        Err(format!("a {u} where a {t} is needed"))
    }
}

fn condition(m: &Module, p: &Proc, c: &Operand) -> Result<(), String> {
    let t = operand_type(m, p, c, Some(&Type::Ind))?;
    if t.is_integer() || t == Type::Ind {
        Ok(())
    } else {
        Err(format!("condition is a {t}"))
    }
}

#[allow(clippy::too_many_lines)]
fn call(
    m: &Module,
    p: &Proc,
    target: &CallTarget,
    args: &[Arg],
    ret: Option<&CallRet>,
) -> Result<(), String> {
    let (params, ret_ty): (Option<&[Param]>, Option<Type>) = match target {
        CallTarget::Local(r) => {
            let callee = m
                .procs
                .get(r.0 as usize)
                .ok_or("procedure does not exist")?;
            (Some(&callee.params), callee.ret.clone())
        }
        CallTarget::Proto(r) | CallTarget::ProcPtr { proto: r, .. } => {
            let proto = m
                .protos
                .get(r.0 as usize)
                .ok_or("prototype does not exist")?;
            if let CallTarget::ProcPtr { ptr, .. } = target {
                pointer(p, *ptr, &Type::ProcPtr)?;
            }
            (Some(&proto.params), proto.ret.clone())
        }
        CallTarget::PgmName { name, lib } => {
            name_ok(name)?;
            if let Some(l) = lib {
                name_ok(l)?;
            }
            (None, None)
        }
        CallTarget::PgmVar(pl) => {
            let t = place_type(m, p, pl)?;
            if !matches!(t, Type::Char(10 | 20)) {
                return Err(format!(
                    "a program name is a char(10) or char(20), not a {t}"
                ));
            }
            (None, None)
        }
        CallTarget::SysPtr(r) => {
            pointer(p, *r, &Type::Ptr)?;
            (None, None)
        }
    };
    match (ret, &ret_ty) {
        (None, _) => {}
        (Some(_), None) => return Err("the target returns nothing".into()),
        (Some(CallRet::Reg(r)), Some(t)) => {
            if !t.is_register() {
                return Err(format!("a {t} is returned into a place"));
            }
            same(t, &reg_type(p, *r)?)?;
        }
        (Some(CallRet::Place(pl)), Some(t)) => {
            same(t, &place_type(m, p, pl)?)?;
        }
    }
    let Some(params) = params else {
        // Untyped program call: everything by reference.
        for a in args {
            match a {
                Arg::Omit => return Err("*OMIT needs a prototype".into()),
                Arg::Value(Operand::Place(_) | Operand::Const(_)) => {}
                Arg::Value(Operand::Reg(r)) => {
                    return Err(format!("%{} cannot be passed by reference", p.reg_name(*r)));
                }
            }
        }
        return Ok(());
    };
    if args.len() > params.len() {
        return Err(format!(
            "{} arguments for {} parameters",
            args.len(),
            params.len()
        ));
    }
    for (k, param) in params.iter().enumerate() {
        match args.get(k) {
            None => {
                if !param.nopass {
                    return Err(format!("parameter {} is not passed", param.name));
                }
            }
            Some(Arg::Omit) => {
                if !param.omit {
                    return Err(format!("parameter {} cannot be *OMIT", param.name));
                }
            }
            Some(Arg::Value(v)) => {
                let t = operand_type(m, p, v, Some(&param.ty))?;
                if param.by_value {
                    same(&param.ty, &t)?;
                } else {
                    if matches!(v, Operand::Reg(_)) {
                        return Err(format!(
                            "parameter {} is by reference and needs a place",
                            param.name
                        ));
                    }
                    if t != param.ty {
                        // A *VARSIZE `bytes` parameter is opaque storage of
                        // any type.
                        let opaque = param.varsize && matches!(param.ty, Type::Bytes(_));
                        let ok = param.varsize && t.class() == param.ty.class() || opaque;
                        if !ok {
                            return Err(format!(
                                "parameter {} is a {}, the argument a {t}",
                                param.name, param.ty
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn pointer(p: &Proc, r: Reg, want: &Type) -> Result<(), String> {
    let t = reg_type(p, r)?;
    if t == *want {
        Ok(())
    } else {
        Err(format!("%{} is a {t}, not a {want}", p.reg_name(r)))
    }
}

fn name_ok(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 10 {
        return Err(format!("{name:?} is not an object name"));
    }
    Ok(())
}
