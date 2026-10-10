// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dedicated Service Tools on this system: the environment the
//! `svctools` panels talk to, and the loop that shows them.

use std::sync::mpsc::Receiver;

pub use svctools::Outcome;
use svctools::dst::security::{TAG_SECURITY, decode as decode_flags, encode as encode_flags};
use svctools::env::{DiskUnit, LogEntry, NonConfigured, ResourceRow, SecurityFlags};
use svctools::{DstEnv, UserDb};
use sysval::{IPL_NAMES, SysVals};

use crate::hwrsrc::{Kind, ParityCapable};
use crate::ipl::menu::{self, Answer};
use crate::ipl::system::{System, now_unix};
use crate::ipl::{Event, storage};
use crate::src;

/// The service tools' view of this system.
pub struct Env<'a> {
    system: &'a mut System,
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
            .ok_or_else(|| "no ASP 1 unit".to_string())
    }

    fn resource_for(&self, device: &str) -> String {
        let name = device.rsplit('/').next().unwrap_or(device);
        let disk = diskimg::locate::disk_of(std::path::Path::new(device))
            .unwrap_or_else(|| name.to_string());
        self.system
            .hw
            .resources
            .iter()
            .find(|r| r.kind == Kind::Disk && r.location == disk)
            .map_or_else(|| "-----".to_string(), |r| r.name.clone())
    }
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

    fn save_users(&mut self) -> Result<(), String> {
        let Some((part, record)) = self.system.lic.as_mut() else {
            return Ok(()); // no partition: the defaults live for this IPL only
        };
        self.users.store(record).map_err(|e| e.to_string())?;
        part.write(record).map(|_| ()).map_err(|e| e.to_string())
    }

    fn disk_units(&mut self) -> Vec<DiskUnit> {
        let Ok(store) = self.store() else {
            return Vec::new();
        };
        let asp = store.asp_number();
        let infos = store.units();
        let resume = storage::resume_progress(self.system);
        let devices: Vec<String> = self
            .system
            .storage
            .as_ref()
            .map(|s| s.devices.iter().map(|d| d.display().to_string()).collect())
            .unwrap_or_default();
        infos
            .into_iter()
            .map(|u| {
                let e = &u.entry;
                let device = devices
                    .get(usize::from(u.unit))
                    .cloned()
                    .unwrap_or_default();
                let (type_model, _) = diskimg::locate::identity(std::path::Path::new(&device));
                let used = (u.used_pages * 100)
                    .checked_div(e.usable_pages)
                    .map_or(0, |p| u8::try_from(p).unwrap_or(100));
                DiskUnit {
                    asp,
                    unit: u.unit,
                    serial: e.serial_text(),
                    type_model,
                    resource: self.resource_for(&device),
                    size_mib: e.pages * 4096 / (1024 * 1024),
                    percent_used: used,
                    status: e.state.to_string(),
                    mirrored: e.mirrored(),
                    partner: e.mirrored().then_some(e.mirror_partner),
                    attached: u.attached,
                    resume_percent: resume.filter(|p| p.unit == u.unit).map(|p| p.percent),
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
        for disk in diskimg::locate::whole_disks().unwrap_or_default() {
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
        out
    }

    fn add_unit(&mut self, device: &str) -> Result<u16, String> {
        let (dev, serial) = open_unit(device)?;
        let store = self.store()?;
        store
            .add_unit(dev, storage::fresh_uuid(), &serial)
            .map_err(|e| e.to_string())
    }

    fn remove_unit(&mut self, unit: u16) -> Result<(), String> {
        self.store()?
            .remove_unit(unit)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn start_mirror(&mut self, unit: u16, device: &str) -> Result<u16, String> {
        let (dev, serial) = open_unit(device)?;
        let store = self.store()?;
        store
            .start_mirror(unit, dev, storage::fresh_uuid(), &serial)
            .map_err(|e| e.to_string())
    }

    fn stop_mirror(&mut self, keep: u16) -> Result<(), String> {
        self.store()?
            .stop_mirror(keep)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn suspend(&mut self, unit: u16) -> Result<(), String> {
        self.store()?.suspend_unit(unit).map_err(|e| e.to_string())
    }

    fn resume(&mut self, unit: u16) -> Result<(), String> {
        self.store()?;
        storage::start_resume(self.system, unit).map(|_| ())
    }

    fn replace(&mut self, unit: u16, device: &str) -> Result<(), String> {
        let (dev, serial) = open_unit(device)?;
        let store = self.store()?;
        store
            .replace_unit(unit, dev, storage::fresh_uuid(), &serial)
            .map_err(|e| e.to_string())
    }

    fn parity_capable(&mut self) -> Vec<String> {
        self.system.hw.parity_capable()
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
        let mut vals = SysVals::from_record(record, &host).map_err(|e| e.to_string())?;
        vals.set_text(name, value).map_err(|e| e.to_string())?;
        vals.store_ipl(record).map_err(|e| e.to_string())?;
        part.write(record).map(|_| ()).map_err(|e| e.to_string())
    }

    fn read_page(&mut self, unit: u16, lba: u64) -> Result<Vec<u8>, String> {
        self.store()?
            .read_unit_page(unit, lba)
            .map_err(|e| e.to_string())
    }

    fn object_header(&mut self, id: &str) -> Result<Vec<u8>, String> {
        let id: store::obj::ObjectId = id.parse()?;
        self.store()?
            .object_header_page(id)
            .map_err(|e| e.to_string())
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
            .map_err(|e| e.to_string())?;
        part.write(record).map(|_| ()).map_err(|e| e.to_string())
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
                        text: src::lookup(&e.code_str())
                            .map_or("", |s| s.text)
                            .to_string(),
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

    fn resources(&mut self) -> Vec<ResourceRow> {
        self.system
            .hw
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
    let dev = blockdev::LinuxBlockDev::open(p).map_err(|e| format!("open {device}: {e}"))?;
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
