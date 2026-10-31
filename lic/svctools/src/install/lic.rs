// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Installing the Licensed Internal Code: the two menus, the load
//! source selection, the confirmation, and the status display while
//! the environment writes the disk. Ends with a re-IPL.

use crate::dst::common::{Confirm, Select, Submenu};
use crate::dst::disk::config::{NONCONFIGURED_HEADINGS, nonconfigured_row};
use crate::env::{DstEnv, NonConfigured};
use crate::install::{LIC_STEPS, Progress, disk_status, lic_status};
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
                Some(Box::new(|_env| Flow::Push(Box::new(lic_options_menu())))),
            ),
            (
                2,
                "Work with Dedicated Service Tools (DST)",
                Some(Box::new(|_env| Flow::Exit(Outcome::Dst))),
            ),
            (3, "Define alternate installation device", None),
        ],
    )
}

/// The `Install Licensed Internal Code (LIC)` menu: only option 2 works.
#[must_use]
pub fn lic_options_menu() -> Submenu {
    Submenu::new(
        "install-lic-options",
        "Install Licensed Internal Code (LIC)",
        vec![
            (1, "Restore Licensed Internal Code", None),
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
                None,
            ),
            (
                4,
                "Install Licensed Internal Code and Restore Disk Unit Data",
                None,
            ),
            (
                5,
                "Install Licensed Internal Code and Upgrade Load Source",
                None,
            ),
        ],
    )
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

/// `Install LIC and Initialize System - Confirmation`.
#[must_use]
pub fn confirm(disk: NonConfigured, auto: bool) -> Confirm {
    let lines = vec![
        "The Licensed Internal Code will be installed on this disk unit and".to_string(),
        "the system initialized. All data on the unit will be lost.".to_string(),
        String::new(),
        format!("  {}", nonconfigured_row(&disk)),
        String::new(),
        "The system re-IPLs from the load source when the install completes.".to_string(),
    ];
    Confirm::new(
        "install-lic-confirm",
        "Install LIC and Initialize System - Confirmation",
        lines,
        Box::new(move |env: &mut dyn DstEnv| {
            run_install(env, &disk.device, auto)?;
            Ok("Licensed Internal Code installed; re-IPL".into())
        }),
    )
    .exiting(Outcome::Reboot)
}

/// Write the disk with the status displays, then ask for a re-IPL.
pub fn run_install(env: &mut dyn DstEnv, device: &str, auto: bool) -> Result<(), String> {
    let system = env.system_name();
    let device = device.to_string();
    let mut progress = Progress::new(&LIC_STEPS);
    env.install_lic(&device, auto, &mut |event, env| {
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
        assert!(r.screen(&mut e).dump().contains("not available"));
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
}
