// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Installing the Licensed Internal Code: the two menus, the load
//! source selection, the confirmation, and the status display while
//! the environment writes the disk. Ends with a re-IPL.

use crate::dst::common::{Confirm, Select, Submenu};
use crate::dst::disk::config::{NONCONFIGURED_HEADINGS, nonconfigured_row};
use crate::env::{DstEnv, NonConfigured};
use crate::install::{LIC_STEPS, Progress, RESTORE_STEPS, disk_status, lic_status};
use crate::panel::{Flow, Outcome, Runner};

/// The installer, starting at `Install Licensed Internal Code`.
#[must_use]
pub fn start() -> Runner {
    Runner::new(Box::new(install_lic_menu()))
}

/// The `Install Licensed Internal Code` menu.
#[must_use]
pub fn install_lic_menu() -> Submenu {
    Submenu::new(
        "install-lic",
        "Install Licensed Internal Code",
        vec![
            (
                1,
                "Install Licensed Internal Code",
                Some(Box::new(|env| Flow::Push(Box::new(lic_options_menu(env))))),
            ),
            (
                2,
                "Work with Dedicated Service Tools (DST)",
                Some(Box::new(|_env| Flow::Exit(Outcome::Dst))),
            ),
            (3, "Define alternate installation device", None),
            (
                4,
                "Run hardware qualification (QSHWQUAL)",
                Some(Box::new(|_env| Flow::Push(Box::new(crate::install::hwqual::menu())))),
            ),
        ],
    )
}

/// What a LIC install does with the disks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicKind {
    /// Option 1: the LIC again onto the current load source; ASP 1, its
    /// configuration, the service tools users and the boot entries stay
    /// (the new LIC goes into the slot the system did not boot).
    Restore,
    /// Option 2: partition the load source, initialize the system.
    Initialize,
    /// Option 3: on IBM i, a replaced load source whose configuration
    /// is recovered from the other units. Not there yet: it does what
    /// option 1 does over a load source chosen from the disks with
    /// QS/400 data (see docs/INSTALL.md).
    Recover,
    /// Option 4: a new load source, its ASP 1 copied back from the
    /// mirrored partner that survived.
    RestoreUnits,
}

/// The `Install Licensed Internal Code (LIC)` menu, under the current
/// load source as IBM i shows it.
pub fn lic_options_menu(env: &mut dyn DstEnv) -> Submenu {
    let lead = match env.load_source() {
        Some(d) => vec![
            "Current load source, where option 1 restores the LIC:".to_string(),
            format!("  {NONCONFIGURED_HEADINGS}"),
            format!("  {}", nonconfigured_row(&NonConfigured { has_data: false, ..d })),
        ],
        None => vec!["No installed load source was found.".to_string()],
    };
    Submenu::new(
        "install-lic-options",
        "Install Licensed Internal Code (LIC)",
        vec![
            (
                1,
                "Restore Licensed Internal Code",
                Some(Box::new(restore)),
            ),
            (
                2,
                "Install Licensed Internal Code and Initialize System",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(select_load_source(env)))
                })),
            ),
            (
                3,
                "Install Licensed Internal Code and Recover Configuration",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(select_for(env, LicKind::Recover)))
                })),
            ),
            (
                4,
                "Install Licensed Internal Code and Restore Disk Unit Data",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(select_for(env, LicKind::RestoreUnits)))
                })),
            ),
            (
                5,
                "Install Licensed Internal Code and Upgrade Load Source",
                None,
            ),
        ],
    )
    .with_lead(lead)
}

/// Option 1: as on IBM i, no load source choice and no confirmation;
/// the LIC goes onto the current load source at once, then a re-IPL.
fn restore(env: &mut dyn DstEnv) -> Flow {
    let Some(disk) = env.load_source() else {
        return Flow::Error("No installed load source; use option 2 to install".into());
    };
    match run_install_as(env, &disk.device, LicKind::Restore, false) {
        Ok(()) => Flow::Exit(Outcome::Reboot),
        Err(e) => Flow::Error(e),
    }
}

/// `Select Load Source Device`: every disk the system sees.
pub fn select_load_source(env: &mut dyn DstEnv) -> Select {
    let disks = env.disks();
    let rows: Vec<String> = disks.iter().map(nonconfigured_row).collect();
    Select::new(
        "install-load-source",
        "Select Load Source Device",
        "Type 1 beside the disk unit to install on, then press Enter.",
        &format!("Opt  {NONCONFIGURED_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            let disk = disks[chosen[0]].clone();
            Flow::Push(Box::new(confirm(disk, false)))
        }),
    )
}

/// The load source choice for options 3 and 4: for 3 the disks that
/// hold QS/400 data (the installed load source), for 4 the blank ones
/// (the replacement for the failed one).
pub fn select_for(env: &mut dyn DstEnv, kind: LicKind) -> Select {
    let want_data = kind == LicKind::Recover;
    let disks: Vec<NonConfigured> = env
        .disks()
        .into_iter()
        .filter(|d| d.has_data == want_data)
        .collect();
    let rows: Vec<String> = disks.iter().map(nonconfigured_row).collect();
    let (name, prompt) = match kind {
        LicKind::Recover => (
            "install-recover-source",
            "Type 1 beside the installed load source, then press Enter.",
        ),
        _ => (
            "install-restore-source",
            "Type 1 beside the replacement load source, then press Enter.",
        ),
    };
    Select::new(
        name,
        "Select Load Source Device",
        prompt,
        &format!("Opt  {NONCONFIGURED_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            let disk = disks[chosen[0]].clone();
            Flow::Push(Box::new(confirm_as(disk, kind, false)))
        }),
    )
}

/// `Install LIC and Initialize System - Confirmation`.
#[must_use]
pub fn confirm(disk: NonConfigured, auto: bool) -> Confirm {
    confirm_as(disk, LicKind::Initialize, auto)
}

/// The confirmation of one kind of LIC install.
#[must_use]
pub fn confirm_as(disk: NonConfigured, kind: LicKind, auto: bool) -> Confirm {
    let (name, title, what): (&'static str, &'static str, [&str; 2]) = match kind {
        LicKind::Initialize => (
            "install-lic-confirm",
            "Install LIC and Initialize System - Confirmation",
            [
                "The Licensed Internal Code will be installed on this disk unit and",
                "the system initialized. All data on the unit will be lost.",
            ],
        ),
        LicKind::Recover | LicKind::Restore => (
            "install-recover-confirm",
            "Install LIC and Recover Configuration - Confirmation",
            [
                "The Licensed Internal Code will be installed on this load source.",
                "ASP 1, its disk configuration and every object on it are kept.",
            ],
        ),
        LicKind::RestoreUnits => (
            "install-restore-confirm",
            "Install LIC and Restore Disk Unit Data - Confirmation",
            [
                "The Licensed Internal Code will be installed on this new load source",
                "and ASP 1 copied onto it from its mirrored partner.",
            ],
        ),
    };
    let mut lines = vec![
        what[0].to_string(),
        what[1].to_string(),
        String::new(),
        format!("  {}", nonconfigured_row(&disk)),
        String::new(),
        "The system re-IPLs from the load source when the install completes.".to_string(),
    ];
    if kind == LicKind::Initialize {
        lines.push(String::new());
        lines.push("ASP 1 takes the unit after the LIC. The integrated file system is".into());
        lines.push("made inside ASP 1 at the first IPL and grows with it.".into());
    }
    Confirm::new(
        name,
        title,
        lines,
        Box::new(move |env: &mut dyn DstEnv| {
            run_install_as(env, &disk.device, kind, auto)?;
            Ok("Licensed Internal Code installed; re-IPL".into())
        }),
    )
    .exiting(Outcome::Reboot)
}

/// Write the disk with the status displays, then ask for a re-IPL.
pub fn run_install(env: &mut dyn DstEnv, device: &str, auto: bool) -> Result<(), String> {
    run_install_as(env, device, LicKind::Initialize, auto)
}

/// One kind of LIC install with the status displays.
pub fn run_install_as(
    env: &mut dyn DstEnv,
    device: &str,
    kind: LicKind,
    auto: bool,
) -> Result<(), String> {
    let system = env.system_name();
    let device = device.to_string();
    let steps: &[&str] = match kind {
        LicKind::Restore | LicKind::Recover => &RESTORE_STEPS,
        LicKind::Initialize | LicKind::RestoreUnits => &LIC_STEPS,
    };
    let mut progress = Progress::new(steps);
    env.install_lic_as(&device, kind, auto, &mut |event, env| {
        match event {
            LicEvent::DiskPercent(p) => {
                env.show(disk_status(&system, &device, p));
                if p % 10 == 0 {
                    env.pace();
                }
                return;
            }
            LicEvent::StepDone(i) => progress.done(i),
        }
        env.show(lic_status(env, &progress));
        env.pace();
    })
}

/// What the environment reports while installing the LIC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicEvent {
    /// Percent of the disk written.
    DiskPercent(u8),
    /// Step `i` of [`LIC_STEPS`] is done.
    StepDone(usize),
}

/// Automatic installation: the first disk, no prompts.
pub fn auto(env: &mut dyn DstEnv) -> Result<(), String> {
    let disks = env.disks();
    let Some(first) = disks.first() else {
        return Err("No disk unit to install on".into());
    };
    let device = first.device.clone();
    run_install(env, &device, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MemEnv;

    fn f(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_sequence_ends_in_a_reboot() {
        let mut e = MemEnv::new("QS400");
        e.spare = vec![NonConfigured {
            device: "/dev/vda".into(),
            resource: "DD001".into(),
            serial: "QM00001".into(),
            type_model: "QEMU HARDDISK".into(),
            size_mib: 4096,
            has_data: false,
            controller: "DC01".into(),
        }];
        let mut r = start();
        assert_eq!(r.enter(&f(&["1"]), &mut e), None);
        assert_eq!(r.top(), "install-lic-options");
        assert_eq!(r.enter(&f(&["1"]), &mut e), None);
        let d = r.screen(&mut e).dump();
        assert!(d.contains("No installed load source was found"));
        assert!(d.contains("use option 2"));
        assert!(e.actions.is_empty());
        assert_eq!(r.enter(&f(&["2"]), &mut e), None);
        assert_eq!(r.top(), "install-load-source");
        assert_eq!(r.enter(&f(&["1"]), &mut e), None);
        assert_eq!(r.top(), "install-lic-confirm");
        assert_eq!(r.function(12, &mut e), None);
        assert_eq!(r.top(), "install-load-source");
        r.enter(&f(&["1"]), &mut e);
        assert_eq!(r.function(10, &mut e), Some(Outcome::Reboot));
        assert_eq!(e.actions, vec!["install-lic /dev/vda auto=false"]);
        assert!(
            e.shown
                .iter()
                .any(|s| s.contains("Install Licensed Internal Code - Status"))
        );
        assert!(e.shown.last().unwrap().contains("X  Write the LIC record"));
        e.actions.clear();
        auto(&mut e).unwrap();
        assert_eq!(e.actions, vec!["install-lic /dev/vda auto=true"]);
        let mut r = start();
        assert_eq!(r.enter(&f(&["2"]), &mut e), Some(Outcome::Dst));
    }

    #[test]
    fn options_3_and_4_offer_their_disks() {
        let mut e = MemEnv::new("QS400");
        let disk = |dev: &str, res: &str, data: bool| NonConfigured {
            device: dev.into(),
            resource: res.into(),
            has_data: data,
            size_mib: 4096,
            ..NonConfigured::default()
        };
        e.spare = vec![disk("/dev/vda", "DD001", true), disk("/dev/vdb", "DD002", false)];
        let mut r = start();
        r.enter(&f(&["1"]), &mut e);
        r.enter(&f(&["3"]), &mut e);
        assert_eq!(r.top(), "install-recover-source");
        let d = r.screen(&mut e).dump();
        assert!(d.contains("DD001") && !d.contains("DD002"));
        r.enter(&f(&["1"]), &mut e);
        assert_eq!(r.top(), "install-recover-confirm");
        assert_eq!(r.function(10, &mut e), Some(Outcome::Reboot));
        assert_eq!(e.actions, vec!["install-lic /dev/vda Recover auto=false"]);
        e.actions.clear();
        let mut r = start();
        r.enter(&f(&["1"]), &mut e);
        r.enter(&f(&["4"]), &mut e);
        assert_eq!(r.top(), "install-restore-source");
        assert!(!r.screen(&mut e).dump().contains("DD001"));
        r.enter(&f(&["1"]), &mut e);
        assert!(r.screen(&mut e).dump().contains("mirrored partner"));
        assert_eq!(r.function(10, &mut e), Some(Outcome::Reboot));
        assert_eq!(e.actions, vec!["install-lic /dev/vdb RestoreUnits auto=false"]);
    }

    #[test]
    fn option_1_restores_onto_the_load_source_without_asking() {
        let mut e = MemEnv::new("QS400");
        let disk = |dev: &str, res: &str, data: bool| NonConfigured {
            device: dev.into(),
            resource: res.into(),
            has_data: data,
            size_mib: 4096,
            ..NonConfigured::default()
        };
        e.spare = vec![disk("/dev/vda", "DD001", false), disk("/dev/vdb", "DD002", true)];
        let mut r = start();
        r.enter(&f(&["1"]), &mut e);
        let d = r.screen(&mut e).dump();
        assert!(d.contains("Current load source") && d.contains("DD002"));
        assert!(!d.contains("DD001"));
        // No selection, no confirmation: the status display, then a re-IPL.
        assert_eq!(r.enter(&f(&["1"]), &mut e), Some(Outcome::Reboot));
        assert_eq!(e.actions, vec!["install-lic /dev/vdb Restore auto=false"]);
        let last = e.shown.last().unwrap();
        assert!(last.contains("Install Licensed Internal Code - Status"));
        assert!(last.contains("X  Make the new slot the IPL default"));
    }
}
