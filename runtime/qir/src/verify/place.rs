// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Resolving a place: its type and where it is relative to its base.

use crate::inst::{Index, Place, PlaceBase, Step};
use crate::module::{Module, Proc, Reg, Scope, VarRef};
use crate::types::Type;

/// A dynamic index term: the register, the element stride and the
/// element count for the bounds check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexTerm {
    /// The index register, zero-based.
    pub reg: Reg,
    /// Bytes per element.
    pub stride: u64,
    /// Elements; an index at or past it is `MCH0603`.
    pub len: u32,
}

/// A resolved place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access {
    /// The type at the place.
    pub ty: Type,
    /// The variable, when the base is one.
    pub var: Option<VarRef>,
    /// The pointer register and its offset, when the base is a deref.
    pub deref: Option<(Reg, i64)>,
    /// Constant byte offset from the base.
    pub offset: u64,
    /// Dynamic index terms added to `offset`.
    pub index: Vec<IndexTerm>,
}

/// Resolve a place against a module and the procedure it is in.
pub fn resolve(module: &Module, proc: &Proc, place: &Place) -> Result<Access, String> {
    let (mut ty, var, deref) = match &place.base {
        PlaceBase::Var(r) => {
            let v = module
                .var(*r, Some(proc))
                .ok_or_else(|| format!("variable {} does not exist", describe_ref(*r)))?;
            (v.ty.clone(), Some(*r), None)
        }
        PlaceBase::Deref { ptr, offset, ty } => {
            match proc.reg_type(*ptr) {
                Some(Type::Ptr) => {}
                Some(t) => return Err(format!("%{} is a {t}, not a ptr", proc.reg_name(*ptr))),
                None => return Err(format!("%{} is not defined", proc.reg_name(*ptr))),
            }
            (ty.clone(), None, Some((*ptr, *offset)))
        }
    };
    let mut offset = 0u64;
    let mut index = Vec::new();
    for step in &place.path {
        match step {
            Step::Field(name) => {
                let f = ty
                    .field(name)
                    .ok_or_else(|| format!("{ty} has no field {name}"))?;
                offset += u64::from(f.offset);
                ty = f.ty.clone();
            }
            Step::Index(ix) => {
                let (elem, len) = ty.array().ok_or_else(|| format!("{ty} is not an array"))?;
                let stride = ty.stride().unwrap_or(0);
                match ix {
                    Index::Const(i) => {
                        if *i >= len {
                            return Err(format!("index {i} is past the {len} elements"));
                        }
                        offset += u64::from(*i) * stride;
                    }
                    Index::Reg(r) => {
                        match proc.reg_type(*r) {
                            Some(t) if t.is_integer() => {}
                            Some(t) => {
                                return Err(format!(
                                    "index %{} is a {t}, not an integer",
                                    proc.reg_name(*r)
                                ));
                            }
                            None => {
                                return Err(format!("index %{} is not defined", proc.reg_name(*r)));
                            }
                        }
                        index.push(IndexTerm {
                            reg: *r,
                            stride,
                            len,
                        });
                    }
                }
                ty = elem.clone();
            }
        }
    }
    Ok(Access {
        ty,
        var,
        deref,
        offset,
        index,
    })
}

/// The type of a place.
pub fn place_type(module: &Module, proc: &Proc, place: &Place) -> Result<Type, String> {
    resolve(module, proc, place).map(|a| a.ty)
}

fn describe_ref(r: VarRef) -> String {
    match r.scope {
        Scope::Module => format!("module #{}", r.index),
        Scope::Proc => format!("proc #{}", r.index),
    }
}
