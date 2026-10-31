// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with disk configuration`: the displays, the ASP threshold,
//! adding and removing units, and the protection menus.

use console::Screen;

use crate::dst::NOT_AUTHORIZED;
use crate::dst::common::{Confirm, Notice, Select, Submenu, guarded};
use crate::dst::disk::{self, CAPACITY_HEADINGS, STATUS_HEADINGS, mirror};
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
                Some(Box::new(|env| Flow::Push(Box::new(parity(env))))),
            ),
            (6, "Work with checksum protection", None),
            (7, "Work with disk compression", None),
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
            } else {
                "Unprotected"
            },
            u.partner.map_or(String::new(), |p| (p + 1).to_string()),
            if u.mirrored {
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

/// The device parity display: controllers capable of it, of which this
/// release has none.
fn parity(env: &mut dyn DstEnv) -> Notice {
    let controllers = env.parity_capable();
    let mut lines = vec![
        "Device parity protection needs a controller that provides it.".to_string(),
        String::new(),
    ];
    if controllers.is_empty() {
        lines.push("No device parity capable controllers are attached to this system.".into());
    } else {
        lines.push("Controller  Status".into());
        lines.extend(controllers);
    }
    Notice::new(
        "dst-disk-parity",
        "Work with Device Parity Protection",
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
            "An attention code is raised when ASP 1 is fuller than this.",
            console::Attr::GREEN,
        );
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let Some(p) = fields.first().and_then(|f| f.trim().parse::<u8>().ok()) else {
            return Flow::Error("Threshold must be a number from 1 to 100".into());
        };
        if !(1..=100).contains(&p) {
            return Flow::Error("Threshold must be a number from 1 to 100".into());
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
                Some(Box::new(add_units)),
            ),
            (3, "Add units to ASPs and balance data", None),
            (
                4,
                "Remove units from configuration",
                Some(Box::new(|env| Flow::Push(Box::new(remove_units(env))))),
            ),
            (6, "Move units from one ASP to another", None),
        ],
    )
}

/// Units join ASP 1 only once it has a load source.
pub const NO_LOAD_SOURCE: &str =
    "ASP 1 has no load source unit; install the Licensed Internal Code first";

/// Pick non-configured disks to add to ASP 1.
fn add_units(env: &mut dyn DstEnv) -> Flow {
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
