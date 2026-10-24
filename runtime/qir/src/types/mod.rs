// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IR types and their byte layouts. Every type has the size the
//! machine gives it; nothing is aligned except pointers, which sit on
//! 16-byte boundaries as on IBM i.

mod layout;

use std::fmt;

pub use dds::{DateFmt, TimeFmt};
pub use qdecimal::Shape;

pub use layout::{Layout, PTR_ALIGN, PTR_SIZE};

use crate::error::{QirError, Result};

/// Largest timestamp precision.
pub const MAX_TIMESTAMP_PRECISION: u8 = 12;

/// Largest `char`, `varchar`, `bytes` or `graphic` length.
pub const MAX_STRING_LEN: u32 = 16_773_104;

/// Largest length of a varying type with a 2-byte prefix.
pub const MAX_VAR2_LEN: u32 = 65_535;

/// A type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// Signed 8-bit integer.
    I8,
    /// Signed 16-bit integer, big-endian in storage.
    I16,
    /// Signed 32-bit integer, big-endian in storage.
    I32,
    /// Signed 64-bit integer, big-endian in storage.
    I64,
    /// Unsigned 8-bit integer.
    U8,
    /// Unsigned 16-bit integer, big-endian in storage.
    U16,
    /// Unsigned 32-bit integer, big-endian in storage.
    U32,
    /// Unsigned 64-bit integer, big-endian in storage.
    U64,
    /// IEEE 754 single, big-endian in storage.
    F32,
    /// IEEE 754 double, big-endian in storage.
    F64,
    /// Packed decimal, `digits / 2 + 1` bytes.
    Packed(Shape),
    /// Zoned decimal, one byte per digit.
    Zoned(Shape),
    /// Binary decimal: a big-endian two's complement integer of 2, 4 or
    /// 8 bytes (up to 4, 9 or 18 digits) with an implied scale.
    Bin(Shape),
    /// Fixed character, CP037 bytes, blank padded.
    Char(u32),
    /// Variable character: a big-endian length of 2 or 4 bytes (the
    /// second field) then `n` bytes.
    VarChar(u32, u8),
    /// Fixed UCS-2, `n` code units of 2 bytes.
    Ucs2(u32),
    /// Variable UCS-2: a 2- or 4-byte length (in code units) then `n`
    /// units.
    VarUcs2(u32, u8),
    /// Double-byte graphic, `n` characters of 2 bytes.
    Graphic(u32),
    /// Variable graphic: a 2- or 4-byte length (in characters) then
    /// `n` characters of 2 bytes.
    VarGraphic(u32, u8),
    /// A date in a format, as characters.
    Date(DateFmt),
    /// A time in a format, 8 characters.
    Time(TimeFmt),
    /// A timestamp with `p` fractional digits, `19` or `20 + p` characters.
    Timestamp(u8),
    /// An indicator, one byte holding `'0'` or `'1'`.
    Ind,
    /// A 16-byte space, system or data pointer.
    Ptr,
    /// A 16-byte procedure pointer.
    ProcPtr,
    /// A structure with explicit field offsets; fields may overlap.
    Struct(Box<StructType>),
    /// `len` elements of a type, contiguous or `stride` bytes apart
    /// (an RPG overlay of an array subfield: the gaps hold other fields).
    Array {
        /// Element count.
        len: u32,
        /// Element type.
        elem: Box<Type>,
        /// Bytes from one element to the next when not the element's
        /// size; at least the element's size.
        stride: Option<u32>,
    },
    /// `n` opaque bytes.
    Bytes(u32),
}

/// A structure type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructType {
    /// Total size; at least the end of the last field.
    pub size: u32,
    /// Fields in declaration order.
    pub fields: Vec<Field>,
}

/// A structure field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Field {
    /// Field name, unique within the structure.
    pub name: String,
    /// Byte offset from the start of the structure.
    pub offset: u32,
    /// Field type.
    pub ty: Type,
}

/// The class a type belongs to for operand checking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeClass {
    /// Signed integer.
    Int,
    /// Unsigned integer.
    UInt,
    /// Float.
    Float,
    /// Packed or zoned decimal.
    Decimal,
    /// Any character type: `char`, `varchar`.
    Char,
    /// `ucs2`, `varucs2`.
    Ucs2,
    /// `graphic`.
    Graphic,
    /// Date, time or timestamp.
    DateTime,
    /// Indicator.
    Ind,
    /// Space or system pointer.
    Ptr,
    /// Procedure pointer.
    ProcPtr,
    /// Structure, array or bytes.
    Aggregate,
}

impl Type {
    /// The class of the type.
    #[must_use]
    pub fn class(&self) -> TypeClass {
        match self {
            Self::I8 | Self::I16 | Self::I32 | Self::I64 => TypeClass::Int,
            Self::U8 | Self::U16 | Self::U32 | Self::U64 => TypeClass::UInt,
            Self::F32 | Self::F64 => TypeClass::Float,
            Self::Packed(_) | Self::Zoned(_) | Self::Bin(_) => TypeClass::Decimal,
            Self::Char(_) | Self::VarChar(..) => TypeClass::Char,
            Self::Ucs2(_) | Self::VarUcs2(..) => TypeClass::Ucs2,
            Self::Graphic(_) | Self::VarGraphic(..) => TypeClass::Graphic,
            Self::Date(_) | Self::Time(_) | Self::Timestamp(_) => TypeClass::DateTime,
            Self::Ind => TypeClass::Ind,
            Self::Ptr => TypeClass::Ptr,
            Self::ProcPtr => TypeClass::ProcPtr,
            Self::Struct(_) | Self::Array { .. } | Self::Bytes(_) => TypeClass::Aggregate,
        }
    }

    /// A type a register can hold: integers, floats, indicators and
    /// pointers. Everything else lives in storage only.
    #[must_use]
    pub fn is_register(&self) -> bool {
        matches!(
            self.class(),
            TypeClass::Int
                | TypeClass::UInt
                | TypeClass::Float
                | TypeClass::Ind
                | TypeClass::Ptr
                | TypeClass::ProcPtr
        )
    }

    /// Any integer type.
    #[must_use]
    pub fn is_integer(&self) -> bool {
        matches!(self.class(), TypeClass::Int | TypeClass::UInt)
    }

    /// A type decimal arithmetic accepts: decimals, integers and floats.
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self.class(),
            TypeClass::Int | TypeClass::UInt | TypeClass::Float | TypeClass::Decimal
        )
    }

    /// Any string type.
    #[must_use]
    pub fn is_string(&self) -> bool {
        matches!(
            self.class(),
            TypeClass::Char | TypeClass::Ucs2 | TypeClass::Graphic
        )
    }

    /// A variable-length string type.
    #[must_use]
    pub fn is_varying(&self) -> bool {
        self.var_prefix().is_some()
    }

    /// The length prefix of a varying type in bytes: 2 or 4.
    #[must_use]
    pub fn var_prefix(&self) -> Option<u32> {
        match self {
            Self::VarChar(_, p) | Self::VarUcs2(_, p) | Self::VarGraphic(_, p) => Some(u32::from(*p)),
            _ => None,
        }
    }

    /// Declared length of a string type in characters.
    #[must_use]
    pub fn string_len(&self) -> Option<u32> {
        match self {
            Self::Char(n)
            | Self::VarChar(n, _)
            | Self::Ucs2(n)
            | Self::VarUcs2(n, _)
            | Self::Graphic(n)
            | Self::VarGraphic(n, _) => Some(*n),
            _ => None,
        }
    }

    /// Bits of an integer type.
    #[must_use]
    pub fn int_bits(&self) -> Option<u8> {
        Some(match self {
            Self::I8 | Self::U8 => 8,
            Self::I16 | Self::U16 => 16,
            Self::I32 | Self::U32 => 32,
            Self::I64 | Self::U64 => 64,
            _ => return None,
        })
    }

    /// Whether the type, or anything inside it, is a pointer.
    #[must_use]
    pub fn contains_ptr(&self) -> bool {
        match self {
            Self::Ptr | Self::ProcPtr => true,
            Self::Struct(s) => s.fields.iter().any(|f| f.ty.contains_ptr()),
            Self::Array { elem, .. } => elem.contains_ptr(),
            _ => false,
        }
    }

    /// Decimal shape of a packed or zoned type.
    #[must_use]
    pub fn shape(&self) -> Option<Shape> {
        match self {
            Self::Packed(s) | Self::Zoned(s) | Self::Bin(s) => Some(*s),
            _ => None,
        }
    }

    /// Check that the type can be laid out: lengths in range, struct
    /// fields inside the structure and pointers aligned.
    pub fn check(&self) -> Result<()> {
        let bad = |msg: String| QirError::Type {
            ty: self.to_string(),
            msg,
        };
        match self {
            Self::Packed(s) | Self::Zoned(s) => {
                Shape::new(s.digits, s.scale).map_err(|e| bad(e.to_string()))?;
            }
            Self::Bin(s) => {
                Shape::new(s.digits, s.scale).map_err(|e| bad(e.to_string()))?;
                if s.digits > 18 {
                    return Err(bad("a binary decimal has at most 18 digits".into()));
                }
            }
            Self::Char(n)
            | Self::VarChar(n, _)
            | Self::Ucs2(n)
            | Self::VarUcs2(n, _)
            | Self::Graphic(n)
            | Self::VarGraphic(n, _)
            | Self::Bytes(n) => {
                if *n == 0 || *n > MAX_STRING_LEN {
                    return Err(bad(format!("length must be 1 to {MAX_STRING_LEN}")));
                }
                match self.var_prefix() {
                    Some(2) if *n > MAX_VAR2_LEN => {
                        return Err(bad(format!(
                            "a varying length with a 2-byte prefix is at most {MAX_VAR2_LEN}"
                        )));
                    }
                    Some(2 | 4) | None => {}
                    Some(_) => return Err(bad("a varying prefix is 2 or 4 bytes".into())),
                }
            }
            Self::Timestamp(p) if *p > MAX_TIMESTAMP_PRECISION => {
                return Err(bad(format!(
                    "timestamp precision is at most {MAX_TIMESTAMP_PRECISION}"
                )));
            }
            Self::Array { len, elem, stride } => {
                if *len == 0 {
                    return Err(bad("an array has at least one element".into()));
                }
                elem.check()?;
                if let Some(s) = stride {
                    if u64::from(*s) < elem.size() {
                        return Err(bad("the stride is less than the element's size".into()));
                    }
                    if elem.align() == PTR_ALIGN && s % PTR_ALIGN != 0 {
                        return Err(bad("an element holding a pointer needs a stride of a multiple of 16".into()));
                    }
                }
                if self.size() > u64::from(u32::MAX) {
                    return Err(bad("array is larger than 4 GiB".into()));
                }
            }
            Self::Struct(s) => {
                let mut seen = std::collections::HashSet::new();
                for f in &s.fields {
                    if !seen.insert(f.name.as_str()) {
                        return Err(bad(format!("field {} is declared twice", f.name)));
                    }
                    f.ty.check()?;
                    let end = u64::from(f.offset) + f.ty.size();
                    if end > u64::from(s.size) {
                        return Err(bad(format!(
                            "field {} ends at {end}, past the size {}",
                            f.name, s.size
                        )));
                    }
                    if f.ty.align() == PTR_ALIGN && f.offset % PTR_ALIGN != 0 {
                        return Err(bad(format!(
                            "field {} holds a pointer and is not 16-byte aligned",
                            f.name
                        )));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The field of a structure type.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&Field> {
        match self {
            Self::Struct(s) => s.fields.iter().find(|f| f.name == name),
            _ => None,
        }
    }

    /// Bytes from one element of an array type to the next.
    #[must_use]
    pub fn stride(&self) -> Option<u64> {
        match self {
            Self::Array { elem, stride, .. } => Some(stride.map_or_else(|| elem.size(), u64::from)),
            _ => None,
        }
    }

    /// Element type and count of an array type.
    #[must_use]
    pub fn array(&self) -> Option<(&Type, u32)> {
        match self {
            Self::Array { len, elem, .. } => Some((elem, *len)),
            _ => None,
        }
    }
}

impl StructType {
    /// Build a structure from fields; fields without an offset follow
    /// the previous one, pointers aligned to 16; the size is the end of
    /// the last field unless given.
    pub fn new(fields: Vec<(String, Option<u32>, Type)>, size: Option<u32>) -> Result<Self> {
        let mut next = 0u32;
        let mut end = 0u32;
        let mut out = Vec::with_capacity(fields.len());
        for (name, offset, ty) in fields {
            let offset = offset.unwrap_or_else(|| {
                if ty.align() == PTR_ALIGN {
                    next.div_ceil(PTR_ALIGN) * PTR_ALIGN
                } else {
                    next
                }
            });
            let size = u32::try_from(ty.size()).map_err(|_| QirError::Type {
                ty: ty.to_string(),
                msg: "larger than 4 GiB".into(),
            })?;
            next = offset.saturating_add(size);
            end = end.max(next);
            out.push(Field { name, offset, ty });
        }
        let size = size.unwrap_or(end);
        let s = Self { size, fields: out };
        let ty = Type::Struct(Box::new(s));
        ty.check()?;
        match ty {
            Type::Struct(s) => Ok(*s),
            _ => unreachable!(),
        }
    }
}

/// The text of a varying prefix: nothing for the 2-byte default.
fn pfx(p: u8) -> String {
    if p == 2 { String::new() } else { format!(":{p}") }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I8 => f.write_str("i8"),
            Self::I16 => f.write_str("i16"),
            Self::I32 => f.write_str("i32"),
            Self::I64 => f.write_str("i64"),
            Self::U8 => f.write_str("u8"),
            Self::U16 => f.write_str("u16"),
            Self::U32 => f.write_str("u32"),
            Self::U64 => f.write_str("u64"),
            Self::F32 => f.write_str("f32"),
            Self::F64 => f.write_str("f64"),
            Self::Packed(s) => write!(f, "packed({},{})", s.digits, s.scale),
            Self::Zoned(s) => write!(f, "zoned({},{})", s.digits, s.scale),
            Self::Bin(s) => write!(f, "bin({},{})", s.digits, s.scale),
            Self::Char(n) => write!(f, "char({n})"),
            Self::VarChar(n, p) => write!(f, "varchar({n}{})", pfx(*p)),
            Self::Ucs2(n) => write!(f, "ucs2({n})"),
            Self::VarUcs2(n, p) => write!(f, "varucs2({n}{})", pfx(*p)),
            Self::Graphic(n) => write!(f, "graphic({n})"),
            Self::VarGraphic(n, p) => write!(f, "vargraphic({n}{})", pfx(*p)),
            Self::Date(d) => write!(f, "date({})", d.name()),
            Self::Time(t) => write!(f, "time({})", t.name()),
            Self::Timestamp(p) => write!(f, "timestamp({p})"),
            Self::Ind => f.write_str("ind"),
            Self::Ptr => f.write_str("ptr"),
            Self::ProcPtr => f.write_str("procptr"),
            Self::Bytes(n) => write!(f, "bytes({n})"),
            Self::Array { len, elem, stride: None } => write!(f, "array[{len}] of {elem}"),
            Self::Array { len, elem, stride: Some(s) } => write!(f, "array[{len}:{s}] of {elem}"),
            Self::Struct(s) => {
                write!(f, "struct({}) {{", s.size)?;
                for (i, fl) in s.fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, " {}: {} @{}", fl.name, fl.ty, fl.offset)?;
                }
                f.write_str(" }")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(d: u8, s: u8) -> Shape {
        Shape::new(d, s).unwrap()
    }

    #[test]
    fn sizes_follow_the_machine() {
        assert_eq!(Type::Packed(shape(15, 5)).size(), 8);
        assert_eq!(Type::Packed(shape(16, 0)).size(), 9);
        assert_eq!(Type::Zoned(shape(7, 2)).size(), 7);
        assert_eq!(Type::Bin(shape(4, 2)).size(), 2);
        assert_eq!(Type::Bin(shape(9, 2)).size(), 4);
        assert_eq!(Type::Bin(shape(18, 0)).size(), 8);
        assert_eq!(Type::VarChar(10, 2).size(), 12);
        assert_eq!(Type::VarChar(10, 4).size(), 14);
        assert_eq!(Type::Ucs2(5).size(), 10);
        assert_eq!(Type::VarUcs2(5, 2).size(), 12);
        assert_eq!(Type::VarGraphic(5, 4).size(), 14);
        assert_eq!(Type::Date(DateFmt::Iso).size(), 10);
        assert_eq!(Type::Date(DateFmt::Jul).size(), 6);
        assert_eq!(Type::Time(TimeFmt::Usa).size(), 8);
        assert_eq!(Type::Timestamp(6).size(), 26);
        assert_eq!(Type::Timestamp(0).size(), 19);
        assert_eq!(Type::Ptr.size(), 16);
        assert_eq!(Type::Ptr.align(), 16);
        assert_eq!(Type::Char(3).align(), 1);
    }

    #[test]
    fn struct_layout_aligns_pointers() {
        let s = StructType::new(
            vec![
                ("a".into(), None, Type::Char(3)),
                ("p".into(), None, Type::Ptr),
                ("ov".into(), Some(0), Type::Char(2)),
            ],
            None,
        )
        .unwrap();
        assert_eq!(s.fields[1].offset, 16);
        assert_eq!(s.size, 32);
        let t = Type::Struct(Box::new(s));
        assert_eq!(t.align(), 16);
        assert!(t.contains_ptr());
        assert_eq!(t.size(), 32);
    }

    #[test]
    fn misaligned_pointer_is_rejected() {
        let err = StructType::new(
            vec![
                ("a".into(), None, Type::Char(3)),
                ("p".into(), Some(3), Type::Ptr),
            ],
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("aligned"));
    }

    #[test]
    fn display_round_trips_names() {
        let t = Type::Array {
            len: 3,
            elem: Box::new(Type::Packed(shape(5, 2))),
            stride: None,
        };
        assert_eq!(t.to_string(), "array[3] of packed(5,2)");
        let s = Type::Array {
            len: 3,
            elem: Box::new(Type::Char(3)),
            stride: Some(10),
        };
        assert_eq!(s.to_string(), "array[3:10] of char(3)");
        assert_eq!(s.size(), 23);
        assert_eq!(s.stride(), Some(10));
    }
}
