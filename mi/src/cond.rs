// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Machine conditions as exceptions: the message identifier, the RPG
//! status the language runtime reports for it, and the class a
//! `monitor` selector tests.

pub use codegen::Cond;

/// A raised condition, pending until a handler takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exception {
    /// The condition.
    pub cond: Cond,
    /// `MCHnnnn`.
    pub msgid: String,
    /// The five-digit RPG status the condition maps to.
    pub status: u16,
    /// Call-stack depth where it was raised.
    pub depth: u32,
}

impl Exception {
    /// An exception for a condition raised at a depth.
    #[must_use]
    pub fn new(cond: Cond, depth: u32) -> Self {
        Self {
            cond,
            msgid: cond.msgid(),
            status: status_of(cond),
            depth,
        }
    }

    /// Whether a file operation raised it; none of the machine
    /// conditions do.
    #[must_use]
    pub fn is_file_error(&self) -> bool {
        false
    }
}

/// The RPG status code of a machine condition, as the RPG runtime maps
/// it (`*STATUS` and the `ON-ERROR` ranges).
#[must_use]
pub fn status_of(c: Cond) -> u16 {
    match c {
        Cond::Mch1211 => 102,
        Cond::Mch1210 => 103,
        Cond::Mch0603 => 121,
        Cond::Mch1202 => 907,
        Cond::Mch1602 => 112,
        Cond::Mch0601
        | Cond::Mch0602
        | Cond::Mch3401
        | Cond::Mch3402
        | Cond::Mch3601
        | Cond::Mch3602 => 222,
    }
}
