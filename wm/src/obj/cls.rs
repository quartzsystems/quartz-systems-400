// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*CLS`: the run attributes routing gives a job.

use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The payload.
pub const LAYOUT: Layout = Layout {
    name: "Class",
    size: Some(84),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("RunPty", 10, 1),
        field("Purge", 11, 1),
        field("TimeSlice", 12, 4),
        field("DftWait", 16, 4),
        field("CpuTime", 20, 4),
        field("MaxTmpStg", 24, 4),
        field("MaxThd", 28, 4),
        field("Text", 32, 50),
        field("Reserved", 82, 2),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMCLS ";
const VERSION: u64 = 1;
/// `*NOMAX` in a four-byte limit.
const NOMAX: u64 = 0xFFFF_FFFF;

/// A class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cls {
    /// `RUNPTY` 1-99.
    pub runpty: u8,
    /// `TIMESLICE` in milliseconds.
    pub timeslice: u32,
    /// `PURGE`.
    pub purge: bool,
    /// `DFTWAIT` in seconds; `None` is `*NOMAX`.
    pub dftwait: Option<u32>,
    /// `CPUTIME` in milliseconds; `None` is `*NOMAX`.
    pub cputime: Option<u32>,
    /// `MAXTMPSTG` in megabytes; `None` is `*NOMAX`.
    pub maxtmpstg: Option<u32>,
    /// `MAXTHD`; `None` is `*NOMAX`.
    pub maxthd: Option<u32>,
    /// `TEXT`.
    pub text: String,
}

impl Default for Cls {
    /// `CRTCLS`'s defaults.
    fn default() -> Self {
        Self {
            runpty: 50,
            timeslice: 2000,
            purge: true,
            dftwait: Some(120),
            cputime: None,
            maxtmpstg: None,
            maxthd: None,
            text: String::new(),
        }
    }
}

fn limit(v: Option<u32>) -> u64 {
    v.map_or(NOMAX, u64::from)
}

fn unlimit(v: u64) -> Option<u32> {
    (v != NOMAX).then(|| u32::try_from(v).unwrap_or(u32::MAX))
}

impl Cls {
    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_uint("RunPty", u64::from(self.runpty));
        r.set_flag("Purge", self.purge);
        r.set_uint("TimeSlice", u64::from(self.timeslice));
        r.set_uint("DftWait", limit(self.dftwait));
        r.set_uint("CpuTime", limit(self.cputime));
        r.set_uint("MaxTmpStg", limit(self.maxtmpstg));
        r.set_uint("MaxThd", limit(self.maxthd));
        r.set_text("Text", &self.text);
        r.buf
    }

    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a class".into()));
        }
        Ok(Self {
            runpty: u8::try_from(r.uint("RunPty")).unwrap_or(99),
            timeslice: u32::try_from(r.uint("TimeSlice")).unwrap_or(u32::MAX),
            purge: r.flag("Purge"),
            dftwait: unlimit(r.uint("DftWait")),
            cputime: unlimit(r.uint("CpuTime")),
            maxtmpstg: unlimit(r.uint("MaxTmpStg")),
            maxthd: unlimit(r.uint("MaxThd")),
            text: r.text("Text"),
        })
    }

    /// Check the values.
    pub fn validate(&self) -> Result<()> {
        if !(1..=99).contains(&self.runpty) {
            return Err(Error::value("CPD0085", "RUNPTY is 1 through 99"));
        }
        if !(1..=9_999_999).contains(&self.timeslice) {
            return Err(Error::value("CPD0085", "TIMESLICE is 1 through 9999999"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let c = Cls::default();
        assert_eq!(Cls::decode(&c.encode()).unwrap(), c);
        let d = Cls {
            runpty: 20,
            timeslice: 5000,
            purge: false,
            dftwait: None,
            cputime: Some(60_000),
            maxtmpstg: Some(512),
            maxthd: Some(64),
            text: "Interactive".into(),
        };
        assert_eq!(Cls::decode(&d.encode()).unwrap(), d);
        assert!(
            Cls {
                runpty: 0,
                ..Cls::default()
            }
            .validate()
            .is_err()
        );
    }
}
