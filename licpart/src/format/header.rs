// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fixed header: 24 bytes of IPL state.

use crate::error::LicError;
use crate::format::{Reader, put_u32, put_u64};

/// IPL type, IBM style: A and B are the two code slots, D is install media.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IplType {
    /// Slot A.
    A,
    /// Slot B.
    B,
    /// Install or alternate media.
    D,
}

/// Keylock position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IplMode {
    /// Unattended.
    Normal,
    /// Attended, shows the IPL menu.
    Manual,
}

/// A boot slot on the ESP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// `EFI/BOOT/BOOTX64.EFI`.
    A,
    /// `EFI/QS400/B.EFI`.
    B,
}

impl IplType {
    fn as_u8(self) -> u8 {
        match self {
            IplType::A => 0,
            IplType::B => 1,
            IplType::D => 2,
        }
    }
    pub(crate) fn from_u8(v: u8) -> Result<Self, LicError> {
        match v {
            0 => Ok(IplType::A),
            1 => Ok(IplType::B),
            2 => Ok(IplType::D),
            _ => Err(LicError::Malformed("ipl type")),
        }
    }
}

impl IplMode {
    fn as_u8(self) -> u8 {
        match self {
            IplMode::Normal => 0,
            IplMode::Manual => 1,
        }
    }
    pub(crate) fn from_u8(v: u8) -> Result<Self, LicError> {
        match v {
            0 => Ok(IplMode::Normal),
            1 => Ok(IplMode::Manual),
            _ => Err(LicError::Malformed("ipl mode")),
        }
    }
}

impl Slot {
    fn as_u8(self) -> u8 {
        match self {
            Slot::A => 0,
            Slot::B => 1,
        }
    }
    pub(crate) fn from_u8(v: u8) -> Result<Self, LicError> {
        match v {
            0 => Ok(Slot::A),
            1 => Ok(Slot::B),
            _ => Err(LicError::Malformed("slot")),
        }
    }
    /// The other slot.
    #[must_use]
    pub fn other(self) -> Slot {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }
}

/// Fixed IPL state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// Set at IPL start, cleared at IPL complete. Still set at the next
    /// start means the previous IPL was abnormal.
    pub ipl_in_progress: bool,
    /// Type of the last IPL.
    pub last_ipl_type: IplType,
    /// Mode of the last IPL.
    pub last_ipl_mode: IplMode,
    /// Slot the last IPL booted from.
    pub last_slot: Slot,
    /// Slot of the last IPL that reached completion.
    pub last_good_slot: Slot,
    /// Mode override for the next IPL, set from the service console.
    pub next_mode: Option<IplMode>,
    /// IPLs started.
    pub ipl_count: u32,
    /// IPLs detected as abnormal.
    pub abnormal_count: u32,
    /// Unix time of first initialization.
    pub install_time: u64,
    /// 0 when the system is installed; 1 when the LIC is installed and
    /// the operating system install is still to come.
    pub install_stage: u8,
    /// The pending operating system install runs without prompts.
    pub auto_install: bool,
}

impl Header {
    /// Defaults for a fresh partition.
    pub fn new(install_time: u64) -> Self {
        Self {
            ipl_in_progress: false,
            last_ipl_type: IplType::A,
            last_ipl_mode: IplMode::Normal,
            last_slot: Slot::A,
            last_good_slot: Slot::A,
            next_mode: None,
            ipl_count: 0,
            abnormal_count: 0,
            install_time,
            install_stage: 0,
            auto_install: false,
        }
    }

    pub(crate) fn encode(&self, out: &mut Vec<u8>) {
        out.push(u8::from(self.ipl_in_progress));
        out.push(self.last_ipl_type.as_u8());
        out.push(self.last_ipl_mode.as_u8());
        out.push(self.last_slot.as_u8());
        out.push(self.last_good_slot.as_u8());
        out.push(self.next_mode.map_or(0, |m| m.as_u8() + 1));
        out.push(self.install_stage);
        out.push(u8::from(self.auto_install));
        put_u32(out, self.ipl_count);
        put_u32(out, self.abnormal_count);
        put_u64(out, self.install_time);
    }

    pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Self, LicError> {
        let ipl_in_progress = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err(LicError::Malformed("ipl_in_progress")),
        };
        let last_ipl_type = IplType::from_u8(r.u8()?)?;
        let last_ipl_mode = IplMode::from_u8(r.u8()?)?;
        let last_slot = Slot::from_u8(r.u8()?)?;
        let last_good_slot = Slot::from_u8(r.u8()?)?;
        let next_mode = match r.u8()? {
            0 => None,
            v => Some(IplMode::from_u8(v - 1)?),
        };
        let install_stage = r.u8()?;
        let auto_install = r.u8()? & 1 != 0;
        Ok(Self {
            ipl_in_progress,
            last_ipl_type,
            last_ipl_mode,
            last_slot,
            last_good_slot,
            next_mode,
            ipl_count: r.u32()?,
            abnormal_count: r.u32()?,
            install_time: r.u64()?,
            install_stage,
            auto_install,
        })
    }
}
