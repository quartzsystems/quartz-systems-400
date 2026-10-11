// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The runtime call table: the functions generated code calls, by
//! index. The index is what a relocation names; the runtime fills a
//! table of addresses in the same order. Every function takes the
//! environment first and uses the System V convention.

use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;

/// A runtime function. The discriminant is its table index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum RtFn {
    /// `raise(env, cond)`: a machine condition occurred at the caller.
    /// Returns when the condition was handled and execution resumes;
    /// otherwise sets the job's pending flag and returns.
    Raise = 0,
    /// `enter(env, code)`: a procedure at code address `code` begins.
    Enter = 1,
    /// `leave(env)`: the procedure ends, normally or by percolation.
    Leave = 2,
    /// `deref(env, lo, hi) -> addr`: the native address a fat pointer
    /// (its two halves) addresses, or a raised condition and 0.
    Deref = 3,
    /// `copy(dst, src, len)`: bytes between non-overlapping storage.
    Copy = 4,
    /// `mov_char(dst, dst_len, src, src_len)`: fixed text, blank padded.
    MovChar = 5,
}

/// Every runtime function, in table order.
pub const RT_FNS: [RtFn; 6] = [
    RtFn::Raise,
    RtFn::Enter,
    RtFn::Leave,
    RtFn::Deref,
    RtFn::Copy,
    RtFn::MovChar,
];

impl RtFn {
    /// The table index.
    #[must_use]
    pub fn index(self) -> u32 {
        self as u32
    }

    /// From a table index.
    #[must_use]
    pub fn from_index(i: u32) -> Option<Self> {
        RT_FNS.get(i as usize).copied()
    }

    /// The name, for listings.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Raise => "raise",
            Self::Enter => "enter",
            Self::Leave => "leave",
            Self::Deref => "deref",
            Self::Copy => "copy",
            Self::MovChar => "mov_char",
        }
    }

    /// The Cranelift signature.
    #[must_use]
    pub fn signature(self) -> Signature {
        let mut s = Signature::new(CallConv::SystemV);
        let i64p = AbiParam::new(types::I64);
        match self {
            Self::Raise => {
                s.params.push(i64p);
                s.params.push(AbiParam::new(types::I32));
            }
            Self::Enter => {
                s.params.push(i64p);
                s.params.push(i64p);
            }
            Self::Leave => s.params.push(i64p),
            Self::Deref => {
                s.params.push(i64p);
                s.params.push(i64p);
                s.params.push(i64p);
                s.returns.push(i64p);
            }
            Self::Copy => {
                s.params.push(i64p);
                s.params.push(i64p);
                s.params.push(i64p);
            }
            Self::MovChar => {
                s.params.push(i64p);
                s.params.push(i64p);
                s.params.push(i64p);
                s.params.push(i64p);
            }
        }
        s
    }
}

/// A machine condition generated code raises, as the number of its
/// `MCHnnnn` message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Cond {
    /// `MCH0601`: pointer outside its space.
    Mch0601 = 601,
    /// `MCH0603`: index out of range.
    Mch0603 = 603,
    /// `MCH1210`: size error, a value does not fit.
    Mch1210 = 1210,
    /// `MCH1211`: division by zero.
    Mch1211 = 1211,
    /// `MCH3601`: null pointer.
    Mch3601 = 3601,
}

impl Cond {
    /// The message identifier.
    #[must_use]
    pub fn msgid(self) -> String {
        format!("MCH{:04}", self as u32)
    }

    /// From the number generated code passed.
    #[must_use]
    pub fn from_code(code: u32) -> Option<Self> {
        Some(match code {
            601 => Self::Mch0601,
            603 => Self::Mch0603,
            1210 => Self::Mch1210,
            1211 => Self::Mch1211,
            3601 => Self::Mch3601,
            _ => return None,
        })
    }
}
