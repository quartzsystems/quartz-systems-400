// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media section of a `*DEVD`: what `CRTDEVTAP`, `CRTDEVOPT` and
//! `CRTDEVMLB` keep for a tape drive, an optical drive or a media
//! library (the resource, the vary-on and vary-off choices, the
//! library's wait times). It follows the device description's 192
//! bytes when `MedLen` says so (`docs/WORKMGMT.md`, `MediaDevice`).

use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The section.
pub const LAYOUT: Layout = Layout {
    name: "MediaDevice",
    size: Some(64),
    fields: &[
        field("Kind", 0, 1),
        field("Assign", 1, 1),
        field("Unload", 2, 1),
        field("GenCtgId", 3, 1),
        field("RsrcName", 4, 10),
        field("Reserved", 14, 2),
        field("UnloadWait", 16, 4),
        field("MaxDevTime", 20, 4),
        field("LastUsed", 24, 8),
        field("Reserved2", 32, 32),
    ],
};

/// What kind of media device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum MediaKind {
    /// A tape drive (`*TAP`).
    #[default]
    Tape = 0,
    /// An optical drive (`*OPT`).
    Optical = 1,
    /// A tape media library (`*TAPMLB`).
    TapeLibrary = 2,
}

impl MediaKind {
    /// The device class kept in the description.
    #[must_use]
    pub fn devcls(self) -> &'static str {
        match self {
            Self::Tape => "*TAP",
            Self::Optical => "*OPT",
            Self::TapeLibrary => "*TAPMLB",
        }
    }

    /// From a device class.
    #[must_use]
    pub fn from_devcls(c: &str) -> Option<Self> {
        match c {
            "*TAP" => Some(Self::Tape),
            "*OPT" => Some(Self::Optical),
            "*TAPMLB" => Some(Self::TapeLibrary),
            _ => None,
        }
    }

    /// The prefix of a virtual resource's name.
    #[must_use]
    pub fn virtual_prefix(self) -> &'static str {
        match self {
            Self::Tape | Self::TapeLibrary => "TAPVRT",
            Self::Optical => "OPTVRT",
        }
    }

    /// The type and model of a virtual device, as IBM i reports them.
    #[must_use]
    pub fn virtual_type(self) -> (&'static str, &'static str) {
        match self {
            Self::Tape | Self::TapeLibrary => ("63B0", "001"),
            Self::Optical => ("632B", "001"),
        }
    }
}

/// A media device's own settings.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MediaDevice {
    /// Tape, optical or library.
    pub kind: MediaKind,
    /// `ASSIGN(*YES)`.
    pub assign: bool,
    /// `UNLOAD(*YES)`.
    pub unload: bool,
    /// `GENCTGID(*SYSGEN)`.
    pub gen_ctg_sysgen: bool,
    /// The resource: `TAP01`, `OPT01`, `TAPMLB01`, `TAPVRT01`, `OPTVRT01`.
    pub rsrcname: String,
    /// `UNLOADWAIT` in seconds; 0 `*SYSGEN`.
    pub unload_wait: u32,
    /// `MAXDEVTIME` in minutes; 0 `*SYSGEN`.
    pub max_dev_time: u32,
    /// Unix time the device was last used; 0 never.
    pub last_used: u64,
}

impl MediaDevice {
    /// Whether the resource is a virtual one (`RSRCNAME(*VRT)`).
    #[must_use]
    pub fn is_virtual(&self) -> bool {
        self.rsrcname.starts_with(self.kind.virtual_prefix())
    }

    /// The section's bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_uint("Kind", u64::from(self.kind as u8));
        r.set_flag("Assign", self.assign);
        r.set_flag("Unload", self.unload);
        r.set_flag("GenCtgId", self.gen_ctg_sysgen);
        r.set_text("RsrcName", &self.rsrcname);
        r.set_uint("UnloadWait", u64::from(self.unload_wait));
        r.set_uint("MaxDevTime", u64::from(self.max_dev_time));
        r.set_uint("LastUsed", self.last_used);
        r.buf
    }

    /// Read the section back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        let kind = match r.uint("Kind") {
            0 => MediaKind::Tape,
            1 => MediaKind::Optical,
            2 => MediaKind::TapeLibrary,
            k => return Err(Error::Corrupt(format!("media device kind {k}"))),
        };
        Ok(Self {
            kind,
            assign: r.flag("Assign"),
            unload: r.flag("Unload"),
            gen_ctg_sysgen: r.flag("GenCtgId"),
            rsrcname: r.text("RsrcName"),
            unload_wait: u32::try_from(r.uint("UnloadWait")).unwrap_or(0),
            max_dev_time: u32::try_from(r.uint("MaxDevTime")).unwrap_or(0),
            last_used: r.uint("LastUsed"),
        })
    }
}

/// The virtual resource name for a number: `TAPVRT01`, `OPTVRT01`.
#[must_use]
pub fn virtual_rsrc(kind: MediaKind, n: u32) -> String {
    format!("{}{n:02}", kind.virtual_prefix())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let m = MediaDevice {
            kind: MediaKind::TapeLibrary,
            assign: true,
            unload: true,
            gen_ctg_sysgen: false,
            rsrcname: "TAPMLB01".into(),
            unload_wait: 120,
            max_dev_time: 30,
            last_used: 7,
        };
        assert_eq!(MediaDevice::decode(&m.encode()).unwrap(), m);
        assert_eq!(m.encode().len(), 64);
        assert!(!m.is_virtual());
        let v = MediaDevice {
            rsrcname: virtual_rsrc(MediaKind::Optical, 1),
            kind: MediaKind::Optical,
            ..MediaDevice::default()
        };
        assert_eq!(v.rsrcname, "OPTVRT01");
        assert!(v.is_virtual());
    }
}
