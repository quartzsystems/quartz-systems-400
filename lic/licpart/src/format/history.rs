// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IPL history records, 28 bytes each, stored as reserved-tag TLVs.

use crate::error::LicError;
use crate::format::header::{IplMode, IplType, Slot};
use crate::format::{Reader, put_u64};

/// How an IPL ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Reached IPL complete.
    Normal,
    /// Was found still in progress at the next IPL.
    Abnormal,
    /// Stopped on a fatal SRC.
    Failed,
    /// Running now.
    InProgress,
}

impl Outcome {
    fn as_u8(self) -> u8 {
        match self {
            Outcome::Normal => 0,
            Outcome::Abnormal => 1,
            Outcome::Failed => 2,
            Outcome::InProgress => 3,
        }
    }
    fn from_u8(v: u8) -> Result<Self, LicError> {
        match v {
            0 => Ok(Outcome::Normal),
            1 => Ok(Outcome::Abnormal),
            2 => Ok(Outcome::Failed),
            3 => Ok(Outcome::InProgress),
            _ => Err(LicError::Malformed("outcome")),
        }
    }
}

/// One IPL, start to end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IplHistory {
    /// Unix time the IPL started.
    pub start_time: u64,
    /// Unix time it ended, 0 if unknown.
    pub end_time: u64,
    /// IPL type.
    pub ipl_type: IplType,
    /// IPL mode.
    pub mode: IplMode,
    /// Slot booted.
    pub slot: Slot,
    /// Last SRC emitted, ASCII, space padded.
    pub final_src: [u8; 8],
    /// How it ended.
    pub outcome: Outcome,
}

impl IplHistory {
    /// The final SRC as text.
    pub fn final_src_str(&self) -> &str {
        std::str::from_utf8(&self.final_src)
            .unwrap_or("????????")
            .trim_end()
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(28);
        put_u64(&mut out, self.start_time);
        put_u64(&mut out, self.end_time);
        out.push(match self.ipl_type {
            IplType::A => 0,
            IplType::B => 1,
            IplType::D => 2,
        });
        out.push(match self.mode {
            IplMode::Normal => 0,
            IplMode::Manual => 1,
        });
        out.push(match self.slot {
            Slot::A => 0,
            Slot::B => 1,
        });
        out.extend_from_slice(&self.final_src);
        out.push(self.outcome.as_u8());
        out
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, LicError> {
        let mut r = Reader::new(bytes);
        let h = Self {
            start_time: r.u64()?,
            end_time: r.u64()?,
            ipl_type: IplType::from_u8(r.u8()?)?,
            mode: IplMode::from_u8(r.u8()?)?,
            slot: Slot::from_u8(r.u8()?)?,
            final_src: r.bytes(8)?.try_into().expect("8 bytes"),
            outcome: Outcome::from_u8(r.u8()?)?,
        };
        if !r.at_end() {
            return Err(LicError::Malformed("history record length"));
        }
        Ok(h)
    }
}
