// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The calling convention generated code follows: the environment
//! record, the hidden parameters, how each parameter kind is passed,
//! and the static frame layout. `docs/ABI.md` is the specification.

use std::collections::HashMap;

use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;
use qir::module::index32;
use qir::{Module, Param, Scope, StorageClass, Type};

/// The environment record every generated procedure receives in its
/// first parameter. The runtime owns it; generated code reads the
/// fields at these offsets.
#[repr(C)]
#[derive(Debug)]
pub struct Env {
    /// Base of the program's static area in the current activation
    /// group; a module's frame sits at its bind-time offset inside it.
    pub statics: *mut u8,
    /// The job record generated code polls for a pending exception.
    pub job: *mut Job,
    /// The current call-stack entry; runtime-owned, opaque here.
    pub entry: *mut u8,
}

/// The job record as generated code sees it.
#[repr(C)]
#[derive(Debug, Default)]
pub struct Job {
    /// Nonzero while an exception is percolating; set by the runtime's
    /// `raise`, cleared when a handler takes it.
    pub pending: u64,
}

/// Offset of `Env::statics`.
pub const ENV_STATICS: i32 = 0;
/// Offset of `Env::job`.
pub const ENV_JOB: i32 = 8;
/// Offset of `Env::entry`.
pub const ENV_ENTRY: i32 = 16;
/// Offset of `Job::pending`.
pub const JOB_PENDING: i32 = 0;

/// A byte size as a host size.
#[must_use]
pub fn usz(v: u64) -> usize {
    usize::try_from(v).expect("fits in memory")
}

/// A byte offset as a 32-bit memory offset.
#[must_use]
pub fn off32(v: u64) -> i32 {
    i32::try_from(v).expect("offset fits in 2 GiB")
}

/// Hidden parameters before the declared ones: `env`, `argc`, `opdesc`,
/// then `retbuf` when the procedure returns a storage type.
pub const HIDDEN_PARAMS: usize = 3;

/// The Cranelift type a register type lowers to.
#[must_use]
pub fn reg_type(ty: &Type) -> types::Type {
    match ty {
        Type::I8 | Type::U8 | Type::Ind => types::I8,
        Type::I16 | Type::U16 => types::I16,
        Type::I32 | Type::U32 => types::I32,
        Type::I64 | Type::U64 => types::I64,
        Type::F32 => types::F32,
        Type::F64 => types::F64,
        Type::Ptr | Type::ProcPtr => types::I128,
        _ => unreachable!("{ty} is not a register type"),
    }
}

/// Whether a return type travels in a register (`true`) or through
/// the hidden `retbuf` pointer.
#[must_use]
pub fn returns_in_register(ret: Option<&Type>) -> bool {
    ret.is_none_or(Type::is_register)
}

/// The signature of a procedure with these parameters and return type.
/// By-reference parameters and by-value storage types are `i64`
/// addresses; by-value register types travel as themselves.
#[must_use]
pub fn signature(params: &[Param], ret: Option<&Type>) -> Signature {
    let mut s = Signature::new(CallConv::SystemV);
    let i64p = AbiParam::new(types::I64);
    s.params.push(i64p); // env
    s.params.push(i64p); // argc
    s.params.push(i64p); // opdesc
    if !returns_in_register(ret) {
        s.params.push(i64p); // retbuf
    }
    for p in params {
        if p.by_value && p.ty.is_register() {
            s.params.push(AbiParam::new(reg_type(&p.ty)));
        } else {
            s.params.push(i64p);
        }
    }
    if let Some(r) = ret
        && r.is_register()
    {
        s.returns.push(AbiParam::new(reg_type(r)));
    }
    s
}

/// A variable's slot in the module's static frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticSlot {
    /// Offset from the frame start.
    pub offset: u64,
    /// Bytes.
    pub size: u64,
}

/// The module's static frame: every `static`, `const` and procedure
/// `static` variable, pointers aligned to 16.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StaticLayout {
    /// Slots by (scope, procedure index or 0, variable index).
    pub slots: HashMap<(Scope, u32, u32), StaticSlot>,
    /// Total bytes; a multiple of 16.
    pub size: u64,
}

impl StaticLayout {
    /// Lay out a module's static frame.
    #[must_use]
    pub fn of(m: &Module) -> Self {
        let mut out = Self::default();
        let mut at = 0u64;
        let mut place = |key: (Scope, u32, u32), ty: &Type| {
            if ty.align() > 1 {
                at = at.div_ceil(16) * 16;
            }
            let size = ty.size();
            out.slots.insert(key, StaticSlot { offset: at, size });
            at += size;
        };
        for (i, v) in m.vars.iter().enumerate() {
            if matches!(v.class, StorageClass::Static | StorageClass::Const) {
                place((Scope::Module, 0, index32(i)), &v.ty);
            }
        }
        for (pi, p) in m.procs.iter().enumerate() {
            for (i, v) in p.vars.iter().enumerate() {
                if matches!(v.class, StorageClass::Static | StorageClass::Const) {
                    place((Scope::Proc, index32(pi), index32(i)), &v.ty);
                }
            }
        }
        out.size = at.div_ceil(16) * 16;
        out
    }

    /// The slot of a module-scope variable.
    #[must_use]
    pub fn module_var(&self, index: u32) -> Option<StaticSlot> {
        self.slots.get(&(Scope::Module, 0, index)).copied()
    }

    /// The slot of a procedure-scope variable.
    #[must_use]
    pub fn proc_var(&self, proc: u32, index: u32) -> Option<StaticSlot> {
        self.slots.get(&(Scope::Proc, proc, index)).copied()
    }

    /// The initial bytes of the whole frame: initializers where given,
    /// the types' defaults otherwise.
    #[must_use]
    pub fn initial_bytes(&self, m: &Module) -> Vec<u8> {
        let mut out = vec![0u8; usize::try_from(self.size).expect("frame fits")];
        let mut fill = |slot: StaticSlot, v: &qir::Var| {
            let bytes = v
                .inz
                .clone()
                .unwrap_or_else(|| qir::inst::literal::default_bytes(&v.ty, usz(slot.size), false));
            let at = usz(slot.offset);
            out[at..at + bytes.len()].copy_from_slice(&bytes);
        };
        for (i, v) in m.vars.iter().enumerate() {
            if let Some(slot) = self.module_var(index32(i)) {
                fill(slot, v);
            }
        }
        for (pi, p) in m.procs.iter().enumerate() {
            for (i, v) in p.vars.iter().enumerate() {
                if let Some(slot) = self.proc_var(index32(pi), index32(i)) {
                    fill(slot, v);
                }
            }
        }
        out
    }
}
