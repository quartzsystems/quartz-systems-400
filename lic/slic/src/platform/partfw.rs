// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The boot choice of a system in a partition: no firmware keeps
//! `BootNext` and `BootOrder` there, so the side of the next IPL goes
//! into the LIC record (`licpart::iplsrc`), where the hypervisor reads
//! it before the IPL. Used only when the system did not come up through
//! UEFI and the CPU says a hypervisor runs it; a machine with UEFI
//! firmware never gets here.

use std::fs;
use std::path::Path;

use licpart::{Device, IplSource, LicPart, Record, Slot};

/// Whether this system runs in a partition with no firmware: it did not
/// boot through UEFI and the CPU's hypervisor flag is set.
#[must_use]
pub fn in_partition() -> bool {
    !Path::new("/sys/firmware/efi").is_dir()
        && fs::read_to_string("/proc/cpuinfo").is_ok_and(|t| hypervisor_flag(&t))
}

/// Whether `/proc/cpuinfo`'s text has the hypervisor flag.
fn hypervisor_flag(cpuinfo: &str) -> bool {
    cpuinfo
        .lines()
        .any(|l| l.starts_with("flags") && l.split_whitespace().any(|f| f == "hypervisor"))
}

/// The boot choice asked of a partition, to be put in the LIC record.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PartFirmware {
    next: Option<Slot>,
    default: Option<Slot>,
}

impl PartFirmware {
    /// Nothing asked yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether anything was asked.
    #[must_use]
    pub fn asked(self) -> bool {
        self.next.is_some() || self.default.is_some()
    }

    /// Put what was asked into `record`, over what it holds.
    pub fn set(self, record: &mut Record) -> Result<(), String> {
        if !self.asked() {
            return Ok(());
        }
        let mut src = IplSource::of(record);
        if let Some(s) = self.next {
            src.next = Some(s);
        }
        if let Some(s) = self.default {
            src.default = Some(s);
        }
        src.store(record).map_err(|e| e.to_string())
    }

    /// Put what was asked into the newest record and write it.
    pub fn commit<D: Device>(
        self,
        part: &mut LicPart<D>,
        record: &mut Record,
    ) -> Result<(), String> {
        if !self.asked() {
            return Ok(());
        }
        part.refresh(record);
        self.set(record)?;
        part.write(record).map(|_| ()).map_err(|e| e.to_string())
    }
}

impl ptf::slot::Firmware for PartFirmware {
    fn boot_next(&mut self, s: Slot) -> Result<(), String> {
        self.next = Some(s);
        Ok(())
    }

    fn boot_default(&mut self, s: Slot) -> Result<(), String> {
        self.default = Some(s);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptf::slot::Firmware;
    use std::io::Cursor;

    #[test]
    fn the_flag_is_read_from_the_flags_line() {
        assert!(hypervisor_flag("model name : x\nflags\t\t: fpu vme hypervisor lm\n"));
        assert!(!hypervisor_flag("model name : hypervisor\nflags\t\t: fpu vme lm\n"));
        assert!(!hypervisor_flag(""));
    }

    #[test]
    fn what_was_asked_goes_into_the_record() {
        let mut r = Record::new(1);
        let mut fw = PartFirmware::new();
        assert!(!fw.asked());
        fw.set(&mut r).unwrap();
        assert_eq!(r.tlv(licpart::TAG_IPL_SOURCE), None);
        fw.boot_next(Slot::B).unwrap();
        fw.set(&mut r).unwrap();
        assert_eq!(
            IplSource::of(&r),
            IplSource {
                next: Some(Slot::B),
                default: None
            }
        );
        // The default alone leaves a once-only slot standing.
        let mut fw = PartFirmware::new();
        fw.boot_default(Slot::B).unwrap();
        fw.set(&mut r).unwrap();
        assert_eq!(
            IplSource::of(&r),
            IplSource {
                next: Some(Slot::B),
                default: Some(Slot::B)
            }
        );
    }

    #[test]
    fn a_commit_writes_over_the_newest_record() {
        let dev = Cursor::new(vec![
            0u8;
            usize::try_from(licpart::RESUME_OFFSET).unwrap()
                + licpart::SLOT_BYTES
        ]);
        let (mut part, mut held) = LicPart::init(dev, 1).unwrap();
        // Someone else writes the record after `held` was read.
        let mut other = part.read().unwrap();
        other.set_tlv(0x0105, vec![2]).unwrap();
        part.write(&mut other).unwrap();
        let mut fw = PartFirmware::new();
        fw.boot_next(Slot::B).unwrap();
        fw.commit(&mut part, &mut held).unwrap();
        let back = part.read().unwrap();
        assert_eq!(IplSource::of(&back).next, Some(Slot::B));
        assert_eq!(back.tlv(0x0105), Some(&[2u8][..]));
    }
}
