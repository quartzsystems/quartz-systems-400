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
use qir::{DateFmt, Module, Param, Scope, StorageClass, TimeFmt, Type};

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
    /// The `monitor` region the running code is in, as the module's
    /// region table numbers it (0 outside every region); generated code
    /// stores it before every call so the runtime can find the handlers
    /// of a suspended entry.
    pub cur_region: u32,
    /// The statement the running code is at (`debug.stmt`), for the
    /// call stack display and the inquiry text.
    pub cur_stmt: u32,
}

/// Offset of `Env::statics`.
pub const ENV_STATICS: i32 = 0;
/// Offset of `Env::job`.
pub const ENV_JOB: i32 = 8;
/// Offset of `Env::entry`.
pub const ENV_ENTRY: i32 = 16;
/// Offset of `Job::pending`.
pub const JOB_PENDING: i32 = 0;
/// Offset of `Job::cur_region`.
pub const JOB_REGION: i32 = 8;
/// Offset of `Job::cur_stmt`.
pub const JOB_STMT: i32 = 12;

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

/// An operational descriptor list, what `opdesc` addresses: a native
/// `i32` entry count (one per declared parameter, or per argument when
/// more are passed), 4 reserved bytes, then the entries.
pub const OPDESC_HEADER: u32 = 8;
/// One descriptor: five native `i32`s (`desc_type`, `datatype`,
/// `info1`, `info2`, `length`) and 4 reserved bytes.
pub const OPDESC_ENTRY: u32 = 24;

/// The values a descriptor's words take (a QS/400 reading of the
/// `CEEDOD` codes).
pub mod opdesc {
    /// `desc_type`: the argument was omitted or not passed.
    pub const NONE: i32 = 0;
    /// `desc_type`: one element; `length` is its size.
    pub const ELEMENT: i32 = 2;
    /// `desc_type`: an array; `info1` is the element count, `length`
    /// one element's size.
    pub const ARRAY: i32 = 3;
    /// `datatype`: a structure, pointer or other untyped storage.
    pub const UNTYPED: i32 = 0;
    /// `datatype`: an integer, float, packed or zoned number.
    pub const NUMERIC: i32 = 1;
    /// `datatype`: fixed characters.
    pub const CHAR: i32 = 2;
    /// `datatype`: varying characters with a 2-byte length;
    /// `length` is the maximum.
    pub const VARCHAR: i32 = 3;
    /// `datatype`: fixed UCS-2.
    pub const UCS2: i32 = 4;
    /// `datatype`: varying UCS-2.
    pub const VARUCS2: i32 = 5;
    /// `datatype`: graphic.
    pub const GRAPHIC: i32 = 6;
    /// `datatype`: a date, time or timestamp.
    pub const DATETIME: i32 = 7;
}

/// The descriptor words of an argument passed as `ty`: `desc_type`,
/// `datatype`, `info1`, `info2`, `length`.
#[must_use]
pub fn describe(ty: &Type) -> [i32; 5] {
    let len = |t: &Type| i32::try_from(t.size()).unwrap_or(i32::MAX);
    if let Some((elem, n)) = ty.array() {
        let n = i32::try_from(n).unwrap_or(i32::MAX);
        return [opdesc::ARRAY, datatype(elem), n, 0, len(elem)];
    }
    [opdesc::ELEMENT, datatype(ty), 0, 0, len(ty)]
}

fn datatype(ty: &Type) -> i32 {
    use qir::TypeClass;
    match ty {
        Type::Char(_) => opdesc::CHAR,
        Type::VarChar(_) => opdesc::VARCHAR,
        Type::Ucs2(_) => opdesc::UCS2,
        Type::VarUcs2(_) => opdesc::VARUCS2,
        Type::Graphic(_) => opdesc::GRAPHIC,
        Type::Date(_) | Type::Time(_) | Type::Timestamp(_) => opdesc::DATETIME,
        t if matches!(
            t.class(),
            TypeClass::Int | TypeClass::UInt | TypeClass::Float | TypeClass::Decimal
        ) =>
        {
            opdesc::NUMERIC
        }
        _ => opdesc::UNTYPED,
    }
}

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
/// addresses; by-value register types travel as themselves. A program
/// entry procedure instead takes `argv`, the address of an array of
/// `argc` parameter addresses, so a dynamic call reaches every program
/// the same way.
#[must_use]
pub fn signature(params: &[Param], ret: Option<&Type>, entry: bool) -> Signature {
    let mut s = Signature::new(CallConv::SystemV);
    let i64p = AbiParam::new(types::I64);
    s.params.push(i64p); // env
    s.params.push(i64p); // argc
    s.params.push(i64p); // opdesc
    if entry {
        s.params.push(i64p); // argv
        return s;
    }
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

/// Date formats in descriptor order.
pub const DATE_FMTS: [DateFmt; 8] = [
    DateFmt::Iso,
    DateFmt::Usa,
    DateFmt::Eur,
    DateFmt::Jis,
    DateFmt::Mdy,
    DateFmt::Dmy,
    DateFmt::Ymd,
    DateFmt::Jul,
];

/// Time formats in descriptor order.
pub const TIME_FMTS: [TimeFmt; 5] = [
    TimeFmt::Hms,
    TimeFmt::Iso,
    TimeFmt::Usa,
    TimeFmt::Eur,
    TimeFmt::Jis,
];

/// The index of a date format.
#[must_use]
pub fn date_fmt_index(f: DateFmt) -> u8 {
    u8::try_from(DATE_FMTS.iter().position(|x| *x == f).expect("known")).expect("small")
}

/// The index of a time format.
#[must_use]
pub fn time_fmt_index(f: TimeFmt) -> u8 {
    u8::try_from(TIME_FMTS.iter().position(|x| *x == f).expect("known")).expect("small")
}

/// A type as the 64-bit word runtime calls receive beside a storage
/// address: bits 0-7 the kind, 8-39 the length or digits, 40-47 the
/// decimal positions or timestamp precision, 48-55 the date or time
/// format index, bit 56 [`TypeDesc::FSIGN`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeDesc(pub u64);

impl TypeDesc {
    /// Packed and zoned values written through this descriptor take
    /// the positive sign `F` instead of `C`, as RPG writes them.
    pub const FSIGN: u64 = 1 << 56;

    /// The descriptor with [`Self::FSIGN`] set when `on`.
    #[must_use]
    pub fn fsign(self, on: bool) -> Self {
        if on { Self(self.0 | Self::FSIGN) } else { self }
    }

    /// Encode a type. Structures, arrays and bytes encode as `bytes(n)`.
    #[must_use]
    pub fn of(ty: &Type) -> Self {
        let (kind, len, scale, fmt): (u64, u64, u64, u64) = match ty {
            Type::I8 => (0, 1, 0, 0),
            Type::I16 => (1, 2, 0, 0),
            Type::I32 => (2, 4, 0, 0),
            Type::I64 => (3, 8, 0, 0),
            Type::U8 => (4, 1, 0, 0),
            Type::U16 => (5, 2, 0, 0),
            Type::U32 => (6, 4, 0, 0),
            Type::U64 => (7, 8, 0, 0),
            Type::F32 => (8, 4, 0, 0),
            Type::F64 => (9, 8, 0, 0),
            Type::Packed(s) => (10, u64::from(s.digits), u64::from(s.scale), 0),
            Type::Zoned(s) => (11, u64::from(s.digits), u64::from(s.scale), 0),
            Type::Char(n) => (12, u64::from(*n), 0, 0),
            Type::VarChar(n) => (13, u64::from(*n), 0, 0),
            Type::Ucs2(n) => (14, u64::from(*n), 0, 0),
            Type::VarUcs2(n) => (15, u64::from(*n), 0, 0),
            Type::Graphic(n) => (16, u64::from(*n), 0, 0),
            Type::Date(f) => (17, u64::from(f.length()), 0, u64::from(date_fmt_index(*f))),
            Type::Time(f) => (18, 8, 0, u64::from(time_fmt_index(*f))),
            Type::Timestamp(p) => (19, ty.size(), u64::from(*p), 0),
            Type::Ind => (20, 1, 0, 0),
            Type::Ptr => (21, 16, 0, 0),
            Type::ProcPtr => (22, 16, 0, 0),
            Type::Struct(_) | Type::Array { .. } | Type::Bytes(_) => (23, ty.size(), 0, 0),
        };
        Self(kind | (len << 8) | (scale << 40) | (fmt << 48))
    }

    /// Decode a descriptor.
    #[must_use]
    pub fn decode(self) -> Option<Type> {
        let v = self.0;
        let kind = v & 0xFF;
        let len = u32::try_from((v >> 8) & 0xFFFF_FFFF).ok()?;
        let scale = u8::try_from((v >> 40) & 0xFF).ok()?;
        let fmt = usize::try_from((v >> 48) & 0xFF).ok()?;
        let shape = |d: u32, s: u8| qir::Shape::new(u8::try_from(d).ok()?, s).ok();
        Some(match kind {
            0 => Type::I8,
            1 => Type::I16,
            2 => Type::I32,
            3 => Type::I64,
            4 => Type::U8,
            5 => Type::U16,
            6 => Type::U32,
            7 => Type::U64,
            8 => Type::F32,
            9 => Type::F64,
            10 => Type::Packed(shape(len, scale)?),
            11 => Type::Zoned(shape(len, scale)?),
            12 => Type::Char(len),
            13 => Type::VarChar(len),
            14 => Type::Ucs2(len),
            15 => Type::VarUcs2(len),
            16 => Type::Graphic(len),
            17 => Type::Date(*DATE_FMTS.get(fmt)?),
            18 => Type::Time(*TIME_FMTS.get(fmt)?),
            19 => Type::Timestamp(scale),
            20 => Type::Ind,
            21 => Type::Ptr,
            22 => Type::ProcPtr,
            23 => Type::Bytes(len),
            _ => return None,
        })
    }

    /// The descriptor as the `i64` generated code passes.
    #[must_use]
    pub fn word(self) -> i64 {
        i64::from_ne_bytes(self.0.to_ne_bytes())
    }

    /// From the `i64` a runtime function received.
    #[must_use]
    pub fn from_word(w: i64) -> Self {
        Self(u64::from_ne_bytes(w.to_ne_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_descriptors_round_trip() {
        for t in [
            Type::I8,
            Type::U64,
            Type::F32,
            Type::Packed(qir::Shape::new(15, 5).unwrap()),
            Type::Zoned(qir::Shape::new(63, 63).unwrap()),
            Type::Char(16_773_104),
            Type::VarUcs2(10),
            Type::Date(DateFmt::Jul),
            Type::Time(TimeFmt::Usa),
            Type::Timestamp(12),
            Type::Ind,
            Type::Ptr,
            Type::Bytes(7),
        ] {
            assert_eq!(TypeDesc::of(&t).decode(), Some(t.clone()), "{t}");
            assert_eq!(
                TypeDesc::from_word(TypeDesc::of(&t).word()),
                TypeDesc::of(&t)
            );
        }
        let s = Type::Struct(Box::new(
            qir::StructType::new(vec![("a".into(), None, Type::Char(3))], None).unwrap(),
        ));
        assert_eq!(TypeDesc::of(&s).decode(), Some(Type::Bytes(3)));
    }
}
