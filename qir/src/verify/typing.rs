// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Operand and result types. The parser uses these to type registers
//! at their first definition; the verifier uses them to check every use.

use super::place::place_type;
use crate::inst::{BinOp, CallRet, CallTarget, Const, Inst, Operand, UnOp};
use crate::module::{Module, Proc, Reg};
use crate::types::{Type, TypeClass};

/// The type of an operand. A literal has no type of its own; `hint`
/// gives it one when the context does.
pub fn operand_type(
    module: &Module,
    proc: &Proc,
    op: &Operand,
    hint: Option<&Type>,
) -> Result<Type, String> {
    match op {
        Operand::Reg(r) => reg_type(proc, *r),
        Operand::Place(p) => place_type(module, proc, p),
        Operand::Const(c) => {
            let hint =
                hint.ok_or_else(|| "a literal needs a typed operand beside it".to_string())?;
            const_fits(c, hint)?;
            Ok(hint.clone())
        }
    }
}

/// The type of a register, or why it has none.
pub fn reg_type(proc: &Proc, r: Reg) -> Result<Type, String> {
    proc.reg_type(r)
        .cloned()
        .ok_or_else(|| format!("%{} is used before its type is known", proc.reg_name(r)))
}

/// Whether a literal can stand for a value of a type.
pub fn const_fits(c: &Const, ty: &Type) -> Result<(), String> {
    let ok = match c {
        Const::Int(_) => ty.is_numeric() || *ty == Type::Ind,
        Const::Float(_) => matches!(ty.class(), TypeClass::Float | TypeClass::Decimal),
        Const::Dec(_) => ty.is_numeric(),
        Const::Str(_) => ty.is_string() || ty.class() == TypeClass::DateTime || *ty == Type::Ind,
        Const::Hex(_) => !matches!(ty.class(), TypeClass::Ptr | TypeClass::ProcPtr),
        Const::Blanks => {
            ty.is_string()
                || ty.class() == TypeClass::DateTime
                || ty.class() == TypeClass::Aggregate
        }
        Const::Zeros => true,
        Const::HiVal | Const::LoVal => ty.is_numeric() || ty.is_string() || *ty == Type::Ind,
        Const::Null => matches!(ty, Type::Ptr | Type::ProcPtr),
        Const::On | Const::Off => *ty == Type::Ind,
    };
    if ok {
        Ok(())
    } else {
        Err(format!("{} is not a {ty}", describe_const(c)))
    }
}

/// A literal as the reader would recognise it.
#[must_use]
pub fn describe_const(c: &Const) -> String {
    match c {
        Const::Int(v) => v.to_string(),
        Const::Float(f) => format!("{f:e}"),
        Const::Dec(s) => s.clone(),
        Const::Str(s) => format!("\"{s}\""),
        Const::Hex(h) => format!("x\"{}\"", crate::text::lex::hex(h)),
        Const::Blanks => "*BLANKS".into(),
        Const::Zeros => "*ZEROS".into(),
        Const::HiVal => "*HIVAL".into(),
        Const::LoVal => "*LOVAL".into(),
        Const::Null => "*NULL".into(),
        Const::On => "*ON".into(),
        Const::Off => "*OFF".into(),
    }
}

/// The type of two operands that must agree, one of which may be a
/// literal taking the other's type.
pub fn pair_type(module: &Module, proc: &Proc, a: &Operand, b: &Operand) -> Result<Type, String> {
    match (a, b) {
        (Operand::Const(_), Operand::Const(_)) => {
            Err("two literals have no type; use const first".into())
        }
        (Operand::Const(_), _) => {
            let t = operand_type(module, proc, b, None)?;
            operand_type(module, proc, a, Some(&t))?;
            Ok(t)
        }
        _ => {
            let t = operand_type(module, proc, a, None)?;
            let u = operand_type(module, proc, b, Some(&t))?;
            if t != u {
                return Err(format!("operands are a {t} and a {u}"));
            }
            Ok(t)
        }
    }
}

/// The type the instruction's defined register gets, for instructions
/// whose result type follows from the operands. `None` when the
/// instruction defines nothing, or names its type itself (`const`,
/// `cvt`), in which case the register already carries it.
pub fn def_type(module: &Module, proc: &Proc, inst: &Inst) -> Result<Option<Type>, String> {
    Ok(Some(match inst {
        Inst::Load { src, .. } => {
            let t = place_type(module, proc, src)?;
            if !t.is_register() {
                return Err(format!("load of a {t}; only scalars load into registers"));
            }
            t
        }
        Inst::Bin { op, a, b, .. } => {
            if matches!(op, BinOp::Shl | BinOp::Shr) {
                let t = operand_type(module, proc, a, None)?;
                operand_type(module, proc, b, Some(&Type::I32))?;
                t
            } else {
                pair_type(module, proc, a, b)?
            }
        }
        Inst::Un { op, a, .. } => {
            let t = operand_type(module, proc, a, None)?;
            match op {
                UnOp::Not if !t.is_integer() => return Err(format!("not of a {t}")),
                UnOp::Neg | UnOp::Abs if !(t.is_integer() || t.class() == TypeClass::Float) => {
                    return Err(format!("{} of a {t}", op.name()));
                }
                _ => {}
            }
            t
        }
        Inst::Cmp { .. } | Inst::Passed { .. } | Inst::Omitted { .. } => Type::Ind,
        Inst::Select { a, b, .. } => pair_type(module, proc, a, b)?,
        Inst::Parms { .. } => Type::I32,
        Inst::Call {
            target,
            ret: Some(CallRet::Reg(_)),
            ..
        } => {
            let ret = call_ret_type(module, target)?;
            match ret {
                Some(t) if t.is_register() => t,
                Some(t) => return Err(format!("the call returns a {t}, which needs a place")),
                None => return Err("the call returns nothing".into()),
            }
        }
        _ => return Ok(None),
    }))
}

/// The declared return type of a call target.
pub fn call_ret_type(module: &Module, target: &CallTarget) -> Result<Option<Type>, String> {
    Ok(match target {
        CallTarget::Local(p) => module
            .procs
            .get(p.0 as usize)
            .ok_or("procedure does not exist")?
            .ret
            .clone(),
        CallTarget::Proto(p) | CallTarget::ProcPtr { proto: p, .. } => module
            .protos
            .get(p.0 as usize)
            .ok_or("prototype does not exist")?
            .ret
            .clone(),
        CallTarget::PgmName { .. } | CallTarget::PgmVar(_) | CallTarget::SysPtr(_) => None,
    })
}
