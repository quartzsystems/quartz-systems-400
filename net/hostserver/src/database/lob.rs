// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LOB locators the server hands out for its own result sets: a
//! catalog column the primary describes as a `DBCLOB` locator in CCSID
//! 1200 (`SQLSTATISTICS`' `COLUMN_NAME`, which JDBC reports as `NCLOB`)
//! carries a handle in the row; the client reads the text with retrieve
//! LOB data (`0x1816`) and lets it go with free LOB (`0x1819`). Layouts
//! from the primary's `metadata` trace.

use std::collections::BTreeMap;

use hostsvr::frame::Param;
use sql::value::Value;

use crate::{reply, text};

/// Retrieve request parameters.
mod req {
    /// The locator handle.
    pub const LOCATOR: u16 = 0x3818;
    /// Characters wanted.
    pub const SIZE: u16 = 0x3819;
    /// The first character, from 0.
    pub const OFFSET: u16 = 0x381A;
    /// Whether the reply gives the LOB's current length (`'1'`).
    pub const CURRENT_LENGTH: u16 = 0x3821;
}

/// The reply's data: CCSID, length in characters, UTF-16.
const DATA: u16 = 0x380F;
/// The reply's current length: the length field's size (4), then it.
const LENGTH: u16 = 0x3810;

/// Most handles held at once; the oldest go first.
const MAX_HELD: usize = 4096;

/// The locators of one connection.
#[derive(Debug, Default)]
pub struct Locators {
    next: u32,
    held: BTreeMap<u32, Vec<u16>>,
}

impl Locators {
    /// A handle for `v` (null stays null).
    pub fn hold(&mut self, v: &Value) -> Value {
        if v.is_null() {
            return Value::Null;
        }
        self.next += 1;
        // Handles as the primary numbers them: 0x100, 0x200, ...
        let h = self.next << 8;
        self.held.insert(h, v.as_text().encode_utf16().collect());
        while self.held.len() > MAX_HELD {
            self.held.pop_first();
        }
        Value::int(i64::from(h))
    }

    /// Let a handle go.
    pub fn free(&mut self, params: &[Param]) {
        if let Some(h) = num(params, req::LOCATOR) {
            self.held.remove(&h);
        }
    }

    /// The reply parameters of a retrieve, or `None` for a handle not
    /// held.
    pub fn retrieve(&self, params: &[Param]) -> Option<Vec<Param>> {
        let chars = self.held.get(&num(params, req::LOCATOR)?)?;
        let from = (num(params, req::OFFSET).unwrap_or(0) as usize).min(chars.len());
        let n = (num(params, req::SIZE).unwrap_or(u32::MAX) as usize).min(chars.len() - from);
        let part = &chars[from..from + n];
        let mut out = Vec::new();
        let want_length = params.iter().any(|p| p.cp == req::CURRENT_LENGTH && p.data.first() == Some(&0xF1));
        if want_length {
            let mut d = 4u16.to_be_bytes().to_vec();
            d.extend(reply::len32(chars.len()).to_be_bytes());
            out.push(Param::new(LENGTH, d));
        }
        let mut d = 1200u16.to_be_bytes().to_vec();
        d.extend(reply::len32(part.len()).to_be_bytes());
        d.extend(text::utf16(&String::from_utf16_lossy(part)));
        out.push(Param::new(DATA, d));
        Some(out)
    }
}

fn num(params: &[Param], code: u16) -> Option<u32> {
    let p = params.iter().find(|p| p.cp == code)?;
    (p.data.len() >= 4).then(|| reply::be32(&p.data, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The primary's exchange for the index column `ID` (trace
    /// `database/metadata`, frames 232 and 233).
    #[test]
    fn retrieves_as_the_primary() {
        let mut l = Locators::default();
        assert_eq!(l.hold(&Value::Str("ID".into())), Value::int(0x100));
        let params = vec![
            Param::new(req::LOCATOR, vec![0, 0, 1, 0]),
            Param::new(req::SIZE, vec![0, 0x20, 0, 0]),
            Param::new(req::OFFSET, vec![0; 4]),
            Param::new(0x381B, vec![0xF0]),
            Param::new(req::CURRENT_LENGTH, vec![0xF1]),
            Param::new(0x3828, vec![0, 0, 0, 9]),
        ];
        let r = l.retrieve(&params).unwrap();
        assert_eq!(r[0], Param::new(LENGTH, vec![0, 4, 0, 0, 0, 2]));
        assert_eq!(r[1], Param::new(DATA, vec![0x04, 0xB0, 0, 0, 0, 2, 0, 0x49, 0, 0x44]));
        l.free(&params);
        assert!(l.retrieve(&params).is_none());
    }
}
