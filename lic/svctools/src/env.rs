// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the service tools ask of the system they run in. `slic`
//! implements this against the LIC partition, the units and the
//! hardware table; tests use [`MemEnv`]. Methods a later phase fills in
//! have defaults that report nothing, so panels can land before their
//! data does.

use console::Screen;

use crate::dst::console_mode::ConsoleMode;
use crate::install::lic::{LicEvent, LicKind};
use crate::install::options::{IplOptions, MajorOptions};
use crate::install::os::{DeviceKind, InstallDevice, OsOptions};
use crate::dst::signon::message;
use crate::users::{Privilege, SignOnError, UserDb};

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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// The storage controller resource it is attached through (`DC01`).
    pub controller: String,
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

/// A system dump the main storage dump manager keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpRow {
    /// `QSDnnnnnnn`.
    pub name: String,
    /// Unix seconds taken.
    pub taken: u64,
    /// Bytes.
    pub size: u64,
    /// Why it was taken.
    pub text: String,
}

/// A trace the service tools started or see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceRow {
    /// Table or session name.
    pub name: String,
    /// `LIC` or `Job`.
    pub kind: String,
    /// `Active` or `Stopped`.
    pub status: String,
    /// Records held.
    pub records: u64,
}

/// What "start a trace" asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceSpec {
    /// Table name.
    pub name: String,
    /// Categories, blank-separated; `*ALL` or empty for all.
    pub categories: String,
    /// Job number; 0 for every job.
    pub job: u32,
    /// Size in KiB.
    pub size_kib: u64,
    /// Wrap when full.
    pub wrap: bool,
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

/// A change to the service tools user IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserChange {
    /// A new user with an expired password and no privileges.
    Create {
        /// Name.
        name: String,
        /// Password.
        password: String,
        /// Description.
        text: String,
    },
    /// Delete a user.
    Delete {
        /// Name.
        name: String,
    },
    /// Enable or disable a user.
    Enable {
        /// Name.
        name: String,
        /// Enable it.
        enable: bool,
    },
    /// Replace a user's privileges.
    Privileges {
        /// Name.
        name: String,
        /// The new set.
        privileges: Privilege,
    },
    /// A new, expired password.
    Reset {
        /// Name.
        name: String,
        /// Password.
        password: String,
    },
}

impl UserChange {
    /// Apply the change to `db`.
    pub fn apply(&self, db: &mut UserDb, now: u64, salt: [u8; 16]) -> Result<(), SignOnError> {
        match self {
            Self::Create {
                name,
                password,
                text,
            } => db.create(name, password, Privilege::NONE, text, now, salt),
            Self::Delete { name } => db.delete(name),
            Self::Enable { name, enable } => db.set_enabled(name, *enable),
            Self::Privileges { name, privileges } => db.set_privileges(name, *privileges),
            Self::Reset { name, password } => db.reset_password(name, password, now, salt),
        }
    }
}

/// What a function that needs the running system says in DST.
pub const NOT_HERE: &str = "Not available until the system is running; use STRSST";

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

    /// Let a progress display stay up long enough to be read; the
    /// machine's pace, nothing in memory.
    fn pace(&mut self) {}

    /// Whether a change to disks or user IDs shows a confirmation first
    /// (SST does; DST changes at once where it always has).
    fn confirm_changes(&self) -> bool {
        false
    }

    /// Sign a service tools user on: the privileges, or why not. Every
    /// attempt is saved (failure counts) and audited.
    fn sign_on(&mut self, user: &str, password: &str) -> Result<Privilege, SignOnError> {
        let result = self.users().sign_on(user, password).map(|u| u.privileges);
        // Failure counts and disables are durable.
        let _ = self.save_users();
        match &result {
            Ok(_) => self.audit("ST", "DS", user),
            Err(SignOnError::NotCorrect) => self.audit("PW", "X", user),
            Err(_) => {}
        }
        result
    }

    /// Change an expired password; the user's privileges once done, or
    /// the message text of a refusal.
    fn change_password(&mut self, user: &str, old: &str, new: &str) -> Result<Privilege, String> {
        let now = self.now();
        let salt = self.random();
        self.users()
            .change_password(user, old, new, now, salt)
            .map_err(|e| message(&e))?;
        self.save_users()
            .map_err(|e| format!("Password changed but not saved: {e}"))?;
        Ok(self
            .users()
            .get(user)
            .map(|u| u.privileges)
            .unwrap_or_default())
    }

    /// Change the user IDs and make the change durable; the message
    /// text of a refusal.
    fn change_user(&mut self, change: &UserChange) -> Result<(), String> {
        let now = self.now();
        let salt = self.random();
        change
            .apply(self.users(), now, salt)
            .map_err(|e| message(&e))?;
        if let UserChange::Reset { name, .. } = change {
            self.audit("DS", "R", name);
        }
        self.save_users()
            .map_err(|e| format!("Change made but not saved: {e}"))
    }

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
    /// Install the Licensed Internal Code as `kind` asks: initialize
    /// (option 2, [`DstEnv::install_lic`]), keep ASP 1 and its
    /// configuration (option 3), or restore the load source from its
    /// mirrored partner (option 4).
    fn install_lic_as(
        &mut self,
        device: &str,
        kind: LicKind,
        auto: bool,
        report: &mut dyn FnMut(LicEvent, &mut dyn DstEnv),
    ) -> Result<(), String> {
        match kind {
            LicKind::Initialize => self.install_lic(device, auto, report),
            _ => Err("not available".into()),
        }
    }
    /// Initialize ASP 1 and create the system libraries and values;
    /// `report` is called with each stage of `OS_STEPS` as it completes.
    fn install_os(&mut self, report: &mut dyn FnMut(usize, &mut dyn DstEnv)) -> Result<(), String> {
        let _ = report;
        Err("not available".into())
    }
    /// The devices holding install media of `kind`.
    fn install_devices(&mut self, kind: DeviceKind) -> Vec<InstallDevice> {
        let _ = kind;
        Vec::new()
    }
    /// Install the operating system from `device` with `options`;
    /// `report` is called with each stage of `OS_STEPS`.
    fn install_os_from(
        &mut self,
        device: &InstallDevice,
        options: &OsOptions,
        report: &mut dyn FnMut(usize, &mut dyn DstEnv),
    ) -> Result<(), String> {
        let _ = (device, options);
        self.install_os(report)
    }
    /// `Define or Change the System at IPL`, when the system has it:
    /// shown after the IPL options when they ask for it.
    fn define_system(&mut self) -> Option<Box<dyn crate::panel::Panel>> {
        None
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
    /// The console in use, as the mode names it.
    fn console_type(&self) -> String {
        self.console_mode().text().to_string()
    }
    /// The console mode the LIC record keeps for the next IPL.
    fn console_mode(&self) -> ConsoleMode {
        ConsoleMode::Local
    }
    /// Record the console mode for the next IPL.
    fn set_console_mode(&mut self, mode: ConsoleMode) -> Result<(), String> {
        let _ = mode;
        Err("not available".into())
    }
    /// The Licensed Internal Code's identity and slots, label and
    /// value each, for `Work with Licensed Internal Code`.
    fn lic_info(&mut self) -> Vec<(String, String)> {
        Vec::new()
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
    /// `Work with System Capacity`'s figures, label and value.
    fn capacity(&mut self) -> Vec<(String, String)> {
        Vec::new()
    }
    /// The system dumps kept, oldest first.
    fn dumps(&mut self) -> Result<Vec<DumpRow>, String> {
        Err(NOT_HERE.into())
    }
    /// Take a system dump; its name.
    fn take_dump(&mut self, why: &str) -> Result<String, String> {
        let _ = why;
        Err(NOT_HERE.into())
    }
    /// A system dump's lines.
    fn dump_lines(&mut self, name: &str) -> Result<Vec<String>, String> {
        let _ = name;
        Err(NOT_HERE.into())
    }
    /// Delete a system dump.
    fn delete_dump(&mut self, name: &str) -> Result<(), String> {
        let _ = name;
        Err(NOT_HERE.into())
    }
    /// Copy a system dump to media.
    fn copy_dump(&mut self, name: &str) -> Result<(), String> {
        let _ = name;
        Err("Copying a dump to media is not available in this release".into())
    }
    /// The traces.
    fn traces(&mut self) -> Result<Vec<TraceRow>, String> {
        Err(NOT_HERE.into())
    }
    /// Start a Licensed Internal Code trace.
    fn start_trace(&mut self, spec: &TraceSpec) -> Result<(), String> {
        let _ = spec;
        Err(NOT_HERE.into())
    }
    /// Stop a trace; it is kept for display.
    fn stop_trace(&mut self, name: &str) -> Result<(), String> {
        let _ = name;
        Err(NOT_HERE.into())
    }
    /// A trace formatted.
    fn trace_lines(&mut self, name: &str) -> Result<Vec<String>, String> {
        let _ = name;
        Err(NOT_HERE.into())
    }
    /// Delete a trace.
    fn delete_trace(&mut self, name: &str) -> Result<(), String> {
        let _ = name;
        Err(NOT_HERE.into())
    }
    /// The storage service's event log, entries of major code `major`
    /// (0 for all), one line each.
    fn lic_events(&mut self, major: u16) -> Vec<String> {
        let _ = major;
        Vec::new()
    }
    /// Alter `hex` bytes at `offset` of page `page` of object `id`.
    fn alter_storage(&mut self, id: &str, page: u64, offset: &str, hex: &str) -> Result<String, String> {
        let _ = (id, page, offset, hex);
        Err(NOT_HERE.into())
    }
    /// Print a unit page or an object header; the message.
    fn print_storage(&mut self, unit: &str, page: &str, id: &str) -> Result<String, String> {
        let _ = (unit, page, id);
        Err(NOT_HERE.into())
    }
    /// Print the hardware configuration; the message.
    fn print_hardware(&mut self) -> Result<String, String> {
        Err(NOT_HERE.into())
    }
    /// Every system value: name, value, description.
    fn all_sysvals(&mut self) -> Vec<(String, String, String)> {
        Vec::new()
    }
    /// Change a system value; the message.
    fn set_sysval(&mut self, name: &str, value: &str) -> Result<String, String> {
        let _ = (name, value);
        Err(NOT_HERE.into())
    }
    /// The network attributes, name and value.
    fn network_attributes(&mut self) -> Vec<(String, String)> {
        Vec::new()
    }
    /// The configuration objects: name, type, description.
    fn config_objects(&mut self) -> Vec<(String, String, String)> {
        Vec::new()
    }
    /// The user profiles: name, description.
    fn user_profiles(&mut self) -> Vec<(String, String)> {
        Vec::new()
    }
    /// `EDTRBDAP`'s entries, one line each.
    fn rebuild_paths(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// `EDTCPCST`'s entries, one line each.
    fn pending_constraints(&mut self) -> Vec<String> {
        Vec::new()
    }
    /// The service tools device IDs: name, status, description.
    fn device_ids(&mut self) -> Vec<(String, String, String)> {
        vec![("QCONSOLE".into(), "Enabled".into(), "The system console".into())]
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
    /// The console mode.
    pub console_mode: ConsoleMode,
    /// LIC identity lines to report.
    pub lic: Vec<(String, String)>,
    /// Actions taken, for assertions.
    pub actions: Vec<String>,
    /// Screens shown during long actions, as dumps.
    pub shown: Vec<String>,
    /// Confirm changes first, as SST does.
    pub confirm: bool,
    /// Dumps kept, with their lines.
    pub dumps: Vec<(DumpRow, Vec<String>)>,
    /// Traces, with their lines.
    pub traces: Vec<(TraceRow, Vec<String>)>,
    /// Capacity figures.
    pub capacity: Vec<(String, String)>,
    /// Event log lines.
    pub events: Vec<String>,
    /// System values: name, value, description.
    pub sysvals: Vec<(String, String, String)>,
    /// Install devices to report.
    pub devices: Vec<InstallDevice>,
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
    fn confirm_changes(&self) -> bool {
        self.confirm
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
    fn console_mode(&self) -> ConsoleMode {
        self.console_mode
    }
    fn set_console_mode(&mut self, mode: ConsoleMode) -> Result<(), String> {
        self.actions.push(format!("console {mode:?}"));
        self.console_mode = mode;
        Ok(())
    }
    fn lic_info(&mut self) -> Vec<(String, String)> {
        self.lic.clone()
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
    fn install_lic_as(
        &mut self,
        device: &str,
        kind: LicKind,
        auto: bool,
        report: &mut dyn FnMut(LicEvent, &mut dyn DstEnv),
    ) -> Result<(), String> {
        if kind == LicKind::Initialize {
            return self.install_lic(device, auto, report);
        }
        self.actions
            .push(format!("install-lic {device} {kind:?} auto={auto}"));
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
    fn install_devices(&mut self, kind: DeviceKind) -> Vec<InstallDevice> {
        self.devices
            .iter()
            .filter(|d| d.kind == kind)
            .cloned()
            .collect()
    }
    fn install_os_from(
        &mut self,
        device: &InstallDevice,
        options: &OsOptions,
        report: &mut dyn FnMut(usize, &mut dyn DstEnv),
    ) -> Result<(), String> {
        self.actions.push(format!(
            "install-os {} {} lp={} clear={}",
            device.name, options.language, options.licpgm_all, options.clear_queues
        ));
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
    fn capacity(&mut self) -> Vec<(String, String)> {
        self.capacity.clone()
    }
    fn dumps(&mut self) -> Result<Vec<DumpRow>, String> {
        Ok(self.dumps.iter().map(|d| d.0.clone()).collect())
    }
    fn take_dump(&mut self, why: &str) -> Result<String, String> {
        let name = format!("QSD{:07}", self.dumps.len() + 1);
        self.actions.push(format!("dump {name}"));
        self.dumps.push((
            DumpRow {
                name: name.clone(),
                taken: self.time,
                size: 100,
                text: why.into(),
            },
            vec![format!("System dump {name}")],
        ));
        Ok(name)
    }
    fn dump_lines(&mut self, name: &str) -> Result<Vec<String>, String> {
        self.dumps
            .iter()
            .find(|d| d.0.name == name)
            .map(|d| d.1.clone())
            .ok_or_else(|| format!("System dump {name} not found"))
    }
    fn delete_dump(&mut self, name: &str) -> Result<(), String> {
        self.actions.push(format!("delete dump {name}"));
        self.dumps.retain(|d| d.0.name != name);
        Ok(())
    }
    fn traces(&mut self) -> Result<Vec<TraceRow>, String> {
        Ok(self.traces.iter().map(|t| t.0.clone()).collect())
    }
    fn start_trace(&mut self, spec: &TraceSpec) -> Result<(), String> {
        self.actions.push(format!("trace {} {} {}", spec.name, spec.categories, spec.job));
        self.traces.push((
            TraceRow {
                name: spec.name.clone(),
                kind: "LIC".into(),
                status: "Active".into(),
                records: 0,
            },
            Vec::new(),
        ));
        Ok(())
    }
    fn stop_trace(&mut self, name: &str) -> Result<(), String> {
        self.actions.push(format!("stop trace {name}"));
        for t in &mut self.traces {
            if t.0.name == name {
                t.0.status = "Stopped".into();
            }
        }
        Ok(())
    }
    fn trace_lines(&mut self, name: &str) -> Result<Vec<String>, String> {
        self.traces
            .iter()
            .find(|t| t.0.name == name)
            .map(|t| t.1.clone())
            .ok_or_else(|| format!("Trace {name} not found"))
    }
    fn delete_trace(&mut self, name: &str) -> Result<(), String> {
        self.actions.push(format!("delete trace {name}"));
        self.traces.retain(|t| t.0.name != name);
        Ok(())
    }
    fn lic_events(&mut self, major: u16) -> Vec<String> {
        let _ = major;
        self.events.clone()
    }
    fn alter_storage(&mut self, id: &str, page: u64, offset: &str, hex: &str) -> Result<String, String> {
        self.actions.push(format!("alter {id} {page} {offset} {hex}"));
        Ok(format!("{} bytes altered.", hex.len() / 2))
    }
    fn print_storage(&mut self, unit: &str, page: &str, id: &str) -> Result<String, String> {
        self.actions.push(format!("print {unit} {page} {id}"));
        Ok("Storage printed to QPCSMPRT.".into())
    }
    fn print_hardware(&mut self) -> Result<String, String> {
        self.actions.push("print hardware".into());
        Ok("Configuration printed to QPCSMPRT.".into())
    }
    fn all_sysvals(&mut self) -> Vec<(String, String, String)> {
        self.sysvals.clone()
    }
    fn set_sysval(&mut self, name: &str, value: &str) -> Result<String, String> {
        self.actions.push(format!("sysval {name} {value}"));
        Ok(format!("System value {name} changed"))
    }
    fn config_objects(&mut self) -> Vec<(String, String, String)> {
        vec![("QCONSOLE".into(), "*DEVD".into(), "System console".into())]
    }
    fn user_profiles(&mut self) -> Vec<(String, String)> {
        vec![("QSECOFR".into(), "Security officer".into())]
    }
}
