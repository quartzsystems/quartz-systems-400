// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product activity log: attention and fatal reference codes with
//! their times, a ring of the newest entries kept as reserved-tag TLVs.

use crate::error::LicError;
use crate::format::{Reader, put_u64};

/// Entries kept.
pub const PAL_MAX: usize = 64;

/// How serious an entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The IPL went on.
    Attention,
    /// The IPL stopped.
    Fatal,
}

/// One logged code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalEntry {
    /// Unix seconds.
    pub time: u64,
    /// The code, 8 ASCII bytes.
    pub code: [u8; 8],
    /// Severity.
    pub severity: Severity,
}

impl PalEntry {
    /// The code as text.
    #[must_use]
    pub fn code_str(&self) -> String {
        String::from_utf8_lossy(&self.code).into_owned()
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(17);
        put_u64(&mut out, self.time);
        out.extend_from_slice(&self.code);
        out.push(match self.severity {
            Severity::Attention => 0,
            Severity::Fatal => 1,
        });
        out
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, LicError> {
        let mut r = Reader::new(bytes);
        let time = r.u64()?;
        let code: [u8; 8] = r.bytes(8)?.try_into().expect("8 bytes");
        let severity = match r.u8()? {
            0 => Severity::Attention,
            1 => Severity::Fatal,
            _ => return Err(LicError::Malformed("pal severity")),
        };
        if !r.at_end() {
            return Err(LicError::Malformed("pal entry length"));
        }
        Ok(Self {
            time,
            code,
            severity,
        })
    }
}
