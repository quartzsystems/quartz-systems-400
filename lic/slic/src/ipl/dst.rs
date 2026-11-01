// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dedicated Service Tools on this system: the environment the
//! `svctools` panels talk to, and the loop that shows them.

use std::sync::mpsc::Receiver;

pub use svctools::Outcome;
use svctools::dst::console_mode::{ConsoleMode, TAG_CONSOLE_MODE};
use svctools::dst::security::{TAG_SECURITY, decode as decode_flags, encode as encode_flags};
use store::asp::parity::Level;
use store::{Refusal, StoreError};
use svctools::env::{DiskUnit, LogEntry, NonConfigured, ParityUnit, ResourceRow, SecurityFlags};
use svctools::install::lic::{LicEvent, LicKind};
use svctools::install::os::{DeviceKind, InstallDevice, OsOptions};
use svctools::install::options::{IplOptions, MajorOptions, TAG_IPL_OPTIONS};
use svctools::{DstEnv, UserDb};
use sysval::{IPL_NAMES, SysVals};

use hwrsrc::{Kind, ParityCapable};
use crate::ipl::menu::{self, Answer};
use crate::ipl::system::{System, now_unix};
use crate::ipl::{Event, storage};
use crate::src;

/// A failure the service tools show: what was being done; the reason
/// goes to the console log, never to the screen.
pub(crate) fn fail<E: std::fmt::Display>(what: &'static str) -> impl FnOnce(E) -> String {
    move |e| {
        eprintln!("slic: {what}: {e}");
        svctools::dst::failed(what)
    }
}

/// A disk change the store refused: its reason in the service tools'
/// words when the store gave one, else as [`fail`]. Either way the
/// store's own text goes to the console log.
pub(crate) fn refused(what: &'static str) -> impl FnOnce(StoreError) -> String {
    move |e| {
        eprintln!("slic: {what}: {e}");
        reason_of(&e).unwrap_or_else(|| svctools::dst::failed(what))
    }
}

/// The service tools' words for a refusal.
fn reason_of(e: &StoreError) -> Option<String> {
    use svctools::dst::disk::reasons as r;
    Some(match e {
        StoreError::Refused { unit, reason } => {
            let text = match reason {
                Refusal::Mirrored => r::MIRRORED,
                Refusal::NotMirrored => r::NOT_MIRRORED,
                Refusal::InParitySet(_) => r::IN_PARITY_SET,
                Refusal::NotInParitySet => r::NOT_IN_PARITY_SET,
                Refusal::NotAttached => r::NOT_ATTACHED,
                Refusal::HalfState(_) => r::HALF_STATE,
                Refusal::NotLast => r::NOT_LAST,
                Refusal::HasData(_) => r::HAS_DATA,
                Refusal::CannotMove { .. } => r::CANNOT_MOVE,
                Refusal::ParityUnits { least, .. } if *least >= 4 => r::RAID6_UNITS,
                Refusal::ParityUnits { .. } => r::RAID5_UNITS,
                Refusal::ParityShareInUse(_) => r::PARITY_SHARE_IN_USE,
                Refusal::ParitySetsFull => r::PARITY_SETS_FULL,
                Refusal::NoParitySet(_) => r::NO_PARITY_SET,
                Refusal::ParitySetNotActive(_) => r::PARITY_NOT_ACTIVE,
                Refusal::NotFailed => r::NOT_FAILED,
            };
            r::for_unit(text, *unit)
        }
        StoreError::UnitTooSmall { .. } => r::TOO_SMALL.to_string(),
        StoreError::NoSpace { .. } => r::NO_SPACE.to_string(),
        _ => return None,
    })
}

/// A service failure as the service tools show it: a message the
/// service worded (`CPFnnnn text`) as it is, anything else as [`fail`].
fn kept_or_fail(what: &'static str) -> impl FnOnce(String) -> String {
    move |e| {
        let id = e.split(' ').next().unwrap_or("");
        let worded = id.len() == 7 && id[..3].bytes().all(|b| b.is_ascii_uppercase());
        if worded { e } else { fail(what)(e) }
    }
}

/// The service tools' view of this system.
pub struct Env<'a> {
    pub(crate) system: &'a mut System,
    users: UserDb,
}

impl<'a> Env<'a> {
    /// Load the user IDs from the LIC record, shipping the defaults on
    /// a record that has none yet.
    pub fn new(system: &'a mut System) -> Self {
        let loaded = system
            .lic
            .as_ref()
            .and_then(|(_, record)| UserDb::load(record));
        let users = if let Some(u) = loaded {
            u
        } else {
            let now = now_unix();
            let mut salt = storage::fresh_uuid;
            let db = UserDb::defaults(now, &mut salt);
            system.out.serial(
                "slic: service tools user IDs initialized
",
            );
            db
        };
        let mut env = Self { system, users };
        if loaded_none(&env) {
            let _ = env.save_users();
        }
        env
    }

    /// The store, opened on demand on a D IPL.
    fn store(&mut self) -> Result<&mut store::Store, String> {
        if self.system.storage.is_none() {
            storage::locate_and_open(self.system).map_err(|s| s.text.to_string())?;
        }
        self.system
            .storage
            .as_mut()
            .map(|s| &mut s.store)
            .ok_or_else(|| svctools::dst::disk::config::NO_LOAD_SOURCE.to_string())
    }

    /// The objects of `types` in `QSYS`: name, `*TYPE`, text.
    fn objects_of(&mut self, codes: &[u16]) -> Vec<(String, String, String)> {
        let Some(s) = self.system.storage.as_mut() else {
            return Vec::new();
        };
        let Some(qsys) = s.qsys else {
            return Vec::new();
        };
        let r = crate::svc::db::with_txn(&mut s.store, |txn| {
            let mut out = Vec::new();
            for &code in codes {
                let ty = store::obj::types::type_by_code(code).map_or("", |t| t.name);
                for (key, id) in store::obj::ops::list(txn, qsys, Some(code)).map_err(fail("Listing the objects"))? {
                    let text = store::obj::ops::describe(txn, id)
                        .map(|h| ccsid::from_ebcdic(&h.text).trim_end().to_string())
                        .unwrap_or_default();
                    out.push((key.name.to_string().trim().to_string(), ty.to_string(), text));
                }
            }
            Ok(out)
        });
        r.unwrap_or_default()
    }

    fn resource_for(&self, device: &str) -> String {
        resource_for(self.system, device)
    }

    /// Where a unit's device sits in the list of configured devices.
    fn device_index(&self, uuid: [u8; 16]) -> Option<usize> {
        self.system.storage.as_ref().and_then(|s| {
            s.devices
                .iter()
                .position(|d| storage::unit_uuid_of(d) == Some(uuid))
        })
    }

    /// Drop a removed unit's device from the configured devices.
    fn forget_device(&mut self, at: Option<usize>) {
        if let (Some(s), Some(at)) = (self.system.storage.as_mut(), at) {
            s.devices.remove(at);
        }
    }
}

/// The disk resource name of a device node, or dashes.
fn resource_for(_system: &System, device: &str) -> String {
    let name = device.rsplit('/').next().unwrap_or(device);
    let disk =
        diskimg::locate::disk_of(std::path::Path::new(device)).unwrap_or_else(|| name.to_string());
    hw()
        .resources
        .iter()
        .find(|r| r.kind == Kind::Disk && r.location == disk)
        .map_or_else(|| "-----".to_string(), |r| r.name.clone())
}

/// The storage controller resource a device node's disk is attached
/// through, or dashes.
fn controller_for(_system: &System, device: &str) -> String {
    let name = device.rsplit('/').next().unwrap_or(device);
    let disk =
        diskimg::locate::disk_of(std::path::Path::new(device)).unwrap_or_else(|| name.to_string());
    hw().controller_of(&disk)
        .map_or_else(|| "-----".to_string(), |r| r.name.clone())
}

/// The hardware resource table from the service; empty when it cannot
/// be reached (the screens then show dashes).
fn hw() -> hwrsrc::Table {
    hwrsrc::client::table(false).unwrap_or_default()
}

/// Every disk the system sees, configured or not, for the installer.
pub fn disks(system: &System) -> Vec<NonConfigured> {
    let mut out = Vec::new();
    let media = crate::ipl::media::disk_on_d_ipl(system.boot.ipl_d);
    let others = crate::ipl::instdev::media_disks();
    for disk in diskimg::locate::whole_disks().unwrap_or_default() {
        if media.as_deref() == Some(disk.name.as_str()) || others.contains(&disk.name) {
            continue;
        }
        let device = disk.dev.display().to_string();
        let (type_model, serial) = diskimg::locate::identity(&disk.dev);
        out.push(NonConfigured {
            resource: resource_for(system, &device),
            controller: controller_for(system, &device),
            serial,
            type_model: if type_model.is_empty() {
                disk.model.clone()
            } else {
                type_model
            },
            size_mib: disk.bytes >> 20,
            has_data: disk.has_qs400 || storage::has_unit_header(&disk.dev),
            device,
        });
    }
    // In resource order, which follows the controllers and SCSI addresses.
    out.sort_by(|a, b| a.resource.cmp(&b.resource));
    out
}

/// Disks no disk function may touch: the LIC partition's, the units'
/// devices, the install media and any with a mounted file system.
pub(crate) fn reserved(system: &System) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(lic) = licpart::locate() {
        out.push(lic.display().to_string());
    }
    if let Some(s) = system.storage.as_ref() {
        out.extend(s.devices.iter().map(|d| d.display().to_string()));
    }
    let media = crate::ipl::media::disk_on_d_ipl(system.boot.ipl_d);
    out.extend(
        media
            .into_iter()
            .chain(crate::ipl::instdev::media_disks())
            .map(|n| format!("/dev/{n}")),
    );
    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        out.extend(
            mounts
                .lines()
                .filter_map(|l| l.split_whitespace().next())
                .filter(|src| src.starts_with("/dev/"))
                .map(str::to_string),
        );
    }
    // A partition reserves its whole disk too.
    let disks: Vec<String> = out
        .iter()
        .filter_map(|p| diskimg::locate::disk_of(std::path::Path::new(p)))
        .map(|d| format!("/dev/{d}"))
        .collect();
    out.extend(disks);
    out
}

fn loaded_none(env: &Env<'_>) -> bool {
    env.system
        .lic
        .as_ref()
        .is_some_and(|(_, record)| UserDb::load(record).is_none())
}

impl DstEnv for Env<'_> {
    fn system_name(&self) -> String {
        self.system.system_name()
    }

    fn now(&self) -> u64 {
        now_unix()
    }

    fn random(&mut self) -> [u8; 16] {
        storage::fresh_uuid()
    }

    fn users(&mut self) -> &mut UserDb {
        &mut self.users
    }

    fn audit(&mut self, kind: &str, code: &str, user: &str) {
        self.system
            .audit_lic
            .push((kind.into(), code.into(), user.into()));
    }

    fn save_users(&mut self) -> Result<(), String> {
        let Some((part, record)) = self.system.lic.as_mut() else {
            return Ok(()); // no partition: the defaults live for this IPL only
        };
        self.users.store(record).map_err(fail("Saving the service tools user IDs"))?;
        part.write(record).map(|_| ()).map_err(fail("Saving the service tools user IDs"))
    }

    fn disk_units(&mut self) -> Vec<DiskUnit> {
        let Ok(store) = self.store() else {
            return Vec::new();
        };
        let asp = store.asp_number();
        let infos = store.units();
        // Device parity members: IBM's DPY status, and the held share
        // left out of the use figure.
        let dpy: Vec<(u16, &'static str, u64)> = store
            .parity_sets()
            .iter()
            .flat_map(|s| s.members.iter().map(|m| (m.unit, m.status)))
            .map(|(u, st)| (u, st, store.asp().parity_held(u)))
            .collect();
        let resume = storage::resume_progress(self.system);
        // Devices by the unit UUID in their header: the order they were
        // found in says nothing about unit numbers.
        let devices: Vec<(Option<[u8; 16]>, String)> = self
            .system
            .storage
            .as_ref()
            .map(|s| {
                s.devices
                    .iter()
                    .map(|d| (storage::unit_uuid_of(d), d.display().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        infos
            .into_iter()
            .map(|u| {
                let e = &u.entry;
                let device = devices
                    .iter()
                    .find(|(uuid, _)| *uuid == Some(e.unit_uuid))
                    .map(|(_, d)| d.clone())
                    .unwrap_or_default();
                let (type_model, _) = diskimg::locate::identity(std::path::Path::new(&device));
                let parity = dpy.iter().find(|(n, _, _)| *n == u.unit);
                let held = parity.map_or(0, |p| p.2);
                let used = (u.used_pages.saturating_sub(held) * 100)
                    .checked_div(e.usable_pages.saturating_sub(held))
                    .map_or(0, |p| u8::try_from(p).unwrap_or(100));
                DiskUnit {
                    asp,
                    unit: u.unit,
                    serial: e.serial_text(),
                    type_model,
                    resource: self.resource_for(&device),
                    size_mib: e.pages * 4096 / (1024 * 1024),
                    percent_used: used,
                    status: parity.map_or_else(|| e.state.to_string(), |p| p.1.to_string()),
                    mirrored: e.mirrored(),
                    partner: e.mirrored().then_some(e.mirror_partner),
                    attached: u.attached,
                    resume_percent: resume.filter(|p| p.unit == u.unit).map(|p| p.percent),
                    controller: controller_for(self.system, &device),
                    device,
                }
            })
            .collect()
    }

    fn nonconfigured(&mut self) -> Vec<NonConfigured> {
        let configured: Vec<String> = self
            .system
            .storage
            .as_ref()
            .map(|s| s.devices.iter().map(|d| d.display().to_string()).collect())
            .unwrap_or_default();
        let mut out = Vec::new();
        let media = crate::ipl::media::disk_on_d_ipl(self.system.boot.ipl_d);
        for disk in diskimg::locate::whole_disks().unwrap_or_default() {
            if media.as_deref() == Some(disk.name.as_str()) {
                continue;
            }
            let device = disk.dev.display().to_string();
            let in_use = configured.iter().any(|c| {
                c == &device
                    || c.starts_with(&format!("{device}p"))
                    || (c.starts_with(&device)
                        && !disk.name.ends_with(|ch: char| ch.is_ascii_digit()))
            });
            if in_use || disk.has_qs400 {
                continue;
            }
            let (type_model, serial) = diskimg::locate::identity(&disk.dev);
            out.push(NonConfigured {
                resource: self.resource_for(&device),
                controller: controller_for(self.system, &device),
                serial,
                type_model: if type_model.is_empty() {
                    disk.model.clone()
                } else {
                    type_model
                },
                size_mib: disk.bytes >> 20,
                has_data: storage::has_unit_header(&disk.dev),
                device,
            });
        }
        out.sort_by(|a, b| a.resource.cmp(&b.resource));
        out
    }

    fn add_unit(&mut self, device: &str) -> Result<u16, String> {
        let (dev, serial) = open_unit(device)?;
        let store = self.store()?;
        let n = store
            .add_unit(dev, storage::fresh_uuid(), &serial)
            .map_err(refused("Adding the disk unit"))?;
        if let Some(s) = self.system.storage.as_mut() {
            s.devices.push(std::path::PathBuf::from(device));
        }
        Ok(n)
    }

    fn remove_unit(&mut self, unit: u16) -> Result<(), String> {
        let uuid = self
            .store()?
            .units()
            .get(usize::from(unit))
            .map(|u| u.entry.unit_uuid);
        let at = uuid.and_then(|u| self.device_index(u));
        self.store()?.remove_unit(unit).map_err(refused("Removing the disk unit"))?;
        self.forget_device(at);
        Ok(())
    }

    fn start_mirror(&mut self, unit: u16, device: &str) -> Result<u16, String> {
        // A configured partner leaves the configuration first; its data
        // moves to the other units. Devices are matched to units by the
        // UUID in their header, not by the order they were found in.
        let configured = self
            .system
            .storage
            .as_ref()
            .filter(|s| s.devices.iter().any(|d| d.display().to_string() == device))
            .and_then(|_| storage::unit_uuid_of(std::path::Path::new(device)))
            .and_then(|uuid| {
                self.system.storage.as_ref().and_then(|s| {
                    s.store
                        .units()
                        .iter()
                        .find(|u| u.entry.unit_uuid == uuid)
                        .map(|u| (u.unit, uuid))
                })
            });
        if let Some((n, uuid)) = configured {
            let at = self.device_index(uuid);
            self.store()?.remove_unit(n).map_err(refused("Starting mirrored protection"))?;
            self.forget_device(at);
        }
        let (dev, serial) = open_unit(device)?;
        let store = self.store()?;
        let new = store
            .start_mirror(unit, dev, storage::fresh_uuid(), &serial)
            .map_err(refused("Starting mirrored protection"))?;
        if let Some(s) = self.system.storage.as_mut() {
            s.devices.push(std::path::PathBuf::from(device));
        }
        Ok(new)
    }

    fn stop_mirror(&mut self, keep: u16) -> Result<(), String> {
        self.store()?
            .stop_mirror(keep)
            .map(|_| ())
            .map_err(refused("Stopping mirrored protection"))
    }

    fn suspend(&mut self, unit: u16) -> Result<(), String> {
        self.store()?.suspend_unit(unit).map_err(refused("Suspending the mirrored unit"))
    }

    fn resume(&mut self, unit: u16) -> Result<(), String> {
        self.store()?;
        storage::start_resume(self.system, unit).map(|_| ()).map_err(|e| match e {
            storage::ResumeRefused::Running => svctools::dst::disk::reasons::RESUME_RUNNING.to_string(),
            storage::ResumeRefused::Store(e) => refused("Resuming mirrored protection")(e),
            storage::ResumeRefused::NotOpen => fail("Resuming mirrored protection")("object store not open"),
        })
    }

    fn replace(&mut self, unit: u16, device: &str) -> Result<(), String> {
        let (dev, serial) = open_unit(device)?;
        let store = self.store()?;
        store
            .replace_unit(unit, dev, storage::fresh_uuid(), &serial)
            .map_err(refused("Replacing the unit"))?;
        if let Some(s) = self.system.storage.as_mut() {
            s.devices.push(std::path::PathBuf::from(device));
        }
        Ok(())
    }

    fn parity_units(&mut self) -> Vec<ParityUnit> {
        let controller = hw()
            .parity_capable()
            .first()
            .and_then(|l| l.split_whitespace().next().map(str::to_string))
            .unwrap_or_default();
        let units = self.disk_units();
        let Ok(store) = self.store() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for set in store.parity_sets() {
            for m in &set.members {
                let d = units.iter().find(|u| u.unit == m.unit);
                out.push(ParityUnit {
                    set: set.set,
                    level: set.level.to_string(),
                    unit: m.unit,
                    serial: d.map(|d| d.serial.clone()).unwrap_or_default(),
                    type_model: d.map(|d| d.type_model.clone()).unwrap_or_default(),
                    resource: d.map(|d| d.resource.clone()).unwrap_or_default(),
                    controller: controller.clone(),
                    status: m.status.to_string(),
                    percent: m.rebuild_percent.or(set.sync_percent),
                });
            }
        }
        out
    }

    fn start_parity(&mut self, level: u8, units: &[u16]) -> Result<String, String> {
        let level = if level == 6 { Level::Raid6 } else { Level::Raid5 };
        let set = self
            .store()?
            .start_parity(level, units)
            .map_err(refused("Starting device parity protection"))?;
        Ok(format!(
            "Device parity set {set} ({level}) started; parity is computed in the background"
        ))
    }

    fn stop_parity(&mut self, set: u8) -> Result<String, String> {
        self.store()?
            .stop_parity(set)
            .map_err(refused("Stopping device parity protection"))?;
        Ok(format!("Device parity protection stopped for set {set}"))
    }

    fn include_parity(&mut self, set: u8, unit: u16) -> Result<String, String> {
        self.store()?
            .include_parity(set, unit)
            .map_err(refused("Including the unit in device parity protection"))?;
        Ok(format!(
            "Unit {} included in set {set}; parity is computed again in the background",
            unit + 1
        ))
    }

    fn exclude_parity(&mut self, unit: u16) -> Result<String, String> {
        self.store()?
            .exclude_parity(unit)
            .map_err(refused("Excluding the unit from device parity protection"))?;
        Ok(format!("Unit {} excluded from device parity protection", unit + 1))
    }

    fn parity_capable(&mut self) -> Vec<String> {
        hw().parity_capable()
    }

    fn show(&mut self, screen: console::Screen) {
        menu::show(self.system, screen);
    }

    fn pace(&mut self) {
        if !self.system.pace.is_zero() {
            std::thread::sleep(self.system.pace);
        }
    }

    fn disks(&mut self) -> Vec<NonConfigured> {
        disks(self.system)
    }

    fn load_source(&mut self) -> Option<NonConfigured> {
        // The first LIC partition on a disk that is not install media.
        let all = disks(self.system);
        diskimg::locate::partitions_of_type(licpart::LIC_TYPE_GUID)
            .unwrap_or_default()
            .iter()
            .filter_map(|p| diskimg::locate::disk_of(p))
            .find_map(|disk| {
                let node = format!("/dev/{disk}");
                all.iter().find(|d| d.device == node).cloned()
            })
    }

    fn reserved_disks(&mut self) -> Vec<String> {
        reserved(self.system)
    }

    fn delete_unit_data(
        &mut self,
        device: &str,
        report: &mut dyn FnMut(u8, &mut dyn DstEnv),
    ) -> Result<(), String> {
        const WHAT: &str = "Deleting the disk unit data";
        // Only a disk the list offers, asked again now.
        let offered = svctools::dst::disk::delete::deletable(self)
            .iter()
            .any(|d| d.device == device);
        if !offered {
            return Err(svctools::dst::failed(WHAT));
        }
        let dev = blockdev::LinuxBlockDev::open(std::path::Path::new(device)).map_err(fail(WHAT))?;
        let mut last = None;
        let wiped = diskimg::wipe::wipe(&dev, &mut |p| {
            if last != Some(p) {
                last = Some(p);
                report(p, &mut *self);
            }
        })
        .map_err(fail(WHAT))?;
        drop(dev);
        self.system.out.serial(&format!(
            "slic: DST: data deleted on {device}: {} blocks zeroed, {} discarded, {} partitions\n",
            wiped.zeroed, wiped.discarded, wiped.partitions
        ));
        // The kernel forgets the old partitions; the resources follow.
        let file = std::fs::File::open(device).map_err(fail(WHAT))?;
        crate::platform::blk::reread_partitions(&file).map_err(fail(WHAT))?;
        if let Err(e) = hwrsrc::client::rescan() {
            eprintln!("slic: hardware resource rescan: {e}");
        }
        Ok(())
    }

    fn install_lic(
        &mut self,
        device: &str,
        auto: bool,
        report: &mut dyn FnMut(LicEvent, &mut dyn DstEnv),
    ) -> Result<(), String> {
        self.write_lic(device, auto, 1, report)
    }

    fn install_lic_as(
        &mut self,
        device: &str,
        kind: LicKind,
        auto: bool,
        report: &mut dyn FnMut(LicEvent, &mut dyn DstEnv),
    ) -> Result<(), String> {
        self.write_lic_as(device, kind, auto, report)
    }

    fn install_os(&mut self, report: &mut dyn FnMut(usize, &mut dyn DstEnv)) -> Result<(), String> {
        self.write_os(report)
    }

    fn install_devices(&mut self, kind: DeviceKind) -> Vec<InstallDevice> {
        let rel = self.store_release();
        crate::ipl::instdev::scan(rel, true);
        crate::ipl::instdev::devices(rel, kind)
    }

    fn install_os_from(
        &mut self,
        device: &InstallDevice,
        options: &OsOptions,
        report: &mut dyn FnMut(usize, &mut dyn DstEnv),
    ) -> Result<(), String> {
        self.write_os_from(device, options, report)
    }

    fn clock(&mut self) -> (String, String) {
        let (y, mo, d, h, mi, s) = crate::platform::rtc::unix_to_civil(now_unix());
        (
            format!("{y:04}-{mo:02}-{d:02}"),
            format!("{h:02}:{mi:02}:{s:02}"),
        )
    }

    fn set_clock(&mut self, date: &str, time: &str) -> Result<(), String> {
        let (now_date, now_time) = self.clock();
        if date == now_date && time == now_time {
            return Ok(());
        }
        let num = |s: &str, what: &str| {
            s.parse::<u32>()
                .map_err(|_| format!("{what} is not valid: {s}"))
        };
        let d: Vec<&str> = date.split('-').collect();
        let t: Vec<&str> = time.split(':').collect();
        if d.len() != 3 || t.len() != 3 {
            return Err("Date must be YYYY-MM-DD and time HH:MM:SS".into());
        }
        let year = i64::from(num(d[0], "year")?);
        let (month, day) = (num(d[1], "month")?, num(d[2], "day")?);
        let (hour, min, sec) = (
            num(t[0], "hour")?,
            num(t[1], "minute")?,
            num(t[2], "second")?,
        );
        if !(1..=12).contains(&month)
            || !(1..=31).contains(&day)
            || hour > 23
            || min > 59
            || sec > 59
        {
            return Err("Date or time is out of range".into());
        }
        let unix = crate::platform::rtc::civil_to_unix(year, month, day, hour, min, sec);
        // The time a panel showed a moment ago is not a change.
        if unix.abs_diff(now_unix()) < 300 {
            return Ok(());
        }
        crate::ipl::clock::set(unix).map_err(fail("Setting the clock"))?;
        self.system
            .out
            .serial(&format!("slic: clock set to {unix} UTC by the operator\n"));
        Ok(())
    }

    fn ipl_options(&mut self) -> IplOptions {
        self.system
            .lic
            .as_ref()
            .and_then(|(_, r)| r.tlv(TAG_IPL_OPTIONS))
            .and_then(IplOptions::decode)
            .unwrap_or_default()
    }

    fn console_mode(&self) -> ConsoleMode {
        crate::ipl::display::console_mode(self.system)
    }

    fn set_console_mode(&mut self, mode: ConsoleMode) -> Result<(), String> {
        let Some((part, record)) = self.system.lic.as_mut() else {
            return Err("The Licensed Internal Code record is not open".into());
        };
        record
            .set_tlv(TAG_CONSOLE_MODE, mode.encode())
            .map_err(fail("Changing the console mode"))?;
        part.write(record).map_err(fail("Changing the console mode"))?;
        self.system
            .out
            .serial(&format!("slic: console mode {mode:?} from the next IPL\n"));
        Ok(())
    }

    fn lic_info(&mut self) -> Vec<(String, String)> {
        let mut info = vec![
            (
                "Release".to_string(),
                release::full()
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string(),
            ),
            ("Build".to_string(), release::build_id().to_string()),
        ];
        if let Some((_, r)) = self.system.lic.as_ref() {
            let h = &r.header;
            let (y, mo, d, hh, mm, ss) = crate::platform::rtc::unix_to_civil(h.install_time);
            info.push(("Slot of this IPL".into(), format!("{:?}", self.system.boot.slot)));
            info.push(("Last good slot".into(), format!("{:?}", h.last_good_slot)));
            info.push((
                "IPLs".into(),
                format!("{} ({} abnormal)", h.ipl_count, h.abnormal_count),
            ));
            info.push((
                "Installed".into(),
                format!("{y:04}-{mo:02}-{d:02} {hh:02}:{mm:02}:{ss:02}"),
            ));
        }
        info.extend(crate::ipl::ifs::lic_info());
        info
    }

    fn set_ipl_options(&mut self, options: &IplOptions) -> Result<(), String> {
        if let Some((part, record)) = self.system.lic.as_mut() {
            record
                .set_tlv(TAG_IPL_OPTIONS, options.encode())
                .map_err(fail("Changing the IPL options"))?;
            part.write(record).map_err(fail("Changing the IPL options"))?;
        }
        if let Some(vals) = self.system.sysvals.as_mut() {
            let writers = if options.start_print_writers {
                "1"
            } else {
                "0"
            };
            vals.set_text("QSTRPRTWTR", writers)
                .map_err(fail("Changing the IPL options"))?;
            crate::ipl::sysvals::save(self.system).map_err(fail("Changing the IPL options"))?;
        }
        Ok(())
    }

    fn major_options(&mut self) -> MajorOptions {
        let Some(vals) = self.system.sysvals.as_ref() else {
            return MajorOptions::default();
        };
        let get = |n: &str| vals.get(n).map(|v| v.text()).unwrap_or_default();
        MajorOptions {
            auto_configure: get("QAUTOCFG") != "0",
            device_naming: get("QDEVNAMING"),
            special_environment: get("QSPCENV"),
        }
    }

    fn set_major_options(&mut self, options: &MajorOptions) -> Result<(), String> {
        let Some(vals) = self.system.sysvals.as_mut() else {
            return Err("System values are not loaded yet".into());
        };
        let auto = if options.auto_configure { "1" } else { "0" };
        vals.set_text("QAUTOCFG", auto).map_err(fail("Changing the major system options"))?;
        vals.set_text("QDEVNAMING", &options.device_naming)
            .map_err(fail("Changing the major system options"))?;
        vals.set_text("QSPCENV", &options.special_environment)
            .map_err(fail("Changing the major system options"))?;
        crate::ipl::sysvals::save(self.system).map_err(fail("Changing the major system options"))
    }

    fn lic_sysvals(&mut self) -> Vec<(String, String)> {
        let host = crate::ipl::sysvals::host_info();
        let Some((_, record)) = self.system.lic.as_ref() else {
            return Vec::new();
        };
        let Ok(vals) = SysVals::from_record(record, &host) else {
            return Vec::new();
        };
        IPL_NAMES
            .iter()
            .map(|n| {
                (
                    (*n).to_string(),
                    vals.get(n).map(|v| v.text()).unwrap_or_default(),
                )
            })
            .collect()
    }

    fn set_lic_sysval(&mut self, name: &str, value: &str) -> Result<(), String> {
        let host = crate::ipl::sysvals::host_info();
        let Some((part, record)) = self.system.lic.as_mut() else {
            return Err("LIC partition not attached".into());
        };
        if !IPL_NAMES.contains(&name) {
            return Err(format!("{name} is not a system value the LIC keeps"));
        }
        let mut vals = SysVals::from_record(record, &host).map_err(fail("Changing the system value"))?;
        vals.set_text(name, value).map_err(|_| svctools::dst::value_not_valid(name, value))?;
        vals.store_ipl(record).map_err(fail("Changing the system value"))?;
        part.write(record).map(|_| ()).map_err(fail("Changing the system value"))
    }

    fn read_page(&mut self, unit: u16, lba: u64) -> Result<Vec<u8>, String> {
        self.store()?
            .read_unit_page(unit, lba)
            .map_err(fail("Reading the page"))
    }

    fn object_header(&mut self, id: &str) -> Result<Vec<u8>, String> {
        let id: store::obj::ObjectId = id.parse().map_err(fail("Reading the object header"))?;
        self.store()?
            .object_header_page(id)
            .map_err(fail("Reading the object header"))
    }

    fn security_flags(&mut self) -> SecurityFlags {
        self.system
            .lic
            .as_ref()
            .and_then(|(_, r)| r.tlv(TAG_SECURITY))
            .and_then(|b| b.first().copied())
            .map(decode_flags)
            .unwrap_or_default()
    }

    fn set_security_flags(&mut self, flags: SecurityFlags) -> Result<(), String> {
        let Some((part, record)) = self.system.lic.as_mut() else {
            return Err("LIC partition not attached".into());
        };
        record
            .set_tlv(TAG_SECURITY, vec![encode_flags(flags)])
            .map_err(fail("Changing the security options"))?;
        part.write(record).map(|_| ()).map_err(fail("Changing the security options"))
    }

    fn activity_log(&mut self) -> Vec<LogEntry> {
        self.system
            .lic
            .as_ref()
            .map(|(_, record)| {
                record
                    .pal
                    .iter()
                    .map(|e| LogEntry {
                        time: e.time,
                        code: e.code_str(),
                        text: {
                            let t = src::lookup(&e.code_str()).map_or("", |s| s.text);
                            if e.resource.is_empty() {
                                t.to_string()
                            } else {
                                format!("{t}: {}", e.resource)
                            }
                        },
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn lic_log(&mut self) -> Vec<String> {
        self.system
            .log
            .dump()
            .lines()
            .map(ToString::to_string)
            .collect()
    }

    fn capacity(&mut self) -> Vec<(String, String)> {
        let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        let units = self.disk_units();
        let mib: u64 = units.iter().map(|u| u.size_mib).sum();
        vec![
            ("Installed processors".into(), cpus.to_string()),
            ("Disk units configured".into(), units.len().to_string()),
            ("System ASP capacity".into(), format!("{} G", mib / 1024)),
            ("ASP threshold, %".into(), self.asp_threshold().to_string()),
        ]
    }

    fn dumps(&mut self) -> Result<Vec<svctools::env::DumpRow>, String> {
        self.store()?;
        Ok(crate::stgsvc::dumps::list(self.system).map_err(kept_or_fail("Listing the system dumps"))?
            .into_iter()
            .map(|d| svctools::env::DumpRow {
                name: d.name,
                taken: d.taken,
                size: d.size,
                text: d.text,
            })
            .collect())
    }

    fn take_dump(&mut self, why: &str) -> Result<String, String> {
        self.store()?;
        crate::stgsvc::dumps::take(self.system, why).map_err(kept_or_fail("Taking the system dump"))
    }

    fn dump_lines(&mut self, name: &str) -> Result<Vec<String>, String> {
        self.store()?;
        crate::stgsvc::dumps::show(self.system, name).map_err(kept_or_fail("Showing the system dump"))
    }

    fn delete_dump(&mut self, name: &str) -> Result<(), String> {
        self.store()?;
        crate::stgsvc::dumps::delete(self.system, name).map_err(kept_or_fail("Deleting the system dump"))
    }

    fn all_sysvals(&mut self) -> Vec<(String, String, String)> {
        let Some(vals) = self.system.sysvals.as_ref() else {
            return Vec::new();
        };
        vals.catalog()
            .entries()
            .map(|e| {
                let v = vals.get(&e.name).map(|v| v.text()).unwrap_or_default();
                (e.name.clone(), v, e.text.clone())
            })
            .collect()
    }

    fn set_sysval(&mut self, name: &str, value: &str) -> Result<String, String> {
        let name = name.trim().to_ascii_uppercase();
        let vals = self.system.sysvals.as_mut().ok_or_else(|| svctools::dst::NOT_LOADED.to_string())?;
        vals.set_text(&name, value).map_err(|_| svctools::dst::value_not_valid(&name, value))?;
        crate::ipl::sysvals::save(self.system).map_err(fail("Changing the system value"))?;
        Ok(format!("System value {name} changed"))
    }

    fn network_attributes(&mut self) -> Vec<(String, String)> {
        crate::stgsvc::sysdata::neta(self.system).unwrap_or_default()
    }

    fn config_objects(&mut self) -> Vec<(String, String, String)> {
        // Device, controller and line descriptions.
        self.objects_of(&[store::obj::types::DEVD, 0x001C, 0x001D])
    }

    fn user_profiles(&mut self) -> Vec<(String, String)> {
        self.objects_of(&[store::obj::types::USRPRF])
            .into_iter()
            .map(|(n, _, d)| (n, d))
            .collect()
    }

    fn lic_events(&mut self, major: u16) -> Vec<String> {
        crate::stgsvc::liclog::entries(major)
            .iter()
            .map(crate::stgsvc::liclog::line)
            .collect()
    }

    fn resources(&mut self) -> Vec<ResourceRow> {
        self.system
            .hw()
            .resources
            .iter()
            .map(|r| ResourceRow {
                name: r.name.clone(),
                kind: format!("{:?}", r.kind),
                status: format!("{:?}", r.status),
                location: r.location.clone(),
                description: r.description.clone(),
            })
            .collect()
    }
}

/// Open a device node for a new unit, with its serial from sysfs.
fn open_unit(device: &str) -> Result<(Box<dyn blockdev::BlockDev>, String), String> {
    let p = std::path::Path::new(device);
    let dev = blockdev::LinuxBlockDev::open(p).map_err(|e| {
        eprintln!("slic: opening the disk unit {device}: {e}");
        svctools::dst::disk::reasons::DISK_NOT_OPENED.to_string()
    })?;
    let (_, serial) = diskimg::locate::identity(p);
    Ok((Box::new(dev), serial))
}

/// Run DST until it ends: sign-on, menus, and whatever the operator
/// does in them.
pub fn run(system: &mut System, rx: &Receiver<Event>) -> Outcome {
    let mut runner = svctools::dst::start();
    let mut env = Env::new(system);
    loop {
        let screen = runner.screen(&mut env);
        menu::show(env.system, screen);
        let Some(answer) = menu::read(env.system, rx) else {
            return Outcome::Exit;
        };
        let outcome = match answer {
            Answer::Enter(fields) => runner.enter(&fields, &mut env),
            Answer::Function(n) => runner.function(n, &mut env),
        };
        if let Some(o) = outcome {
            env.system.out.serial(&format!("slic: DST ended: {o:?}\n"));
            return o;
        }
    }
}
