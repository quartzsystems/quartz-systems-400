// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with disk configuration`: the displays, the ASP threshold,
//! adding and removing units, and the protection menus.

use console::Screen;

use crate::dst::NOT_AUTHORIZED;
use crate::dst::common::{Confirm, Notice, Select, Submenu, guarded};
use crate::dst::disk::{self, CAPACITY_HEADINGS, STATUS_HEADINGS, in_parity_set, mirror, parity};
use crate::env::{DstEnv, NonConfigured};
use crate::panel::{Flow, Panel};
use crate::ui::{self, FormLine};

fn admin_only(admin: bool, then: impl FnOnce() -> Flow) -> Flow {
    if admin {
        then()
    } else {
        Flow::Error(NOT_AUTHORIZED.into())
    }
}

/// The `Work with Disk Configuration` menu.
#[must_use]
pub fn work_with_disk_configuration(admin: bool) -> Submenu {
    Submenu::new(
        "dst-disk-config",
        "Work with Disk Configuration",
        vec![
            (
                1,
                "Display disk configuration",
                Some(Box::new(|_env| Flow::Push(Box::new(display_menu())))),
            ),
            (
                2,
                "Work with ASP threshold",
                Some(Box::new(move |_env| {
                    admin_only(admin, || Flow::Push(Box::new(Threshold)))
                })),
            ),
            (
                3,
                "Work with ASP configuration",
                Some(Box::new(move |_env| {
                    admin_only(admin, || Flow::Push(Box::new(asp_configuration())))
                })),
            ),
            (
                4,
                "Work with mirrored protection",
                Some(Box::new(move |_env| {
                    admin_only(admin, || {
                        Flow::Push(Box::new(mirror::work_with_mirrored_protection()))
                    })
                })),
            ),
            (
                5,
                "Work with device parity protection",
                Some(Box::new(move |env| parity::open(env, admin))),
            ),
            (6, "Work with checksum protection", crate::dst::common::says(crate::dst::answers::NO_CHECKSUM)),
            (7, "Work with disk compression", crate::dst::common::says(crate::dst::answers::NO_COMPRESSION)),
        ],
    )
}

/// The `Display Disk Configuration` menu.
#[must_use]
pub fn display_menu() -> Submenu {
    Submenu::new(
        "dst-disk-display",
        "Display Disk Configuration",
        vec![
            (
                1,
                "Display disk configuration status",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(disk::display(
                        "dst-disk-status",
                        "Display Disk Configuration Status",
                        STATUS_HEADINGS,
                        env,
                        disk::status_row,
                    )))
                })),
            ),
            (
                2,
                "Display disk configuration capacity",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(disk::display(
                        "dst-disk-capacity",
                        "Display Disk Configuration Capacity",
                        CAPACITY_HEADINGS,
                        env,
                        disk::capacity_row,
                    )))
                })),
            ),
            (
                3,
                "Display disk configuration protection",
                Some(Box::new(|env| Flow::Push(Box::new(protection(env))))),
            ),
            (
                4,
                "Display non-configured units",
                Some(Box::new(|env| Flow::Push(Box::new(nonconfigured(env))))),
            ),
        ],
    )
}

/// Protection per unit: the pair, or none.
fn protection(env: &mut dyn DstEnv) -> Notice {
    let mut lines = vec!["ASP Unit  Protection   Partner  Status".to_string()];
    let units = env.disk_units();
    if units.is_empty() {
        lines.push("  (No configured units)".into());
    }
    for u in &units {
        lines.push(format!(
            "{:>3} {:>4}  {:<12} {:>7}  {}",
            u.asp,
            u.unit + 1,
            if u.mirrored {
                "Mirrored"
            } else if in_parity_set(u) {
                "Device parity"
            } else {
                "Unprotected"
            },
            u.partner.map_or(String::new(), |p| (p + 1).to_string()),
            if u.mirrored || in_parity_set(u) {
                u.status.clone()
            } else {
                String::new()
            }
        ));
    }
    Notice::new(
        "dst-disk-protection",
        "Display Disk Configuration Protection",
        lines,
    )
}

/// The non-configured list's headings: the controller resource the disk
/// is attached through under `Ctl`, the device by its kernel name.
pub const NONCONFIGURED_HEADINGS: &str =
    "Resource  Serial Number     Type Model    Size (MiB) Ctl   Device";

/// One row of the non-configured list.
#[must_use]
pub fn nonconfigured_row(d: &NonConfigured) -> String {
    format!(
        "{:<9} {:<17} {:<13} {:>10} {:<5} {}{}",
        d.resource,
        disk::clip(&d.serial, 17),
        disk::clip(&d.type_model, 13),
        d.size_mib,
        disk::clip(&d.controller, 5),
        d.device.strip_prefix("/dev/").unwrap_or(&d.device),
        if d.has_data { " (has data)" } else { "" }
    )
}

fn nonconfigured(env: &mut dyn DstEnv) -> Notice {
    let mut lines = vec![NONCONFIGURED_HEADINGS.to_string()];
    let disks = env.nonconfigured();
    if disks.is_empty() {
        lines.push("  (No non-configured units)".into());
    }
    lines.extend(disks.iter().map(nonconfigured_row));
    Notice::new(
        "dst-disk-nonconfigured",
        "Display Non-Configured Units",
        lines,
    )
}

/// The ASP threshold form.
pub struct Threshold;

impl Panel for Threshold {
    fn name(&self) -> &'static str {
        "dst-asp-threshold"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let now = env.asp_threshold();
        let mut s = ui::form(
            "Work with ASP Threshold",
            &env.system_name(),
            &[FormLine::new(
                "ASP 1 threshold (percent)",
                3,
                now.to_string(),
            )],
            "F3=Exit   F12=Cancel",
        );
        s.put(
            ui::BODY_ROW + 2,
            ui::BODY_COL,
            "QSYSOPR is sent CPF0907 while ASP 1 is this full or fuller.",
            console::Attr::GREEN,
        );
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let Some(p) = fields.first().and_then(|f| f.trim().parse::<u8>().ok()) else {
            return Flow::Error(crate::dst::disk::reasons::THRESHOLD_RANGE.into());
        };
        if !(1..=100).contains(&p) {
            return Flow::Error(crate::dst::disk::reasons::THRESHOLD_RANGE.into());
        }
        guarded(
            env,
            "dst-asp-threshold-confirm",
            "Confirm ASP Threshold",
            vec![format!("The ASP 1 threshold will be {p}%.")],
            1,
            Box::new(move |env: &mut dyn DstEnv| {
                env.set_asp_threshold(p)
                    .map(|()| format!("ASP 1 threshold set to {p}%"))
            }),
        )
    }
}

/// The `Work with ASP Configuration` menu.
#[must_use]
pub fn asp_configuration() -> Submenu {
    Submenu::new(
        "dst-asp-config",
        "Work with ASP Configuration",
        vec![
            (
                1,
                "Display disk configuration capacity",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(disk::display(
                        "dst-disk-capacity",
                        "Display Disk Configuration Capacity",
                        CAPACITY_HEADINGS,
                        env,
                        disk::capacity_row,
                    )))
                })),
            ),
            (
                2,
                "Add units to ASPs",
                Some(Box::new(|env| add_units(env, false))),
            ),
            (
                3,
                "Add units to ASPs and balance data",
                Some(Box::new(|env| add_units(env, true))),
            ),
            (
                4,
                "Remove units from configuration",
                Some(Box::new(|env| Flow::Push(Box::new(remove_units(env))))),
            ),
            (6, "Move units from one ASP to another", crate::dst::common::says(crate::dst::answers::ONE_ASP)),
        ],
    )
}

/// Units join ASP 1 only once it has a load source.
pub const NO_LOAD_SOURCE: &str =
    "ASP 1 has no load source unit; install the Licensed Internal Code first";

/// Pick non-configured disks to add to ASP 1; with `balance`, the ASP's
/// data is then spread over every unit by capacity, as `STRASPBAL
/// TYPE(*CAPACITY)` does.
fn add_units(env: &mut dyn DstEnv, balance: bool) -> Flow {
    if env.disk_units().is_empty() {
        return Flow::Error(NO_LOAD_SOURCE.into());
    }
    let disks = env.nonconfigured();
    let rows: Vec<String> = disks.iter().map(nonconfigured_row).collect();
    Flow::Push(Box::new(Select::new(
        "dst-add-units",
        "Add Units to ASP 1",
        "Type 1 beside the units to add, then press Enter.",
        &format!("Opt  {NONCONFIGURED_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            let devices: Vec<String> = chosen.iter().map(|&i| disks[i].device.clone()).collect();
            let mut lines = vec![
                "These units will be added to ASP 1 as unprotected units:".to_string(),
                String::new(),
            ];
            lines.extend(
                chosen
                    .iter()
                    .map(|&i| format!("  {}", nonconfigured_row(&disks[i]))),
            );
            lines.push(String::new());
            lines.push("Any data on them is lost.".into());
            Flow::Push(Box::new(
                Confirm::new(
                    "dst-add-units-confirm",
                    "Confirm Add Units",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| {
                        let mut added = Vec::new();
                        for d in &devices {
                            added.push(env.add_unit(d)? + 1);
                        }
                        if balance {
                            let moved = env.balance_asp()?;
                            return Ok(format!("Unit(s) {added:?} added to ASP 1; {moved} object(s) moved to balance it"));
                        }
                        Ok(format!("Unit(s) {added:?} added to ASP 1"))
                    }),
                )
                .unwinding(2),
            ))
        }),
    )))
}

/// Pick configured units to remove.
fn remove_units(env: &mut dyn DstEnv) -> Select {
    let units = env.disk_units();
    let rows: Vec<String> = units.iter().map(disk::status_row).collect();
    Select::new(
        "dst-remove-units",
        "Remove Units from Configuration",
        "Type 1 beside the units to remove, then press Enter.",
        &format!("Opt  {STATUS_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            let numbers: Vec<u16> = chosen.iter().map(|&i| units[i].unit).collect();
            let mut lines = vec![
                "Data on these units is moved to the other units of the ASP,".to_string(),
                "then the units leave the configuration:".into(),
                String::new(),
            ];
            lines.extend(
                chosen
                    .iter()
                    .map(|&i| format!("  {}", disk::status_row(&units[i]))),
            );
            Flow::Push(Box::new(
                Confirm::new(
                    "dst-remove-units-confirm",
                    "Confirm Remove Units",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| {
                        // Highest first, so numbers stay valid.
                        let mut order = numbers.clone();
                        order.sort_unstable_by(|a, b| b.cmp(a));
                        for u in order {
                            env.remove_unit(u)?;
                        }
                        Ok(format!(
                            "{} unit(s) removed from the configuration",
                            numbers.len()
                        ))
                    }),
                )
                .unwinding(2),
            ))
        }),
    )
}

/// SST's `Work with Disk Configuration` (the primary, V7R1M0,
/// golden/verified/oracle71/sst/sst-disk-config): its twelve options,
/// hot spares at 9 and 10.
#[must_use]
pub fn sst_disk_configuration(admin: bool) -> Submenu {
    use crate::dst::answers;
    use crate::dst::common::says;
    Submenu::new(
        "sst-disk-config",
        "Work with Disk Configuration",
        vec![
            (1, "Display disk configuration", Some(Box::new(|_env| Flow::Push(Box::new(display_menu()))))),
            (2, "Add units to ASPs", Some(Box::new(move |env| admin_only(admin, || add_units(env, false))))),
            (
                3,
                "Work with ASP threshold",
                Some(Box::new(move |_env| admin_only(admin, || Flow::Push(Box::new(Threshold))))),
            ),
            (
                4,
                "Add units to ASPs and balance data",
                Some(Box::new(move |env| admin_only(admin, || add_units(env, true)))),
            ),
            (5, "Enable remote load source mirroring", says(answers::NO_REMOTE_BUS)),
            (6, "Disable remote load source mirroring", says(answers::NO_REMOTE_BUS)),
            (7, "Start compression on non-configured units", says(answers::NO_COMPRESSION)),
            (8, "Work with device parity protection", Some(Box::new(move |env| parity::open_sst(env, admin)))),
            (9, "Start hot spare", Some(Box::new(move |env| parity::hot_spare(env, admin, true)))),
            (10, "Stop hot spare", Some(Box::new(move |env| parity::hot_spare(env, admin, false)))),
            (11, "Work with encryption", says(answers::NO_ENCRYPTION)),
            (
                12,
                "Work with removing units from configuration",
                Some(Box::new(move |env| admin_only(admin, || Flow::Push(Box::new(remove_units(env)))))),
            ),
        ],
    )
    .sst(26)
}
