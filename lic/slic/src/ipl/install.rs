// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The installer (docs/INSTALL.md): the Licensed Internal Code onto a
//! load source (menu option 2 on installation media, option 4 or a
//! response file without prompts), a re-IPL, then the operating system
//! from an install device (the `QS00SS1` load restored, a slip install
//! or upgrade over an installed system, the recovery data of `SAVSYS`
//! media) and the IPL options. The screens are `svctools`'s; the work
//! is here.

use std::fs::{File, OpenOptions};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use blockdev::{BlockDev, LinuxBlockDev};
use diskimg::{Guids, Plan, fat};
use instmedia::Payload;
use instmedia::osload::{self, Request};
use licpart::{LicPart, Slot};
use release::InstallKind;
use svctools::install::lic::{LicEvent, LicKind};
use svctools::install::os::{InstallDevice, OsOptions};
use svctools::install::stage;
use svctools::{DstEnv, Outcome};

use crate::ipl::dst::Env;
use crate::ipl::menu::{self, Answer};
use crate::ipl::system::{System, now_unix};
use crate::ipl::{Event, dst, instdev, media, power, storage};
use crate::src;

/// Slot A's path on the ESP.
const SLOT_A: &str = "EFI/BOOT/BOOTX64.EFI";
/// Slot B's path on the ESP.
const SLOT_B: &str = "EFI/QS400/B.EFI";

/// The LIC record's TLV tag for the response file of an automatic
/// install, kept from the LIC install to the operating system's.
pub const TAG_INSTALL_ANSWERS: u16 = 0x0108;
/// The LIC record's TLV tag for a LIC written into one slot by an
/// upgrade: the slot the other copy goes to once the operating system is
/// installed too (one byte, `A` or `B`).
pub const TAG_LIC_COPY: u16 = 0x0109;

/// Drive a `svctools` runner until it ends.
fn drive(env: &mut Env<'_>, mut runner: svctools::Runner, rx: &Receiver<Event>) -> Outcome {
    loop {
        let screen = runner.screen(env);
        menu::show(env.system, screen);
        let Some(answer) = menu::read(env.system, rx) else {
            return Outcome::Exit;
        };
        let outcome = match answer {
            Answer::Enter(fields) => runner.enter(&fields, env),
            Answer::Function(n) => runner.function(n, env),
        };
        if let Some(o) = outcome {
            return o;
        }
    }
}

/// Option 2: the LIC install screens. A finished install re-IPLs.
pub fn run_lic(system: &mut System, rx: &Receiver<Event>) {
    let mut env = Env::new(system);
    loop {
        match drive(&mut env, svctools::install::lic::start(), rx) {
            Outcome::Reboot => {
                env.system
                    .out
                    .serial("slic: install: re-IPL from the load source\n");
                power::down(env.system, true);
            }
            Outcome::Dst => match dst::run(env.system, rx) {
                Outcome::PerformIpl => power::down(env.system, true),
                Outcome::AutoInstall => run_auto(env.system),
                _ => {}
            },
            _ => return,
        }
    }
}

/// The answers of an automatic install: the response file the media
/// or the command line names, else the defaults.
fn answers(system: &mut System) -> (String, instmedia::Response) {
    match media::response(system.boot.rsp.as_deref()) {
        Ok(Some((text, r))) => {
            system
                .out
                .serial("slic: install: answers from the response file\n");
            (text, r)
        }
        Ok(None) => (String::new(), instmedia::Response::default()),
        Err(e) => {
            system
                .out
                .serial(&format!("slic: install: response file: {e}; the defaults apply\n"));
            (String::new(), instmedia::Response::default())
        }
    }
}

/// Option 4 (or a response file on the media): the LIC as the answers
/// say, no prompts, then re-IPL. On failure the message goes to serial
/// and the caller's menu returns.
pub fn run_auto(system: &mut System) {
    let (text, rsp) = answers(system);
    PENDING_ANSWERS.with(|a| *a.borrow_mut() = Some(text));
    let mut env = Env::new(system);
    let disks = env.disks();
    let device = match &rsp.load_source {
        Some(want) => disks
            .iter()
            .find(|d| instmedia::Response::names(want, &d.resource, &d.device))
            .map(|d| d.device.clone()),
        None => disks.first().map(|d| d.device.clone()),
    };
    let kind = match rsp.lic_option {
        instmedia::rsp::LicOption::Initialize => LicKind::Initialize,
        instmedia::rsp::LicOption::Recover => LicKind::Recover,
        instmedia::rsp::LicOption::RestoreUnits => LicKind::RestoreUnits,
    };
    let r = match device {
        Some(d) => svctools::install::lic::run_install_as(&mut env, &d, kind, true),
        None => Err(format!(
            "No disk unit {} to install on",
            rsp.load_source.as_deref().unwrap_or("")
        )),
    };
    match r {
        Ok(()) => {
            env.system
                .out
                .serial("slic: automatic install: LIC written; re-IPL from the load source\n");
            power::down(env.system, true);
        }
        Err(e) => env
            .system
            .out
            .serial(&format!("slic: automatic install: {e}\n")),
    }
}

thread_local! {
    /// The response file of the automatic install in progress, for the
    /// LIC record.
    static PENDING_ANSWERS: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// The operating system install screens, from DST or the menu. Returns
/// whether the system was installed.
pub fn run_os(system: &mut System, rx: &Receiver<Event>) -> bool {
    let mut env = Env::new(system);
    matches!(
        drive(&mut env, svctools::install::os::start(), rx),
        Outcome::Continue
    )
}

/// The answers the LIC install kept in the record.
pub fn kept_answers(system: &System) -> Option<instmedia::Response> {
    let (_, r) = system.lic.as_ref()?;
    let text = String::from_utf8_lossy(r.tlv(TAG_INSTALL_ANSWERS)?).into_owned();
    instmedia::Response::parse(&text).ok()
}

/// The operating system without prompts, as the kept answers say: the
/// disks they name added and mirrored, then the install from the device
/// they name (else the first that holds media).
pub fn run_os_auto(system: &mut System) -> Result<(), String> {
    let rsp = kept_answers(system).unwrap_or_default();
    let mut env = Env::new(system);
    configure_disks(&mut env, &rsp);
    let kind = match rsp.device.as_str() {
        "*TAP" => Some(svctools::install::os::DeviceKind::Tape),
        "*OPT" => Some(svctools::install::os::DeviceKind::Optical),
        "*VRT" => Some(svctools::install::os::DeviceKind::Virtual),
        _ => None,
    };
    let device = if let Some(k) = kind {
        env.install_devices(k).into_iter().next()
    } else {
        // A SAVSYS medium first: an automatic recovery.
        let rel = env.store_release();
        instdev::savsys(rel).map(|f| f.device).or_else(|| {
            [
                svctools::install::os::DeviceKind::Optical,
                svctools::install::os::DeviceKind::Virtual,
            ]
            .into_iter()
            .find_map(|k| env.install_devices(k).into_iter().next())
        })
    };
    match device {
        Some(d) => {
            if let Some(why) = &d.refused {
                env.system.emit(&src::INSTALL_REFUSED);
                return Err(why.clone());
            }
            let o = OsOptions {
                licpgm_all: matches!(rsp.licpgm, instmedia::rsp::Licpgms::All),
                ..OsOptions::defaults(&d.name, &rsp.language)
            };
            svctools::install::os::run_install_from(&mut env, &d, &o)
        }
        None => svctools::install::os::run_install(&mut env, &rsp.language),
    }
}

/// The response file's disks: pairs mirrored, units added. A pair
/// naming the load source mirrors unit 1 (the load source's ASP 1).
fn configure_disks(env: &mut Env<'_>, rsp: &instmedia::Response) {
    if rsp.mirror.is_empty() && rsp.units.is_empty() {
        return;
    }
    let disks = env.disks();
    let node = |want: &str| {
        disks
            .iter()
            .find(|d| instmedia::Response::names(want, &d.resource, &d.device))
            .map(|d| d.device.clone())
    };
    let units = env.disk_units();
    for (a, b) in &rsp.mirror {
        let (Some(da), Some(db)) = (node(a), node(b)) else {
            env.system
                .out
                .serial(&format!("slic: install: mirror {a} {b}: a disk is not there\n"));
            continue;
        };
        // The half already in ASP 1, and the one that joins it.
        let configured = |dev: &str| {
            units
                .iter()
                .find(|u| u.device == dev || u.device.starts_with(dev))
                .map(|u| u.unit)
        };
        let (unit, other) = match (configured(&da), configured(&db)) {
            (Some(u), None) => (u, db),
            (None, Some(u)) => (u, da),
            (None, None) => match env.add_unit(&da) {
                Ok(u) => (u, db),
                Err(e) => {
                    env.system
                        .out
                        .serial(&format!("slic: install: add {da}: {e}\n"));
                    continue;
                }
            },
            (Some(_), Some(_)) => continue,
        };
        match env.start_mirror(unit, &other) {
            Ok(n) => env.system.out.serial(&format!(
                "slic: install: unit {unit} mirrored with {other} (unit {n})\n"
            )),
            Err(e) => env
                .system
                .out
                .serial(&format!("slic: install: mirror {unit} {other}: {e}\n")),
        }
    }
    for u in &rsp.units {
        if let Some(d) = node(u) {
            match env.add_unit(&d) {
                Ok(n) => env
                    .system
                    .out
                    .serial(&format!("slic: install: {d} added as unit {n}\n")),
                Err(e) => env
                    .system
                    .out
                    .serial(&format!("slic: install: add {d}: {e}\n")),
            }
        }
    }
}

/// The IPL options screens, once the system values are loaded.
pub fn run_options(system: &mut System, rx: &Receiver<Event>) {
    let mut env = Env::new(system);
    let _ = drive(&mut env, svctools::install::options::start(), rx);
}

/// One panel and whatever it pushes, to its outcome.
pub fn drive_panel(system: &mut System, panel: Box<dyn svctools::Panel>, rx: &Receiver<Event>) -> Outcome {
    let mut env = Env::new(system);
    drive(&mut env, svctools::Runner::new(panel), rx)
}

/// An automatic install's IPL options, from the kept answers: the
/// clock, the IPL and major options, `QSTRUPPGM`, the system name and
/// QSECOFR's password.
pub fn apply_answers(system: &mut System) {
    let Some(rsp) = kept_answers(system) else {
        return;
    };
    let mut env = Env::new(system);
    if rsp.date.is_some() || rsp.time.is_some() {
        let (d, t) = env.clock();
        let date = rsp.date.clone().unwrap_or(d);
        let time = rsp.time.clone().unwrap_or(t);
        if let Err(e) = env.set_clock(&date, &time) {
            env.system.out.serial(&format!("slic: install: clock: {e}\n"));
        }
    }
    let o = svctools::install::options::IplOptions {
        set_major_options: true,
        define_system: false,
        clear_job_queues: rsp.clear_job_queues,
        clear_output_queues: rsp.clear_output_queues,
        clear_job_logs: rsp.clear_job_logs,
        start_print_writers: rsp.start_print_writers,
        restricted_state: rsp.restricted_state,
    };
    let _ = env.set_ipl_options(&o);
    let _ = env.set_major_options(&svctools::install::options::MajorOptions {
        auto_configure: rsp.auto_configure,
        device_naming: rsp.device_naming.clone(),
        special_environment: rsp.special_environment.clone(),
    });
    let system = env.system;
    if let Some(vals) = system.sysvals.as_mut() {
        let mut set = |n: &str, v: &str| {
            if let Err(e) = vals.set_text(n, v) {
                system_note(n, &e.to_string());
            }
        };
        set("QSTRUPPGM", &rsp.qstrup);
        if let Some(name) = &rsp.system_name {
            set("QSYSNAME", name);
        }
        let _ = crate::ipl::sysvals::save(system);
    }
    if let instmedia::rsp::QsecofrPolicy::Set(pw) = &rsp.qsecofr {
        match set_qsecofr(system, pw) {
            Ok(()) => system
                .out
                .serial("slic: install: QSECOFR's password set from the response file\n"),
            Err(e) => system.out.serial(&format!("slic: install: QSECOFR: {e}\n")),
        }
    }
    system.out.serial("slic: install: response file applied\n");
}

fn system_note(name: &str, e: &str) {
    eprintln!("slic: install: {name}: {e}");
}

/// QSECOFR's password, not expired: the shipped profiles made first.
fn set_qsecofr(system: &mut System, pw: &str) -> Result<(), String> {
    crate::stgsvc::security::install(system)?;
    let pwdlvl = crate::stgsvc::security::sysval_num(system, "QPWDLVL", 3);
    let s = system.storage.as_mut().ok_or("the store is not open")?;
    let qsys = s.qsys.ok_or("no QSYS")?;
    let now = now_unix();
    sec::guard::as_system(|| {
        let mut t = s.store.begin().map_err(|e| e.to_string())?;
        let (id, mut p) =
            sec::profile::objects::read(&mut t, qsys, "QSECOFR").map_err(|e| e.to_string())?;
        p.hashes = sec::password::hashes_for(pw, pwdlvl);
        p.pwd_none = false;
        p.pwdexp = false;
        p.pwd_changed = now;
        sec::profile::objects::write(&mut t, id, &p).map_err(|e| e.to_string())?;
        t.commit().map_err(|e| e.to_string())
    })
}

/// The installation stage recorded on the load source: `Some(auto)`
/// when the LIC is installed and the operating system is still to come.
pub fn pending(system: &System) -> Option<bool> {
    system
        .lic
        .as_ref()
        .map(|(_, r)| &r.header)
        .filter(|h| h.install_stage == 1)
        .map(|h| h.auto_install)
}

/// The operating system is installed: clear the stage, the answers, and
/// put an upgrade's LIC into the other slot too.
pub fn finish(system: &mut System) {
    copy_lic(system);
    let cleared = match system.lic.as_mut() {
        Some((part, record)) if record.header.install_stage != 0 => {
            record.header.install_stage = 0;
            record.header.auto_install = false;
            record.remove_tlv(TAG_INSTALL_ANSWERS);
            part.write(record).map(|_| true).map_err(|e| e.to_string())
        }
        _ => Ok(false),
    };
    match cleared {
        Ok(true) => system.emit(&src::INSTALL_OS_COMPLETE),
        Ok(false) => {}
        Err(e) => system.out.serial(&format!("slic: LIC write: {e}\n")),
    }
}

fn slot_path(s: Slot) -> &'static str {
    match s {
        Slot::A => SLOT_A,
        Slot::B => SLOT_B,
    }
}

fn letter(s: Slot) -> u8 {
    match s {
        Slot::A => b'A',
        Slot::B => b'B',
    }
}

/// After an upgrade's operating system: the new LIC copied from the
/// slot it went to into the other one, so both slots hold the release.
fn copy_lic(system: &mut System) {
    let Some(to) = system
        .lic
        .as_ref()
        .and_then(|(_, r)| r.tlv(TAG_LIC_COPY))
        .and_then(|b| match b.first() {
            Some(b'A') => Some(Slot::A),
            Some(b'B') => Some(Slot::B),
            _ => None,
        })
    else {
        return;
    };
    let from = to.other();
    let r = (|| -> Result<(), String> {
        let lic = licpart::locate().ok();
        let (node, _) = diskimg::locate::esp(lic.as_deref()).map_err(|e| e.to_string())?;
        let open = || OpenOptions::new().read(true).write(true).open(&node);
        let image = fat::read_file_at(open().map_err(|e| e.to_string())?, slot_path(from))
            .map_err(|e| e.to_string())?;
        let mut f = open().map_err(|e| e.to_string())?;
        fat::write_file(&mut f, slot_path(to), &image).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())
    })();
    match r {
        Ok(()) => {
            system.out.serial(&format!(
                "slic: install: the Licensed Internal Code copied from slot {from:?} to slot {to:?}\n"
            ));
            if let Some((part, record)) = system.lic.as_mut() {
                record.remove_tlv(TAG_LIC_COPY);
                let _ = part.write(record);
            }
        }
        Err(e) => system
            .out
            .serial(&format!("slic: install: slot {to:?} not written: {e}\n")),
    }
}

impl Env<'_> {
    /// The release ASP 1 records, opening it when it can be: `None` for
    /// a system with no ASP 1 (or no QSYS) yet.
    pub(crate) fn store_release(&mut self) -> Option<release::Release> {
        if self.system.storage.is_none() {
            let _ = storage::locate_and_open(self.system);
        }
        let Some(s) = self.system.storage.as_ref() else {
            return storage::refused_release();
        };
        s.qsys?;
        Some(s.store.asp_release())
    }

    /// Install the LIC on `device` as `kind` says.
    pub(crate) fn write_lic_as(
        &mut self,
        device: &str,
        kind: LicKind,
        auto: bool,
        report: &mut dyn FnMut(LicEvent, &mut dyn DstEnv),
    ) -> Result<(), String> {
        match kind {
            LicKind::Initialize => self.write_lic(device, auto, 1, report),
            LicKind::Recover => self.write_lic_over(device, auto, report),
            LicKind::RestoreUnits => self.write_lic_restore(device, auto, report),
        }
    }

    /// Write the LIC onto `device`, reporting each step; the record's
    /// install stage is `stage` (1: the operating system follows).
    pub(crate) fn write_lic(
        &mut self,
        device: &str,
        auto: bool,
        stage: u8,
        report: &mut dyn FnMut(LicEvent, &mut dyn DstEnv),
    ) -> Result<(), String> {
        let now = now_unix();
        let name = device.rsplit('/').next().unwrap_or(device).to_string();
        self.system.emit(&src::INSTALL_STARTED);
        let (uki_a, uki_b) = read_media_ukis(self.system)?;
        let bytes = {
            let dev = LinuxBlockDev::open(std::path::Path::new(device))
                .map_err(|e| format!("open {device}: {e}"))?;
            dev.blocks() * 4096
        };
        let plan = Plan::for_bytes(bytes).map_err(|e| e.to_string())?;
        let esp = fat::build(&fat::DISK_ESP, &[(SLOT_A, &uki_a), (SLOT_B, &uki_b)])
            .map_err(|e| e.to_string())?;
        let guids = Guids::from_random([
            storage::fresh_uuid(),
            storage::fresh_uuid(),
            storage::fresh_uuid(),
            storage::fresh_uuid(),
            storage::fresh_uuid(),
            storage::fresh_uuid(),
        ]);
        // A used disk: the IFS log is cleared, the root stamped now.
        let ifs = diskimg::IfsFormat {
            time: diskimg::xfsfmt::Timestamp {
                secs: i64::try_from(now).unwrap_or(0),
                nsecs: 0,
            },
            zeroed: false,
        };
        storage::close(self.system);
        crate::ipl::ifs::unmount(self.system);
        storage::release_lic_on(self.system, &name);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(device)
            .map_err(|e| format!("open {device}: {e}"))?;
        let mut last = 0u8;
        report(LicEvent::DiskPercent(0), self);
        diskimg::write_image_with(&mut file, &plan, &guids, &esp, &ifs, &mut |done, total| {
            let p = u8::try_from(done * 100 / total.max(1)).unwrap_or(100);
            if p / 5 != last / 5 {
                last = p;
                report(LicEvent::DiskPercent(p), self);
            }
        })
        .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        crate::platform::blk::reread_partitions(&file)
            .map_err(|e| format!("re-read partitions: {e}"))?;
        drop(file);
        self.system.emit(&src::INSTALL_PARTITIONED);
        report(LicEvent::StepDone(0), self);
        for n in [2, 3, diskimg::IFS_INDEX] {
            if !crate::platform::blk::wait_for_block(&part_name(&name, n), Duration::from_secs(10)) {
                return Err(format!("partition {} did not appear", part_name(&name, n)));
            }
        }
        // The LIC partition: a fresh record with the install pending.
        let lic_dev: File = OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/{}", part_name(&name, 2)))
            .map_err(|e| e.to_string())?;
        let (mut part_lic, mut record) = LicPart::init(lic_dev, now).map_err(|e| e.to_string())?;
        self.system.emit(&src::INSTALL_LIC_PART);
        report(LicEvent::StepDone(1), self);
        // Both slots went onto the ESP with the image; the steps are
        // reported in order for the display.
        self.system.emit(&src::INSTALL_SLOT_A);
        report(LicEvent::StepDone(2), self);
        self.system.emit(&src::INSTALL_SLOT_B);
        report(LicEvent::StepDone(3), self);
        if let Some(vals) = self.system.sysvals.as_ref() {
            vals.store(&mut record).map_err(|e| e.to_string())?;
        }
        // The console mode chosen in the media's DST follows the system.
        let tag = svctools::dst::console_mode::TAG_CONSOLE_MODE;
        if let Some(mode) = self.system.lic.as_ref().and_then(|(_, r)| r.tlv(tag)) {
            record
                .set_tlv(tag, mode.to_vec())
                .map_err(|e| e.to_string())?;
        }
        keep_answers(&mut record, auto)?;
        record.header.install_stage = stage;
        record.header.auto_install = auto && stage == 1;
        part_lic.write(&mut record).map_err(|e| e.to_string())?;
        drop(part_lic);
        self.system.emit(&src::INSTALL_LIC_RECORD);
        report(LicEvent::StepDone(4), self);
        self.system.emit(&src::INSTALL_LIC_COMPLETE);
        Ok(())
    }

    /// Option 3: the LIC over the installed load source; ASP 1 and its
    /// configuration stay. The release rule decides: the same release
    /// again or a newer one goes into the slot the system did not boot,
    /// which becomes the default; the operating system install follows.
    fn write_lic_over(
        &mut self,
        device: &str,
        auto: bool,
        report: &mut dyn FnMut(LicEvent, &mut dyn DstEnv),
    ) -> Result<(), String> {
        self.system.emit(&src::INSTALL_STARTED);
        let found = media::locate()?;
        let media_rel: release::Release = found
            .manifest
            .release
            .parse()
            .map_err(|_| format!("the media's release {} is not valid", found.manifest.release))?;
        let have = self
            .store_release()
            .ok_or("No installed system on this load source; use option 2")?;
        let kind = release::check_upgrade(have, media_rel).map_err(|e| {
            self.system.emit(&src::INSTALL_REFUSED);
            e.to_string()
        })?;
        self.system.out.serial(&format!(
            "slic: install: {} over {have}\n",
            match kind {
                InstallKind::Slip => format!("slip install of {media_rel}"),
                InstallKind::Upgrade { to, .. } => format!("upgrade to {to}"),
            }
        ));
        report(LicEvent::StepDone(0), self);
        let (uki_a, uki_b) = read_media_ukis(self.system)?;
        let disk = device.rsplit('/').next().unwrap_or(device);
        let lic_node = format!("/dev/{}", part_name(disk, 2));
        let (node, _) = diskimg::locate::esp(Some(std::path::Path::new(&lic_node)))
            .map_err(|e| format!("the load source's ESP: {e}"))?;
        // The LIC record of the load source, read where it is.
        let lic_dev: File = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lic_node)
            .map_err(|e| format!("{lic_node}: {e}"))?;
        let mut part_lic = LicPart::open(lic_dev);
        let mut record = part_lic.read().map_err(|e| format!("the LIC record: {e}"))?;
        report(LicEvent::StepDone(1), self);
        let slots = ptf::slot::SlotState::decode(record.tlv(ptf::slot::TAG_LIC_SLOTS).unwrap_or_default());
        let booted = slots.default.unwrap_or(Slot::A);
        let target = booted.other();
        let image = match target {
            Slot::A => &uki_a,
            Slot::B => &uki_b,
        };
        {
            let mut f = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&node)
                .map_err(|e| format!("{}: {e}", node.display()))?;
            fat::write_file(&mut f, slot_path(target), image).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        self.system.emit(&src::INSTALL_LIC_SLOT);
        report(LicEvent::StepDone(if target == Slot::A { 2 } else { 3 }), self);
        // The new slot boots from now on; the firmware is told, and
        // where it cannot be, slot A (its removable-media path) gets the
        // image too, so the next IPL runs the new LIC either way.
        let mut st = slots.clone();
        st.default = Some(target);
        record
            .set_tlv(ptf::slot::TAG_LIC_SLOTS, st.encode())
            .map_err(|e| e.to_string())?;
        let told = crate::platform::efivar::EfiFirmware::open()
            .and_then(|mut fw| ptf::slot::Firmware::boot_default(&mut fw, target));
        match told {
            Ok(()) => {
                record
                    .set_tlv(TAG_LIC_COPY, vec![letter(booted)])
                    .map_err(|e| e.to_string())?;
            }
            Err(e) => {
                self.system.out.serial(&format!(
                    "slic: install: boot order not set ({e}); slot A written as well\n"
                ));
                let mut f = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&node)
                    .map_err(|e| e.to_string())?;
                fat::write_file(&mut f, SLOT_A, &uki_a).map_err(|e| e.to_string())?;
                fat::write_file(&mut f, SLOT_B, &uki_b).map_err(|e| e.to_string())?;
                f.sync_all().map_err(|e| e.to_string())?;
                record.remove_tlv(TAG_LIC_COPY);
            }
        }
        report(LicEvent::StepDone(if target == Slot::A { 3 } else { 2 }), self);
        keep_answers(&mut record, auto)?;
        record.header.install_stage = 1;
        record.header.auto_install = auto;
        part_lic.write(&mut record).map_err(|e| e.to_string())?;
        drop(part_lic);
        // The copy slic holds follows.
        if let Some((part, r)) = self.system.lic.as_mut() {
            part.refresh(r);
        }
        self.system.emit(&src::INSTALL_LIC_RECORD);
        report(LicEvent::StepDone(4), self);
        self.system.emit(&src::INSTALL_LIC_COMPLETE);
        Ok(())
    }

    /// Option 4: a new load source in the place of a failed one. The LIC
    /// is written as for option 2 with no install pending, then ASP 1 is
    /// opened on its other units and the new disk's ASP 1 partition put
    /// in the place of the missing half, to be copied from its partner.
    fn write_lic_restore(
        &mut self,
        device: &str,
        auto: bool,
        report: &mut dyn FnMut(LicEvent, &mut dyn DstEnv),
    ) -> Result<(), String> {
        self.write_lic(device, auto, 0, report)?;
        storage::locate_and_open(self.system).map_err(|s| {
            format!("ASP 1 could not be opened on the other units: {}", s.text)
        })?;
        let missing = self
            .disk_units()
            .into_iter()
            .find(|u| u.mirrored && !u.attached)
            .ok_or("No mirrored unit of ASP 1 is missing; nothing to restore")?;
        let disk = device.rsplit('/').next().unwrap_or(device);
        let asp = format!("/dev/{}", part_name(disk, 3));
        self.replace(missing.unit, &asp)?;
        self.system.out.serial(&format!(
            "slic: install: unit {} is now {asp}, copied from unit {}\n",
            missing.unit,
            missing.partner.unwrap_or_default()
        ));
        self.system.emit(&src::INSTALL_RESTORE_UNITS);
        Ok(())
    }

    /// Without install media: ASP 1 and the system libraries; the
    /// operating system's objects are made at the first IPL, as before
    /// the install restored them.
    pub(crate) fn write_os(
        &mut self,
        report: &mut dyn FnMut(usize, &mut dyn DstEnv),
    ) -> Result<(), String> {
        let now = now_unix();
        if self.system.storage.is_none() {
            storage::locate_and_open(self.system).map_err(|s| s.text.to_string())?;
        }
        if self.system.storage.is_none() {
            return Err("no ASP 1 partition on the load source".into());
        }
        self.system.emit(&src::INSTALL_ASP);
        let s = self.system.storage.as_mut().expect("opened above");
        if s.qsys.is_none() {
            let qsys = storage::bootstrap(&mut s.store, now, "QS400").map_err(|e| e.to_string())?;
            s.qsys = Some(qsys);
            s.fresh = false;
        }
        self.system.emit(&src::INSTALL_LIBRARIES);
        self.system
            .out
            .serial("slic: install: no install media; the system objects are made at the first IPL\n");
        for i in 0..svctools::install::OS_STEPS.len() {
            report(i, self);
        }
        Ok(())
    }

    /// Install the operating system from `device`: ASP 1 and QSYS made
    /// when new, the `QS00SS1` load restored (a slip install or upgrade
    /// over an installed one), programs translated again after an
    /// upgrade, the recovery data of `SAVSYS` media, then the licensed
    /// programs asked for.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn write_os_from(
        &mut self,
        device: &InstallDevice,
        options: &OsOptions,
        report: &mut dyn FnMut(usize, &mut dyn DstEnv),
    ) -> Result<(), String> {
        let now = now_unix();
        let found = instdev::by_name(&device.name)
            .ok_or_else(|| format!("{} holds no install media now", device.name))?;
        if let Some(why) = &device.refused {
            self.system.emit(&src::INSTALL_REFUSED);
            return Err(why.clone());
        }
        if !found.contents.has_os() {
            return Err(format!("The media on {} holds no operating system", device.name));
        }
        if self.system.storage.is_none() {
            storage::locate_and_open(self.system).map_err(|s| s.text.to_string())?;
        }
        self.system.emit(&src::INSTALL_ASP);
        let Some(s) = self.system.storage.as_mut() else {
            return Err("no ASP 1 partition on the load source".into());
        };
        let fresh = s.qsys.is_none();
        let kind = if fresh {
            let qsys = storage::bootstrap(&mut s.store, now, "QS400").map_err(|e| e.to_string())?;
            s.qsys = Some(qsys);
            s.fresh = false;
            Ok(InstallKind::Slip)
        } else {
            release::check_upgrade(s.store.asp_release(), found.contents.release)
        };
        let kind = kind.map_err(|e| {
            self.system.emit(&src::INSTALL_REFUSED);
            e.to_string()
        })?;
        self.system.emit(&src::INSTALL_LIBRARIES);
        report(stage::LIBRARIES, self);
        let stream = instdev::read(&found, &Payload::Product(instmedia::layout::OS_PRODUCT.into()))?;
        let mut req = Request {
            stream,
            product: instmedia::layout::OS_PRODUCT,
            options: Vec::new(),
            key: crate::stgsvc::licpgm::quartz_key(),
            system: crate::stgsvc::licpgm::system_info(self.system),
            accepted_by: "QSECOFR",
            now,
        };
        let s = self.system.storage.as_mut().expect("opened above");
        let qsys = s.qsys.expect("made above");
        let mut r = sec::guard::as_system(|| osload::install(&mut s.store, qsys, &req))
            .map_err(|e| e.to_string())?;
        self.system.emit(&src::INSTALL_OS_LOAD);
        report(stage::CODE, self);
        report(stage::LANGUAGE, self);
        if !fresh {
            let s = self.system.storage.as_mut().expect("opened above");
            r.retranslated = sec::guard::as_system(|| {
                osload::retranslate_user_programs(&mut s.store, qsys, kind, &mut |t, id, all| {
                    ile::store::retranslate(t, id, all).map_err(|e| e.to_string())
                })
            })
            .map_err(|e| e.to_string())?;
            if matches!(kind, InstallKind::Upgrade { .. }) {
                s.store.asp_superblock_set_release();
                s.store.checkpoint().map_err(|e| e.to_string())?;
            }
            self.system.emit(&src::INSTALL_RETRANSLATED);
        }
        self.system
            .out
            .serial(&format!("slic: install: {}\n", osload::summary(&r)));
        report(stage::CHECK, self);
        if found.contents.savsys {
            let sec = found
                .contents
                .payloads
                .contains(&Payload::SecData)
                .then(|| instdev::read(&found, &Payload::SecData))
                .transpose()?;
            let cfg = found
                .contents
                .payloads
                .contains(&Payload::Config)
                .then(|| instdev::read(&found, &Payload::Config))
                .transpose()?;
            for line in crate::cpp::savsys::recover(self.system, sec.as_deref(), cfg.as_deref()) {
                self.system.out.serial(&format!("slic: install: {line}\n"));
            }
            self.system.emit(&src::INSTALL_RECOVERY);
        }
        report(stage::VALUES, self);
        let wanted = licpgms_wanted(self.system, options, &found);
        for p in wanted {
            let stream = match instdev::read(&found, &Payload::Product(p.clone())) {
                Ok(s) => s,
                Err(e) => {
                    self.system.out.serial(&format!("slic: install: {p}: {e}\n"));
                    continue;
                }
            };
            req.stream = stream;
            let s = self.system.storage.as_mut().expect("opened above");
            let pid = p.clone();
            let r = sec::guard::as_system(|| {
                osload::install(
                    &mut s.store,
                    qsys,
                    &Request {
                        product: &pid,
                        stream: std::mem::take(&mut req.stream),
                        options: Vec::new(),
                        key: req.key,
                        system: req.system.clone(),
                        accepted_by: req.accepted_by,
                        now,
                    },
                )
            });
            let line = match r {
                Ok(r) => osload::summary(&r),
                Err(e) => format!("{p}: {e}"),
            };
            self.system.out.serial(&format!("slic: install: {line}\n"));
        }
        self.system.emit(&src::INSTALL_LICPGMS);
        report(stage::DONE, self);
        Ok(())
    }
}

/// The licensed programs to install after the operating system: every
/// one on the media for `GO LICPGM` option 1's choice, else the ones the
/// kept answers list.
fn licpgms_wanted(system: &System, options: &OsOptions, found: &instdev::Found) -> Vec<String> {
    let on_media: Vec<String> = found
        .contents
        .products()
        .into_iter()
        .filter(|p| p != instmedia::layout::OS_PRODUCT)
        .collect();
    if options.licpgm_all {
        return on_media;
    }
    match kept_answers(system).map(|r| r.licpgm) {
        Some(instmedia::rsp::Licpgms::All) => on_media,
        Some(instmedia::rsp::Licpgms::List(v)) => {
            let mut out: Vec<String> = v
                .into_iter()
                .map(|(p, _)| p)
                .filter(|p| on_media.contains(p))
                .collect();
            out.dedup();
            out
        }
        _ => Vec::new(),
    }
}

/// A partition's node name on `disk`.
fn part_name(disk: &str, n: u32) -> String {
    let sep = if disk.ends_with(|c: char| c.is_ascii_digit()) {
        "p"
    } else {
        ""
    };
    format!("{disk}{sep}{n}")
}

/// The response file of an automatic install into a new LIC record.
fn keep_answers(record: &mut licpart::Record, auto: bool) -> Result<(), String> {
    let text = PENDING_ANSWERS.with(|a| a.borrow().clone()).unwrap_or_default();
    if auto && !text.is_empty() {
        record
            .set_tlv(TAG_INSTALL_ANSWERS, text.into_bytes())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The two slot UKIs: from a `SAVSYS` medium when one is attached (the
/// saved system's own LIC), else from the installation media.
fn read_media_ukis(system: &mut System) -> Result<(Vec<u8>, Vec<u8>), String> {
    if let Some(f) = instdev::savsys(None)
        && f.contents.payloads.contains(&Payload::LicA)
    {
        system.out.serial(&format!(
            "slic: install: the LIC from the system save on {}\n",
            f.device.name
        ));
        let a = instdev::read(&f, &Payload::LicA)?;
        let b = instdev::read(&f, &Payload::LicB).unwrap_or_else(|_| a.clone());
        return Ok((a, b));
    }
    let found = media::locate()?;
    system.out.serial(&format!(
        "slic: install: media {} on {} ({})\n",
        found.manifest.volume, found.device, found.fstype
    ));
    media::read_payload(&found)
}
