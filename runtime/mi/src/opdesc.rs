// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Operational descriptors: reading the list a caller built for a
//! procedure declared `opdesc` (`codegen::abi` gives the layout).

use codegen::abi::{OPDESC_ENTRY, OPDESC_HEADER};

/// One argument's descriptor, as `CEEDOD` reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpDesc {
    /// `codegen::abi::opdesc::NONE`, `ELEMENT` or `ARRAY`.
    pub desc_type: i32,
    /// The data type code.
    pub datatype: i32,
    /// The element count of an array.
    pub info1: i32,
    /// Reserved.
    pub info2: i32,
    /// The length in bytes of the argument, or of one element.
    pub length: i32,
}

/// Entry `pos` (from 1) of the list at `list`; `None` without a list
/// or past its count.
#[must_use]
pub fn entry(list: u64, pos: i32) -> Option<OpDesc> {
    if list == 0 {
        return None;
    }
    let head = crate::rtlib::mem::bytes(list.cast_signed(), 4);
    let count = i32::from_ne_bytes([head[0], head[1], head[2], head[3]]);
    if pos < 1 || pos > count {
        return None;
    }
    let at = list + u64::from(OPDESC_HEADER) + u64::from(OPDESC_ENTRY) * u64::from(pos.cast_unsigned() - 1);
    let b = crate::rtlib::mem::bytes(at.cast_signed(), 20);
    let word = |i: usize| i32::from_ne_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
    Some(OpDesc {
        desc_type: word(0),
        datatype: word(1),
        info1: word(2),
        info2: word(3),
        length: word(4),
    })
}
