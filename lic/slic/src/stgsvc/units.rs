// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The disk units as `WRKDSKSTS`, `WRKSYSSTS` and Navigator show them:
//! type and model, size and use, protection and status. Pages device
//! parity holds (its share and a larger member's unused rest) are no
//! capacity the ASP can use, so size and use leave them out, as IBM's
//! displays leave out the parity share. The IFS volume is ASP pages in
//! use; an older system's IFS partition sits on the load source, unit
//! 1, and counts there.

use std::path::Path;

use store::asp::parity::Level;
use store::asp::superblock::NO_PARTNER;

use crate::ipl::storage::{self, unit_uuid_of};
use crate::ipl::system::System;

/// One unit as the displays show it.
pub struct UnitView {
    /// Unit number, from 0.
    pub unit: u16,
    /// The medium: `SSD`, `HDD`, `NVME`, `VIRTUAL`, `DISK`.
    pub type_code: String,
    /// The device's model, from sysfs; empty when unknown.
    pub model: String,
    /// Size, pages.
    pub pages: u64,
    /// Usable pages, the base of the use figure.
    pub usable: u64,
    /// Pages in use.
    pub used: u64,
    /// `None`, `Mirrored (A)`, `Device parity (RAID 5)`.
    pub protection: String,
    /// `Active`, `Suspended`, `Synchronizing 37%`, ...
    pub status: String,
    /// The serial number.
    pub serial: String,
}

/// A parity member's held pages, status, level and how far its set's
/// synchronization has come.
struct Dpy {
    held: u64,
    status: &'static str,
    level: Level,
    sync: Option<u8>,
}

/// Every unit in unit order; empty without a store.
pub fn views(system: &mut System) -> Vec<UnitView> {
    let ifs = super::ifs::partition_capacity(system).map(|(size, used)| (size / 4096, used / 4096));
    let hw = system.hw();
    let Some(s) = system.storage.as_ref() else {
        return Vec::new();
    };
    let dpy: Vec<(u16, Dpy)> = s
        .store
        .parity_sets()
        .iter()
        .flat_map(|set| {
            set.members.iter().map(|m| {
                let d = Dpy {
                    held: s.store.asp().parity_held(m.unit),
                    status: m.status,
                    level: set.level,
                    sync: set.sync_percent,
                };
                (m.unit, d)
            })
        })
        .collect();
    // Devices by the unit UUID in their header, as DST finds them.
    let devices: Vec<_> = s.devices.iter().map(|d| (unit_uuid_of(d), d.clone())).collect();
    s.store
        .units()
        .into_iter()
        .map(|u| {
            let e = &u.entry;
            let device = devices
                .iter()
                .find(|(id, _)| *id == Some(e.unit_uuid))
                .map(|(_, d)| d.as_path());
            let (type_code, model) = identity(&hw, device);
            let parity = dpy.iter().find(|(n, _)| *n == u.unit).map(|(_, d)| d);
            let held = parity.map_or(0, |d| d.held);
            let (ifs_pages, ifs_used) = if u.unit == 0 { ifs.unwrap_or((0, 0)) } else { (0, 0) };
            UnitView {
                unit: u.unit,
                type_code,
                model,
                pages: e.pages.saturating_sub(held) + ifs_pages,
                usable: e.usable_pages.saturating_sub(held) + ifs_pages,
                used: u.used_pages.saturating_sub(held) + ifs_used,
                protection: protection(u.unit, e.mirror_partner, parity.map(|d| d.level)),
                status: match parity {
                    _ if !u.attached => "Not found".into(),
                    Some(d) => parity_status(d.status, d.sync, storage::parity_stalled()),
                    None => unit_status(e.state).into(),
                },
                serial: e.serial_text(),
            }
        })
        .collect()
}

/// Pages device parity holds over the whole ASP.
pub fn parity_held(system: &System) -> u64 {
    system.storage.as_ref().map_or(0, |s| {
        s.store
            .parity_sets()
            .iter()
            .flat_map(|set| set.members.iter())
            .map(|m| s.store.asp().parity_held(m.unit))
            .sum()
    })
}

/// The type of a unit's disk (its medium, as the hardware table has
/// it, else from sysfs) and the disk's model.
fn identity(hw: &hwrsrc::Table, device: Option<&Path>) -> (String, String) {
    let Some(device) = device else {
        return (hwrsrc::Kind::Disk.default_type_model().0.to_string(), String::new());
    };
    let disk = diskimg::locate::disk_of(device).unwrap_or_else(|| {
        device
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    let type_code = hw
        .resources
        .iter()
        .find(|r| r.kind == hwrsrc::Kind::Disk && r.location == disk)
        .map_or_else(
            || {
                let dir = Path::new("/sys/class/block").join(&disk);
                hwrsrc::storage::medium_at(&dir).type_word().to_string()
            },
            |r| r.type_model().0,
        );
    let (model, _) = diskimg::locate::identity(device);
    (type_code, model)
}

/// A unit's protection: its mirrored half (`(A)` the lower unit of a
/// pair) or its parity set's level.
fn protection(unit: u16, partner: u16, parity: Option<Level>) -> String {
    match parity {
        Some(Level::Raid5) => "Device parity (RAID 5)".into(),
        Some(Level::Raid6) => "Device parity (RAID 6)".into(),
        None if partner == NO_PARTNER => "None".into(),
        None if unit < partner => "Mirrored (A)".into(),
        None => "Mirrored (B)".into(),
    }
}

/// A parity member's status from IBM's `DPY/...`, with the progress of
/// its set's synchronization (the set has no parity until it ends).
fn parity_status(dpy: &str, sync: Option<u8>, stalled: bool) -> String {
    match (dpy, sync) {
        ("DPY/Unprotected", Some(_)) if stalled => "Synchronization stopped".into(),
        ("DPY/Unprotected", Some(p)) => format!("Synchronizing {p}%"),
        ("DPY/Active", _) => "Active".into(),
        ("DPY/Degraded", _) => "Degraded".into(),
        ("DPY/Rebuild", _) => "Rebuilding".into(),
        ("DPY/Failed", _) => "Failed".into(),
        _ => "Unprotected".into(),
    }
}

/// An attached unit's status as Navigator words it.
fn unit_status(state: store::asp::unit::UnitState) -> &'static str {
    use store::asp::unit::UnitState as S;
    match state {
        S::Unprotected | S::Active => "Active",
        S::Suspended => "Suspended",
        S::Resuming => "Resuming",
        S::NotOperational => "Failed",
        S::Missing => "Not found",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protection_names_the_pair_half_and_the_parity_level() {
        assert_eq!(protection(0, NO_PARTNER, None), "None");
        assert_eq!(protection(0, 1, None), "Mirrored (A)");
        assert_eq!(protection(1, 0, None), "Mirrored (B)");
        assert_eq!(protection(2, NO_PARTNER, Some(Level::Raid5)), "Device parity (RAID 5)");
        assert_eq!(protection(2, NO_PARTNER, Some(Level::Raid6)), "Device parity (RAID 6)");
        assert_eq!(parity_status("DPY/Rebuild", None, false), "Rebuilding");
        assert_eq!(parity_status("DPY/Unprotected", None, false), "Unprotected");
        assert_eq!(parity_status("DPY/Unprotected", Some(37), false), "Synchronizing 37%");
        assert_eq!(parity_status("DPY/Unprotected", Some(37), true), "Synchronization stopped");
        assert_eq!(parity_status("DPY/Active", None, false), "Active");
    }
}
