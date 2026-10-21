// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with disk unit recovery`: replace a lost half, suspend and
//! resume, and the recovery procedures text.

use crate::dst::NOT_AUTHORIZED;
use crate::dst::common::{Confirm, Notice, Select, Submenu};
use crate::dst::disk::config::nonconfigured_row;
use crate::dst::disk::{self, STATUS_HEADINGS};
use crate::env::{DiskUnit, DstEnv};
use crate::panel::Flow;

/// The `Work with Disk Unit Recovery` menu.
#[must_use]
pub fn work_with_disk_unit_recovery(ops: bool, admin: bool) -> Submenu {
    let guard = move |allowed: bool, then: Flow| {
        if allowed {
            then
        } else {
            Flow::Error(NOT_AUTHORIZED.into())
        }
    };
    Submenu::new(
        "dst-disk-recovery",
        "Work with Disk Unit Recovery",
        vec![
            (1, "Save disk unit data", None),
            (2, "Restore disk unit data", None),
            (
                3,
                "Replace configured unit",
                Some(Box::new(move |env| {
                    guard(admin, Flow::Push(Box::new(replace(env))))
                })),
            ),
            (4, "Assign missing unit", None),
            (5, "Recover configuration", None),
            (
                6,
                "Disk unit problem recovery procedures",
                Some(Box::new(|_env| Flow::Push(Box::new(procedures())))),
            ),
            (
                7,
                "Suspend mirrored protection",
                Some(Box::new(move |env| {
                    guard(ops, Flow::Push(Box::new(suspend(env))))
                })),
            ),
            (
                8,
                "Resume mirrored protection",
                Some(Box::new(move |env| {
                    guard(ops, Flow::Push(Box::new(resume(env))))
                })),
            ),
            (9, "Copy disk unit data", None),
        ],
    )
}

fn procedures() -> Notice {
    Notice::new(
        "dst-disk-procedures",
        "Disk Unit Problem Recovery Procedures",
        vec![
            "A mirrored pair keeps running on one half when the other is missing or".into(),
            "fails; the system shows A6001255 or A6001256 and the half is Suspended.".into(),
            String::new(),
            "1. If the unit is back, resume mirrored protection (option 8). The".into(),
            "   copy runs in the background and shows its percent here.".into(),
            "2. If the unit is gone for good, attach a non-configured disk of at".into(),
            "   least the same size and replace the configured unit (option 3).".into(),
            "3. An unprotected unit that is missing stops the IPL with B1001030;".into(),
            "   attach it again and IPL.".into(),
            String::new(),
            "Every allocated page is checksummed. A page that fails on one half".into(),
            "is read from the other and rewritten; chkstg compares both halves.".into(),
        ],
    )
}

fn replace(env: &mut dyn DstEnv) -> Select {
    let lost: Vec<DiskUnit> = env
        .disk_units()
        .into_iter()
        .filter(|u| {
            u.mirrored && (!u.attached || u.status == "Not operational" || u.status == "Suspended")
        })
        .collect();
    let rows: Vec<String> = lost.iter().map(disk::status_row).collect();
    Select::new(
        "dst-replace-unit",
        "Replace Configured Unit",
        "Type 1 beside the unit to replace, then press Enter.",
        &format!("Opt  {STATUS_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], env: &mut dyn DstEnv| {
            let unit = lost[chosen[0]].clone();
            let spare: Vec<_> = env
                .nonconfigured()
                .into_iter()
                .filter(|d| d.size_mib >= unit.size_mib)
                .collect();
            let rows: Vec<String> = spare.iter().map(nonconfigured_row).collect();
            Flow::Push(Box::new(Select::new(
                "dst-replace-with",
                "Select Replacement Unit",
                "Type 1 beside the non-configured unit to use, then press Enter.",
                &format!("Opt  {}", super::config::NONCONFIGURED_HEADINGS),
                rows,
                Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
                    let device = spare[chosen[0]].device.clone();
                    let number = unit.unit;
                    let lines = vec![
                        format!(
                            "Unit {} is replaced by {}; the new unit is copied from its",
                            number + 1,
                            device
                        ),
                        "partner in the background once you continue.".into(),
                    ];
                    Flow::Push(Box::new(
                        Confirm::new(
                            "dst-replace-confirm",
                            "Confirm Replace Configured Unit",
                            lines,
                            Box::new(move |env: &mut dyn DstEnv| {
                                env.replace(number, &device)?;
                                env.resume(number)?;
                                Ok(format!("Unit {} replaced and resuming", number + 1))
                            }),
                        )
                        .unwinding(3),
                    ))
                }),
            )))
        }),
    )
}

fn suspend(env: &mut dyn DstEnv) -> Select {
    let active: Vec<DiskUnit> = env
        .disk_units()
        .into_iter()
        .filter(|u| u.mirrored && u.status == "Active")
        .collect();
    let rows: Vec<String> = active.iter().map(disk::status_row).collect();
    Select::new(
        "dst-suspend",
        "Suspend Mirrored Protection",
        "Type 1 beside the units to suspend, then press Enter.",
        &format!("Opt  {STATUS_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], env: &mut dyn DstEnv| {
            let mut done = 0;
            for &i in chosen {
                if let Err(e) = env.suspend(active[i].unit) {
                    return Flow::Error(e);
                }
                done += 1;
            }
            Flow::Unwind(1, Some((format!("{done} unit(s) suspended"), false)))
        }),
    )
}

fn resume(env: &mut dyn DstEnv) -> Select {
    let suspended: Vec<DiskUnit> = env
        .disk_units()
        .into_iter()
        .filter(|u| u.mirrored && u.attached && u.status != "Active")
        .collect();
    let rows: Vec<String> = suspended.iter().map(disk::status_row).collect();
    Select::new(
        "dst-resume",
        "Resume Mirrored Protection",
        "Type 1 beside the unit to resume, then press Enter.",
        &format!("Opt  {STATUS_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], env: &mut dyn DstEnv| {
            let unit = suspended[chosen[0]].unit;
            match env.resume(unit) {
                Ok(()) => Flow::Unwind(
                    1,
                    Some((
                        format!(
                            "Unit {} resuming in the background; option 1 shows its progress",
                            unit + 1
                        ),
                        false,
                    )),
                ),
                Err(e) => Flow::Error(e),
            }
        }),
    )
}
