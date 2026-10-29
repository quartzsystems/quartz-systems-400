// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LL/CP parameters: a 4-byte length (itself and the code point
//! included), a 2-byte code point, the data.

use super::header::{be16, be32};

/// The LL and CP bytes before a parameter's data.
pub const PARAM_HEAD: usize = 6;

/// One parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    /// Code point.
    pub cp: u16,
    /// Data after the code point.
    pub data: Vec<u8>,
}

impl Param {
    /// A parameter.
    pub fn new(cp: u16, data: impl Into<Vec<u8>>) -> Self {
        Self {
            cp,
            data: data.into(),
        }
    }

    /// A parameter holding a big-endian u32.
    pub fn u32(cp: u16, v: u32) -> Self {
        Self::new(cp, v.to_be_bytes())
    }

    /// A parameter holding a big-endian u16.
    pub fn u16(cp: u16, v: u16) -> Self {
        Self::new(cp, v.to_be_bytes())
    }

    /// A parameter holding one byte.
    pub fn byte(cp: u16, v: u8) -> Self {
        Self::new(cp, [v])
    }

    /// Its encoded size.
    pub fn len(&self) -> usize {
        PARAM_HEAD + self.data.len()
    }

    /// Whether it carries no data.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// The data as a big-endian u32, when it is four bytes.
    pub fn as_u32(&self) -> Option<u32> {
        (self.data.len() == 4).then(|| be32(&self.data, 0))
    }

    /// The data as a big-endian u16, when it is two bytes.
    pub fn as_u16(&self) -> Option<u16> {
        (self.data.len() == 2).then(|| be16(&self.data, 0))
    }

    /// Append LL, CP and data.
    pub fn encode(&self, out: &mut Vec<u8>) {
        let ll = u32::try_from(self.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&ll.to_be_bytes());
        out.extend_from_slice(&self.cp.to_be_bytes());
        out.extend_from_slice(&self.data);
    }
}

/// Split `b` into parameters. Stops at the first LL that does not fit
/// and returns what is left as the tail.
pub fn split(b: &[u8]) -> (Vec<Param>, &[u8]) {
    let mut params = Vec::new();
    let mut at = 0;
    while b.len() - at >= PARAM_HEAD {
        let ll = be32(b, at) as usize;
        if ll < PARAM_HEAD || ll > b.len() - at {
            break;
        }
        params.push(Param::new(be16(b, at + 4), &b[at + PARAM_HEAD..at + ll]));
        at += ll;
    }
    (params, &b[at..])
}

/// The first parameter with code point `cp`.
pub fn find(params: &[Param], cp: u16) -> Option<&Param> {
    params.iter().find(|p| p.cp == cp)
}

/// Encode a parameter list.
pub fn encode_all(params: &[Param], out: &mut Vec<u8>) {
    for p in params {
        p.encode(out);
    }
}
