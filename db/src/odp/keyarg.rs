// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The search argument of a positioning or random operation.

use crate::record::Value;

/// What `SETLL`, `SETGT`, `CHAIN`, `READE` and `READPE` search for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyArg {
    /// Key field values, the first `n` of the key (`%KDS` style).
    Values(Vec<Value>),
    /// An encoded key or key prefix.
    Bytes(Vec<u8>),
    /// A relative record number.
    Rrn(u64),
    /// `*START`.
    Start,
    /// `*END`.
    End,
    /// `*LOVAL`: below every key.
    LoVal,
    /// `*HIVAL`: above every key.
    HiVal,
}

impl KeyArg {
    /// One key field value.
    #[must_use]
    pub fn one(v: Value) -> Self {
        Self::Values(vec![v])
    }
}
