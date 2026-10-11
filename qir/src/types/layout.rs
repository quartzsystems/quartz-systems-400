// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Byte sizes and alignment. Only pointers are aligned.

use super::Type;

/// Size of a pointer.
pub const PTR_SIZE: u64 = 16;

/// Alignment of a pointer and of anything holding one.
pub const PTR_ALIGN: u32 = 16;

/// Size and alignment of a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Layout {
    /// Bytes.
    pub size: u64,
    /// 1 or 16.
    pub align: u32,
}

impl Type {
    /// Size in bytes.
    #[must_use]
    pub fn size(&self) -> u64 {
        match self {
            Self::I8 | Self::U8 | Self::Ind => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 | Self::Time(_) => 8,
            Self::Packed(s) => u64::from(s.digits) / 2 + 1,
            Self::Zoned(s) => u64::from(s.digits),
            Self::Char(n) | Self::Bytes(n) => u64::from(*n),
            Self::VarChar(n) => 2 + u64::from(*n),
            Self::Ucs2(n) | Self::Graphic(n) => 2 * u64::from(*n),
            Self::VarUcs2(n) => 2 + 2 * u64::from(*n),
            Self::Date(d) => u64::from(d.length()),
            Self::Timestamp(p) => {
                if *p == 0 {
                    19
                } else {
                    20 + u64::from(*p)
                }
            }
            Self::Ptr | Self::ProcPtr => PTR_SIZE,
            Self::Struct(s) => u64::from(s.size),
            Self::Array { len, elem } => u64::from(*len) * elem.size(),
        }
    }

    /// Alignment: 16 for anything holding a pointer, else 1.
    #[must_use]
    pub fn align(&self) -> u32 {
        if self.contains_ptr() { PTR_ALIGN } else { 1 }
    }

    /// Size and alignment together.
    #[must_use]
    pub fn layout(&self) -> Layout {
        Layout {
            size: self.size(),
            align: self.align(),
        }
    }
}
