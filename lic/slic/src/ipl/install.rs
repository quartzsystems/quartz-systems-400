// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The installer: the Licensed Internal Code onto a load source (menu
//! option 2 on installation media, or option 4 without prompts), a
//! re-IPL, then the operating system (ASP 1, the system libraries, the
//! system values) and the IPL options. The screens are `svctools`'s;
//! the work is here.

use std::fs::{File, OpenOptions};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use blockdev::{BlockDev, LinuxBlockDev};
use diskimg::{Guids, Plan, fat};
use licpart::LicPart;
use svctools::install::lic::LicEvent;
use svctools::{DstEnv, Outcome};

use crate::ipl::dst::Env;
use crate::ipl::menu::{self, Answer};
use crate::ipl::system::{System, now_unix};
use crate::ipl::{Event, dst, media, power, storage};
use crate::src;

/// Slot A's path on the ESP.
const SLOT_A: &str = "EFI/BOOT/BOOTX64.EFI";
/// Slot B's path on the ESP.
const SLOT_B: &str = "EFI/QS400/B.EFI";

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

/// Option 4: the LIC onto the first disk, no prompts, then re-IPL. On
/// failure the message goes to serial and the caller's menu returns.
pub fn run_auto(system: &mut System) {
    let mut env = Env::new(system);
    match svctools::install::lic::auto(&mut env) {
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

/// The operating system install screens, from DST or the menu. Returns
/// whether the system was installed.
pub fn run_os(system: &mut System, rx: &Receiver<Event>) -> bool {
    let mut env = Env::new(system);
    matches!(
        drive(&mut env, svctools::install::os::start(), rx),
        Outcome::Continue
    )
}

/// The operating system without prompts.
pub fn run_os_auto(system: &mut System) -> Result<(), String> {
    let mut env = Env::new(system);
    svctools::install::os::run_install(&mut env, "2924")
}

/// The IPL options screens, once the system values are loaded.
pub fn run_options(system: &mut System, rx: &Receiver<Event>) {
    let mut env = Env::new(system);
    let _ = drive(&mut env, svctools::install::options::start(), rx);
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

/// The operating system is installed: clear the stage.
pub fn finish(system: &mut System) {
    let cleared = match system.lic.as_mut() {
        Some((part, record)) if record.header.install_stage != 0 => {
            record.header.install_stage = 0;
            record.header.auto_install = false;
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

impl Env<'_> {
    /// Write the LIC onto `device`, reporting each step.
    pub(crate) fn write_lic(
        &mut self,
        device: &str,
        auto: bool,
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
        let part = |n: u32| {
            let sep = if name.ends_with(|c: char| c.is_ascii_digit()) {
                "p"
            } else {
                ""
            };
            format!("{name}{sep}{n}")
        };
        for n in [2, 3, diskimg::IFS_INDEX] {
            if !crate::platform::blk::wait_for_block(&part(n), Duration::from_secs(10)) {
                return Err(format!("partition {} did not appear", part(n)));
            }
        }
        // The LIC partition: a fresh record with the install pending.
        let lic_dev: File = OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/{}", part(2)))
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
        record.header.install_stage = 1;
        record.header.auto_install = auto;
        part_lic.write(&mut record).map_err(|e| e.to_string())?;
        drop(part_lic);
        self.system.emit(&src::INSTALL_LIC_RECORD);
        report(LicEvent::StepDone(4), self);
        self.system.emit(&src::INSTALL_LIC_COMPLETE);
        Ok(())
    }

    /// Initialize ASP 1 on the load source and create the system
    /// libraries; the system values follow in the OS phase.
    pub(crate) fn write_os(
        &mut self,
        report: &mut dyn FnMut(usize, &mut dyn DstEnv),
    ) -> Result<(), String> {
        let now = now_unix();
        // The ASP as DST configured it, or a blank one formatted now.
        if self.system.storage.is_none() {
            storage::locate_and_open(self.system).map_err(|s| s.text.to_string())?;
        }
        if self.system.storage.is_none() {
            return Err("no ASP 1 partition on the load source".into());
        }
        self.system.emit(&src::INSTALL_ASP);
        report(0, self);
        let s = self.system.storage.as_mut().expect("opened above");
        if s.qsys.is_none() {
            let qsys = storage::bootstrap(&mut s.store, now, "QS400").map_err(|e| e.to_string())?;
            s.qsys = Some(qsys);
            s.fresh = false;
        }
        self.system.emit(&src::INSTALL_LIBRARIES);
        report(1, self);
        // The *SYSVAL object is created when the values load.
        report(2, self);
        Ok(())
    }
}

/// The two slot UKIs from the installation media, wherever it is (see
/// `media`).
fn read_media_ukis(system: &mut System) -> Result<(Vec<u8>, Vec<u8>), String> {
    let found = media::locate()?;
    system.out.serial(&format!(
        "slic: install: media {} on {} ({})\n",
        found.manifest.volume, found.device, found.fstype
    ));
    media::read_payload(&found)
}
