// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The firmware's boot choice through efivarfs: a `Boot####` entry for
//! each slot, `BootNext` for the one IPL a LIC PTF or `IPLSRC` asks
//! for, `BootOrder` for the slot IPLs boot from then on
//! (`docs/PTF.md` §5). Plain file writes; no ioctl.

use std::fs;
use std::path::{Path, PathBuf};

use diskimg::efi::{self, EspLocation};
use licpart::Slot;
use rustix::fs::{IFlags, ioctl_getflags, ioctl_setflags};
use rustix::mount::{MountFlags, mount};

/// Where efivarfs is mounted.
const EFIVARS: &str = "/sys/firmware/efi/efivars";
/// Slot A's boot entry number (`Boot5141`).
pub const ENTRY_A: u16 = 0x5141;
/// Slot B's (`Boot5142`).
pub const ENTRY_B: u16 = 0x5142;
/// Slot A's path on the ESP.
pub const PATH_A: &str = "EFI/BOOT/BOOTX64.EFI";
/// Slot B's path on the ESP.
pub const PATH_B: &str = "EFI/QS400/B.EFI";

/// A slot's boot entry number and ESP path.
#[must_use]
pub fn entry(s: Slot) -> (u16, &'static str) {
    match s {
        Slot::A => (ENTRY_A, PATH_A),
        Slot::B => (ENTRY_B, PATH_B),
    }
}

/// The firmware of this machine.
pub struct EfiFirmware {
    dir: PathBuf,
    esp: EspLocation,
}

fn var(dir: &Path, name: &str) -> PathBuf {
    dir.join(efi::var_name(name))
}

impl EfiFirmware {
    /// The firmware, with efivarfs mounted, for the ESP beside the LIC
    /// partition.
    pub fn open() -> Result<Self, String> {
        Self::open_for(licpart::locate().ok().as_deref())
    }

    /// The firmware, for the ESP beside LIC partition `lic` (the disk a
    /// LIC install just wrote).
    pub fn open_for(lic: Option<&Path>) -> Result<Self, String> {
        if !Path::new("/sys/firmware/efi").is_dir() {
            return Err("not booted through UEFI".into());
        }
        if !fs::read_dir(EFIVARS).is_ok_and(|mut d| d.next().is_some()) {
            match mount("efivarfs", EFIVARS, "efivarfs", MountFlags::NOSUID | MountFlags::NOEXEC | MountFlags::NODEV, None) {
                Ok(()) | Err(rustix::io::Errno::BUSY) => {}
                Err(e) => return Err(format!("efivarfs: {e}")),
            }
        }
        let (_, esp) = diskimg::locate::esp(lic).map_err(|e| e.to_string())?;
        Ok(Self {
            dir: PathBuf::from(EFIVARS),
            esp,
        })
    }

    fn write(&self, name: &str, data: &[u8]) -> Result<(), String> {
        let path = var(&self.dir, name);
        // efivarfs marks the firmware's own variables (`BootOrder`,
        // `Boot####`) immutable; the flag comes off before a write.
        if let Ok(f) = fs::File::open(&path)
            && let Ok(flags) = ioctl_getflags(&f)
            && flags.contains(IFlags::IMMUTABLE)
        {
            ioctl_setflags(&f, flags - IFlags::IMMUTABLE).map_err(|e| format!("{name}: {e}"))?;
        }
        fs::write(&path, efi::file_bytes(data)).map_err(|e| format!("{name}: {e}"))
    }

    /// Slot A's entry first in `BootOrder`, both slots' entries made: a
    /// newly installed disk boots without the firmware's setup.
    pub fn register_install(&mut self) -> Result<(), String> {
        ptf::slot::Firmware::boot_default(self, Slot::A)
    }

    /// Both slots' `Boot####` entries, written when missing or changed.
    fn ensure_entries(&self) -> Result<(), String> {
        for s in [Slot::A, Slot::B] {
            let (n, path) = entry(s);
            let name = efi::boot_name(n);
            let want = efi::load_option(&format!("QS/400 slot {}", if s == Slot::A { 'A' } else { 'B' }), &self.esp, path);
            let have = fs::read(var(&self.dir, &name)).ok();
            if have.as_deref().and_then(|h| h.get(4..)) != Some(want.as_slice()) {
                self.write(&name, &want)?;
            }
        }
        Ok(())
    }
}

impl ptf::slot::Firmware for EfiFirmware {
    fn boot_next(&mut self, s: Slot) -> Result<(), String> {
        self.ensure_entries()?;
        self.write("BootNext", &efi::numbers(&[entry(s).0]))
    }

    fn boot_default(&mut self, s: Slot) -> Result<(), String> {
        self.ensure_entries()?;
        let order = fs::read(var(&self.dir, "BootOrder"))
            .map(|b| efi::parse_numbers(&b))
            .unwrap_or_default();
        self.write("BootOrder", &efi::numbers(&efi::order_with_first(&order, entry(s).0)))
    }
}
