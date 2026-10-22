// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the service tools ask of the system they run in. `slic`
//! implements this against the LIC partition, the units and the
//! hardware table; tests use [`MemEnv`]. Methods a later phase fills in
//! have defaults that report nothing, so panels can land before their
//! data does.

use console::Screen;

use crate::install::lic::LicEvent;
use crate::install::options::{IplOptions, MajorOptions};
use crate::users::UserDb;

/// A configured disk unit, as the disk configuration displays show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskUnit {
    /// ASP number.
    pub asp: u16,
    /// Unit number.
    pub unit: u16,
    /// Serial number.
    pub serial: String,
    /// Type and model from the device.
    pub type_model: String,
    /// Resource name, `DD001`.
    pub resource: String,
    /// Size in MiB.
    pub size_mib: u64,
    /// Percent of the span in use.
    pub percent_used: u8,
    /// `Active`, `Suspended`, ... or `Unprotected`.
    pub status: String,
    /// Whether the unit is half of a pair.
    pub mirrored: bool,
    /// The other half.
    pub partner: Option<u16>,
    /// Whether the device is attached now.
    pub attached: bool,
    /// Percent of a resume done, while one runs on this unit.
    pub resume_percent: Option<u8>,
    /// Device node, for pairing an added unit.
    pub device: String,
}

/// A disk the system sees that belongs to no ASP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonConfigured {
    /// Device node.
    pub device: String,
    /// Resource name.
    pub resource: String,
    /// Serial number.
    pub serial: String,
    /// Type and model.
    pub type_model: String,
    /// Size in MiB.
    pub size_mib: u64,
    /// Whether it carries QS/400 partitions or a unit header.
    pub has_data: bool,
}

/// One hardware resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRow {
    /// Resource name.
    pub name: String,
    /// Kind, as the hardware table names it.
    pub kind: String,
    /// `Operational` or `Not detected`.
    pub status: String,
    /// PCI address or device name.
    pub location: String,
    /// Description.
    pub description: String,
}

/// One product activity log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Unix seconds.
    pub time: u64,
    /// The reference code.
    pub code: String,
    /// Its text.
    pub text: String,
}

/// The `Work with system security` flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)] // one flag per line of the display
pub struct SecurityFlags {
    /// A user with a default and expired password may change it.
    pub allow_default_password_change: bool,
    /// New digital certificates may be added.
    pub allow_new_certificates: bool,
    /// Security system values may be changed.
    pub allow_sysval_security_changes: bool,
    /// Certificates may be added.
    pub allow_add_certificates: bool,
}

/// The system behind the service tools.
pub trait DstEnv {
    /// The system name for the top line.
    fn system_name(&self) -> String;
    /// Unix seconds.
    fn now(&self) -> u64;
    /// Sixteen random bytes (salts, identities).
    fn random(&mut self) -> [u8; 16];
    /// The service tools user IDs, in memory.
    fn users(&mut self) -> &mut UserDb;
    /// Make the user IDs durable.
    fn save_users(&mut self) -> Result<(), String>;

    /// Something the security audit journal records: `kind` `ST` (a
    /// sign-on, `code` `DS`), `DS` (a password reset) or `PW` (`X`, a
    /// service tools user ID or password not valid).
    fn audit(&mut self, _kind: &str, _code: &str, _user: &str) {}

    /// The configured disk units.
    fn disk_units(&mut self) -> Vec<DiskUnit> {
        Vec::new()
    }
    /// Disks that belong to no ASP.
    fn nonconfigured(&mut self) -> Vec<NonConfigured> {
        Vec::new()
    }
    /// Add a non-configured disk to ASP 1 as an unprotected unit.
    fn add_unit(&mut self, device: &str) -> Result<u16, String> {
        let _ = device;
        Err("not available".into())
    }
    /// Remove a unit from ASP 1.
    fn remove_unit(&mut self, unit: u16) -> Result<(), String> {
        let _ = unit;
        Err("not available".into())
    }
    /// Pair a unit with a non-configured disk; the copy runs at the
    /// next IPL.
    fn start_mirror(&mut self, unit: u16, device: &str) -> Result<u16, String> {
        let _ = (unit, device);
        Err("not available".into())
    }
    /// Dissolve a pair, keeping `keep`.
    fn stop_mirror(&mut self, keep: u16) -> Result<(), String> {
        let _ = keep;
        Err("not available".into())
    }
    /// Stop writing to a half.
    fn suspend(&mut self, unit: u16) -> Result<(), String> {
        let _ = unit;
        Err("not available".into())
    }
    /// Bring a half back in step, in the background.
    fn resume(&mut self, unit: u16) -> Result<(), String> {
        let _ = unit;
        Err("not available".into())
    }
    /// Put a non-configured disk in the place of a missing half.
    fn replace(&mut self, unit: u16, device: &str) -> Result<(), String> {
        let _ = (unit, device);
        Err("not available".into())
    }
    /// Show a screen while a long action runs.
    fn show(&mut self, screen: Screen) {
        let _ = screen;
    }
    /// Every whole disk, configured or not, for the load source choice.
    fn disks(&mut self) -> Vec<NonConfigured> {
        self.nonconfigured()
    }
    /// Install the Licensed Internal Code on `device`: partition it,
    /// initialize the LIC partition, copy both slots, write the record
    /// marking the operating system install pending (automatic when
    /// `auto`). `report` is called as the work proceeds.
    fn install_lic(
        &mut self,
        device: &str,
        auto: bool,
        report: &mut dyn FnMut(LicEvent, &mut dyn DstEnv),
    ) -> Result<(), String> {
        let _ = (device, auto, report);
        Err("not available".into())
    }
    /// Initialize ASP 1 and create the system libraries and values;
    /// `report` is called with each step of `OS_STEPS` as it completes.
    fn install_os(&mut self, report: &mut dyn FnMut(usize, &mut dyn DstEnv)) -> Result<(), String> {
        let _ = report;
        Err("not available".into())
    }
    /// The clock as `YYYY-MM-DD` and `HH:MM:SS`.
    fn clock(&mut self) -> (String, String) {
        ("2026-01-01".into(), "00:00:00".into())
    }
    /// Set the clock from `YYYY-MM-DD` and `HH:MM:SS`.
    fn set_clock(&mut self, date: &str, time: &str) -> Result<(), String> {
        let _ = (date, time);
        Ok(())
    }
    /// The IPL options.
    fn ipl_options(&mut self) -> IplOptions {
        IplOptions::default()
    }
    /// Record the IPL options.
    fn set_ipl_options(&mut self, options: &IplOptions) -> Result<(), String> {
        let _ = options;
        Ok(())
    }
    /// The major system options.
    fn major_options(&mut self) -> MajorOptions {
        MajorOptions::default()
    }
    /// Record the major system options.
    fn set_major_options(&mut self, options: &MajorOptions) -> Result<(), String> {
        let _ = options;
        Ok(())
    }
    /// Controllers that provide device parity protection, one line
    /// each. None in this release: hardware parity waits on a later
    /// phase, software parity is out of scope.
    fn parity_capable(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// The ASP threshold percent.
    fn asp_threshold(&mut self) -> u8 {
        90
    }
    /// Change the ASP threshold.
    fn set_asp_threshold(&mut self, percent: u8) -> Result<(), String> {
        let _ = percent;
        Err("not available".into())
    }

    /// The system values the LIC keeps, name and value.
    fn lic_sysvals(&mut self) -> Vec<(String, String)> {
        Vec::new()
    }
    /// Change one of them.
    fn set_lic_sysval(&mut self, name: &str, value: &str) -> Result<(), String> {
        let _ = (name, value);
        Err("not available".into())
    }
    /// The console in use.
    fn console_type(&self) -> String {
        "Local console".into()
    }
    /// The product activity log, newest last.
    fn activity_log(&mut self) -> Vec<LogEntry> {
        Vec::new()
    }
    /// The LIC log: this IPL's codes and recovery lines.
    fn lic_log(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// A page of a unit, for display.
    fn read_page(&mut self, unit: u16, lba: u64) -> Result<Vec<u8>, String> {
        let _ = (unit, lba);
        Err("not available".into())
    }
    /// An object header by ID text, for display.
    fn object_header(&mut self, id: &str) -> Result<Vec<u8>, String> {
        let _ = id;
        Err("not available".into())
    }
    /// The hardware resource table.
    fn resources(&mut self) -> Vec<ResourceRow> {
        Vec::new()
    }
    /// The security flags.
    fn security_flags(&mut self) -> SecurityFlags {
        SecurityFlags::default()
    }
    /// Change the security flags.
    fn set_security_flags(&mut self, flags: SecurityFlags) -> Result<(), String> {
        let _ = flags;
        Err("not available".into())
    }
}

/// An environment in memory, for tests and golden screens.
#[derive(Default)]
pub struct MemEnv {
    /// System name.
    pub name: String,
    /// The clock.
    pub time: u64,
    /// The users.
    pub users: UserDb,
    /// Times `save_users` was called.
    pub saves: u32,
    /// Units to report.
    pub units: Vec<DiskUnit>,
    /// Disks to report.
    pub spare: Vec<NonConfigured>,
    /// Resources to report.
    pub hardware: Vec<ResourceRow>,
    /// Log to report.
    pub log: Vec<LogEntry>,
    /// Flags.
    pub flags: SecurityFlags,
    /// Actions taken, for assertions.
    pub actions: Vec<String>,
    /// Screens shown during long actions, as dumps.
    pub shown: Vec<String>,
    seed: u64,
}

impl MemEnv {
    /// With the default users and a name.
    #[must_use]
    pub fn new(name: &str) -> Self {
        let mut e = Self {
            name: name.to_string(),
            time: 1_767_225_600,
            ..Self::default()
        };
        let mut salt = || e.random();
        e.users = UserDb::defaults(1_767_225_600, &mut salt);
        e
    }
}

impl DstEnv for MemEnv {
    fn system_name(&self) -> String {
        if self.name.is_empty() {
            "QS400".into()
        } else {
            self.name.clone()
        }
    }
    fn now(&self) -> u64 {
        self.time
    }
    fn random(&mut self) -> [u8; 16] {
        let mut out = [0u8; 16];
        for b in &mut out {
            self.seed = self
                .seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            *b = (self.seed >> 56) as u8;
        }
        out
    }
    fn users(&mut self) -> &mut UserDb {
        &mut self.users
    }
    fn save_users(&mut self) -> Result<(), String> {
        self.saves += 1;
        Ok(())
    }
    fn disk_units(&mut self) -> Vec<DiskUnit> {
        self.units.clone()
    }
    fn nonconfigured(&mut self) -> Vec<NonConfigured> {
        self.spare.clone()
    }
    fn add_unit(&mut self, device: &str) -> Result<u16, String> {
        self.actions.push(format!("add {device}"));
        Ok(u16::try_from(self.units.len()).expect("few"))
    }
    fn remove_unit(&mut self, unit: u16) -> Result<(), String> {
        self.actions.push(format!("remove {unit}"));
        Ok(())
    }
    fn start_mirror(&mut self, unit: u16, device: &str) -> Result<u16, String> {
        self.actions.push(format!("mirror {unit} {device}"));
        Ok(u16::try_from(self.units.len()).expect("few"))
    }
    fn stop_mirror(&mut self, keep: u16) -> Result<(), String> {
        self.actions.push(format!("stop {keep}"));
        Ok(())
    }
    fn suspend(&mut self, unit: u16) -> Result<(), String> {
        self.actions.push(format!("suspend {unit}"));
        Ok(())
    }
    fn resume(&mut self, unit: u16) -> Result<(), String> {
        self.actions.push(format!("resume {unit}"));
        Ok(())
    }
    fn replace(&mut self, unit: u16, device: &str) -> Result<(), String> {
        self.actions.push(format!("replace {unit} {device}"));
        Ok(())
    }
    fn set_asp_threshold(&mut self, percent: u8) -> Result<(), String> {
        self.actions.push(format!("threshold {percent}"));
        Ok(())
    }
    fn show(&mut self, screen: Screen) {
        self.shown.push(screen.dump());
    }
    fn install_lic(
        &mut self,
        device: &str,
        auto: bool,
        report: &mut dyn FnMut(LicEvent, &mut dyn DstEnv),
    ) -> Result<(), String> {
        self.actions
            .push(format!("install-lic {device} auto={auto}"));
        for p in [0, 50, 100] {
            report(LicEvent::DiskPercent(p), self);
        }
        for i in 0..crate::install::LIC_STEPS.len() {
            report(LicEvent::StepDone(i), self);
        }
        Ok(())
    }
    fn install_os(&mut self, report: &mut dyn FnMut(usize, &mut dyn DstEnv)) -> Result<(), String> {
        self.actions.push("install-os".into());
        for i in 0..crate::install::OS_STEPS.len() {
            report(i, self);
        }
        Ok(())
    }
    fn set_clock(&mut self, date: &str, time: &str) -> Result<(), String> {
        self.actions.push(format!("clock {date} {time}"));
        Ok(())
    }
    fn set_ipl_options(&mut self, options: &IplOptions) -> Result<(), String> {
        self.actions
            .push(format!("ipl-options {:?}", options.encode()));
        Ok(())
    }
    fn set_major_options(&mut self, options: &MajorOptions) -> Result<(), String> {
        self.actions.push(format!(
            "major {} {} {}",
            if options.auto_configure { "Y" } else { "N" },
            options.device_naming,
            options.special_environment
        ));
        Ok(())
    }
    fn activity_log(&mut self) -> Vec<LogEntry> {
        self.log.clone()
    }
    fn resources(&mut self) -> Vec<ResourceRow> {
        self.hardware.clone()
    }
    fn security_flags(&mut self) -> SecurityFlags {
        self.flags
    }
    fn set_security_flags(&mut self, flags: SecurityFlags) -> Result<(), String> {
        self.flags = flags;
        Ok(())
    }
}
