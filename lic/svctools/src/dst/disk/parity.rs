// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with device parity protection`: the status display, start
//! (RAID 5, RAID 6), stop, include and exclude. The sets are kept by the
//! object store; the controller they belong to is the virtual one the
//! hardware table names (`docs/STORE_FORMAT.md` §11).

use crate::dst::NOT_AUTHORIZED;
use crate::dst::common::{Confirm, Notice, Select, Submenu};
use crate::dst::disk::config::{NONCONFIGURED_HEADINGS, nonconfigured_row};
use crate::dst::disk::{self, STATUS_HEADINGS, in_parity_set, reasons};
use crate::env::{DiskUnit, DstEnv, NonConfigured, ParityUnit};
use crate::panel::Flow;

/// Most units in a set.
const MAX_UNITS: usize = 18;

/// The headings of the status display and the member lists.
pub const PARITY_HEADINGS: &str = "Set  Level   Unit  Serial Number     Type Model    Resource  Status";

/// The `set` of a hot spare's row in the parity displays.
pub const SPARE_SET: u8 = 0;

/// The status a hot spare shows.
pub const HOT_SPARE: &str = "Hot spare";

/// What starting a hot spare reports.
pub const SPARE_STARTED: &str = "Hot spare started";

/// What stopping the hot spare on `resource` reports.
#[must_use]
pub fn spare_stopped(resource: &str) -> String {
    format!("{resource} is no longer a hot spare")
}

/// One member row. The status is in the words of IBM's parity status
/// display: `Active`, `Failed`, `Unprotected`, and while the work runs
/// `nn% Rebuilt` on the unit being rebuilt and `nn% Resyncing` on every
/// unit of a set whose parity is being computed. A hot spare has no set
/// and no unit number.
#[must_use]
pub fn parity_row(p: &ParityUnit) -> String {
    if p.set == SPARE_SET {
        return format!(
            "{:>3}  {:<7} {:>4}  {:<17} {:<13} {:<9} {HOT_SPARE}",
            "",
            "",
            "",
            disk::clip(&p.serial, 17),
            disk::clip(&p.type_model, 13),
            p.resource,
        );
    }
    let status = match (p.percent, p.status.as_str()) {
        (Some(n), "DPY/Rebuild") => format!("{n:>3}% Rebuilt"),
        (Some(n), _) => format!("{n:>3}% Resyncing"),
        (None, s) => s.strip_prefix("DPY/").unwrap_or(s).to_string(),
    };
    format!(
        "{:>3}  {:<7} {:>4}  {:<17} {:<13} {:<9} {}",
        p.set,
        p.level,
        p.unit + 1,
        disk::clip(&p.serial, 17),
        disk::clip(&p.type_model, 13),
        p.resource,
        status
    )
}

/// What option 5 of `Work with disk configuration` shows: the menu, or
/// a note when no controller provides parity.
pub fn open(env: &mut dyn DstEnv, admin: bool) -> Flow {
    let controllers = env.parity_capable();
    if controllers.is_empty() {
        return Flow::Push(Box::new(Notice::new(
            "dst-disk-parity",
            "Work with Device Parity Protection",
            vec![
                "Device parity protection needs a controller that provides it.".to_string(),
                String::new(),
                "No device parity capable controllers are attached to this system.".into(),
            ],
        )));
    }
    Flow::Push(Box::new(menu(admin)))
}

fn admin_only(admin: bool, then: impl FnOnce() -> Flow) -> Flow {
    if admin {
        then()
    } else {
        Flow::Error(NOT_AUTHORIZED.into())
    }
}

/// The `Work with Device Parity Protection` menu.
#[must_use]
pub fn menu(admin: bool) -> Submenu {
    Submenu::new(
        "dst-parity",
        "Work with Device Parity Protection",
        vec![
            (
                1,
                "Display device parity status",
                Some(Box::new(|env| Flow::Push(Box::new(status(env))))),
            ),
            (
                2,
                "Start device parity protection - RAID 5",
                Some(Box::new(move |env| admin_only(admin, || start(env, 5)))),
            ),
            (
                3,
                "Start device parity protection - RAID 6",
                Some(Box::new(move |env| admin_only(admin, || start(env, 6)))),
            ),
            (
                4,
                "Stop device parity protection",
                Some(Box::new(move |env| admin_only(admin, || stop(env)))),
            ),
            (
                5,
                "Include unit in device parity protection",
                Some(Box::new(move |env| admin_only(admin, || include(env)))),
            ),
            (
                6,
                "Exclude unit from device parity protection",
                Some(Box::new(move |env| admin_only(admin, || exclude(env)))),
            ),
            (
                7,
                "Start device parity protection - RAID 5 with hot spare",
                Some(Box::new(move |env| admin_only(admin, || start_with_spare(env, 5)))),
            ),
            (
                8,
                "Start device parity protection - RAID 6 with hot spare",
                Some(Box::new(move |env| admin_only(admin, || start_with_spare(env, 6)))),
            ),
            (
                9,
                "Start hot spare",
                Some(Box::new(move |env| admin_only(admin, || start_spare(env)))),
            ),
            (
                10,
                "Stop hot spare",
                Some(Box::new(move |env| admin_only(admin, || stop_spare(env)))),
            ),
        ],
    )
}

/// The `Display Device Parity Status` display.
fn status(env: &mut dyn DstEnv) -> Notice {
    let controllers = env.parity_capable();
    let units = env.parity_units();
    let mut lines = Vec::new();
    for c in &controllers {
        lines.push(format!("Controller: {c}"));
    }
    lines.push(String::new());
    lines.push(PARITY_HEADINGS.to_string());
    if units.iter().all(|p| p.set == SPARE_SET) {
        lines.push("  (No units have device parity protection)".into());
    }
    // Sets first, then the hot spares.
    lines.extend(units.iter().filter(|p| p.set != SPARE_SET).map(parity_row));
    lines.extend(units.iter().filter(|p| p.set == SPARE_SET).map(parity_row));
    Notice::new(
        "dst-parity-status",
        "Display Device Parity Status",
        lines,
    )
}

/// Units that can join a set: configured, attached, neither mirrored
/// nor in a set already.
fn eligible(env: &mut dyn DstEnv) -> Vec<DiskUnit> {
    env.disk_units()
        .into_iter()
        .filter(|u| !u.mirrored && !in_parity_set(u) && u.attached)
        .collect()
}

fn level_name(level: u8) -> &'static str {
    if level == 6 { "RAID 6" } else { "RAID 5" }
}

fn start(env: &mut dyn DstEnv, level: u8) -> Flow {
    start_set(env, level, false)
}

fn start_with_spare(env: &mut dyn DstEnv, level: u8) -> Flow {
    start_set(env, level, true)
}

/// The non-configured disk a new set takes as its hot spare: the
/// smallest at least as large as its largest unit.
fn spare_for(disks: &[NonConfigured], largest_mib: u64) -> Option<NonConfigured> {
    disks
        .iter()
        .filter(|d| d.size_mib >= largest_mib)
        .min_by_key(|d| d.size_mib)
        .cloned()
}

/// Options 2, 3, 7 and 8: pick the units, confirm, start.
fn start_set(env: &mut dyn DstEnv, level: u8, with_spare: bool) -> Flow {
    let units = eligible(env);
    let disks = if with_spare { env.nonconfigured() } else { Vec::new() };
    let rows: Vec<String> = units.iter().map(disk::status_row).collect();
    let (name, title) = match (level, with_spare) {
        (6, false) => ("dst-parity-start6", "Start Device Parity Protection - RAID 6"),
        (6, true) => ("dst-parity-start6-spare", "Start Device Parity Protection - RAID 6 with Hot Spare"),
        (_, false) => ("dst-parity-start5", "Start Device Parity Protection - RAID 5"),
        (_, true) => ("dst-parity-start5-spare", "Start Device Parity Protection - RAID 5 with Hot Spare"),
    };
    Flow::Push(Box::new(Select::new(
        name,
        title,
        "Type 1 beside the units to protect together, then press Enter.",
        &format!("Opt  {STATUS_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            let least = if level == 6 { 4 } else { 3 };
            if chosen.len() < least || chosen.len() > MAX_UNITS {
                let why = if level == 6 { reasons::RAID6_UNITS } else { reasons::RAID5_UNITS };
                return Flow::Error(why.into());
            }
            let numbers: Vec<u16> = chosen.iter().map(|&i| units[i].unit).collect();
            let share = if level == 6 { 2 } else { 1 };
            let smallest = chosen.iter().map(|&i| units[i].size_mib).min().unwrap_or(0);
            let largest = chosen.iter().map(|&i| units[i].size_mib).max().unwrap_or(0);
            let hot_spare = if with_spare {
                match spare_for(&disks, largest) {
                    Some(d) => Some(d),
                    None => return Flow::Error(reasons::NO_SPARE_DISK.into()),
                }
            } else {
                None
            };
            let mut lines = vec![
                format!(
                    "Device parity protection ({}) starts on these units. Each gives",
                    level_name(level)
                ),
                format!(
                    "about {share}/{} of its space to parity; parity is computed in the",
                    numbers.len()
                ),
                "background and the units show DPY/Unprotected until it is done.".into(),
            ];
            if largest > smallest {
                lines.push(format!(
                    "The set uses {smallest} MiB of each unit; larger units leave the rest unused."
                ));
            }
            lines.push(String::new());
            lines.push(format!("     {STATUS_HEADINGS}"));
            lines.extend(chosen.iter().map(|&i| format!("     {}", disk::status_row(&units[i]))));
            if let Some(d) = &hot_spare {
                lines.push(String::new());
                lines.push("This non-configured unit becomes the hot spare; its data is lost:".into());
                lines.push(format!("     {}", nonconfigured_row(d)));
            }
            let device = hot_spare.map(|d| d.device);
            let confirm = if with_spare {
                "dst-parity-start-spare-confirm"
            } else {
                "dst-parity-start-confirm"
            };
            Flow::Push(Box::new(
                Confirm::new(
                    confirm,
                    "Confirm Start Device Parity Protection",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| {
                        let started = env.start_parity(level, &numbers)?;
                        match &device {
                            Some(d) => env
                                .start_hot_spare(d)
                                .map(|m| format!("{m}; {started}")),
                            None => Ok(started),
                        }
                    }),
                )
                .unwinding(2),
            ))
        }),
    )))
}

/// Option 9: a non-configured disk becomes a hot spare.
fn start_spare(env: &mut dyn DstEnv) -> Flow {
    let disks = env.nonconfigured();
    let rows: Vec<String> = disks.iter().map(nonconfigured_row).collect();
    Flow::Push(Box::new(Select::new(
        "dst-spare-start",
        "Start Hot Spare",
        "Type 1 beside the unit to hold as a hot spare, then press Enter.",
        &format!("Opt  {NONCONFIGURED_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            if chosen.len() != 1 {
                return Flow::Error("Type 1 beside one unit".into());
            }
            let d = disks[chosen[0]].clone();
            let lines = vec![
                "This unit becomes a hot spare. When a mirrored unit or a device".to_string(),
                "parity unit no larger than it fails, the spare takes its place and".into(),
                "is rebuilt in the background. Any data on it is lost.".into(),
                String::new(),
                format!("     {NONCONFIGURED_HEADINGS}"),
                format!("     {}", nonconfigured_row(&d)),
            ];
            let device = d.device;
            Flow::Push(Box::new(
                Confirm::new(
                    "dst-spare-start-confirm",
                    "Confirm Start Hot Spare",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| env.start_hot_spare(&device)),
                )
                .unwinding(2),
            ))
        }),
    )))
}

/// Option 10: a hot spare goes back to being a non-configured unit.
fn stop_spare(env: &mut dyn DstEnv) -> Flow {
    let spares: Vec<ParityUnit> = env
        .parity_units()
        .into_iter()
        .filter(|p| p.set == SPARE_SET)
        .collect();
    if spares.is_empty() {
        return Flow::Error(reasons::NO_SPARES.into());
    }
    let rows: Vec<String> = spares.iter().map(parity_row).collect();
    Flow::Push(Box::new(Select::new(
        "dst-spare-stop",
        "Stop Hot Spare",
        "Type 1 beside the hot spare to stop, then press Enter.",
        &format!("Opt  {PARITY_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            if chosen.len() != 1 {
                return Flow::Error("Type 1 beside one unit".into());
            }
            let p = spares[chosen[0]].clone();
            let lines = vec![
                "This unit stops being a hot spare and becomes a non-configured".to_string(),
                "unit; a unit that fails is no longer replaced by it.".into(),
                String::new(),
                PARITY_HEADINGS.to_string(),
                parity_row(&p),
            ];
            let resource = p.resource;
            Flow::Push(Box::new(
                Confirm::new(
                    "dst-spare-stop-confirm",
                    "Confirm Stop Hot Spare",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| env.stop_hot_spare(&resource)),
                )
                .unwinding(2),
            ))
        }),
    )))
}

/// One row per set: its number, level, members and their worst status.
fn sets(env: &mut dyn DstEnv) -> Vec<(u8, String)> {
    let units: Vec<ParityUnit> = env.parity_units().into_iter().filter(|p| p.set != SPARE_SET).collect();
    let mut out: Vec<(u8, String)> = Vec::new();
    for p in &units {
        if out.iter().any(|(s, _)| *s == p.set) {
            continue;
        }
        let members: Vec<&ParityUnit> = units.iter().filter(|u| u.set == p.set).collect();
        let list: Vec<String> = members.iter().map(|u| (u.unit + 1).to_string()).collect();
        let state = members
            .iter()
            .map(|u| u.status.as_str())
            .find(|s| *s != "DPY/Active")
            .unwrap_or("DPY/Active");
        out.push((
            p.set,
            format!("{:>3}  {:<7} {:<16} {}", p.set, p.level, state, list.join(" ")),
        ));
    }
    out
}

/// The heading of the set list.
const SET_HEADINGS: &str = "Opt  Set  Level   Status           Units";

fn stop(env: &mut dyn DstEnv) -> Flow {
    let sets = sets(env);
    let rows: Vec<String> = sets.iter().map(|(_, r)| r.clone()).collect();
    Flow::Push(Box::new(Select::new(
        "dst-parity-stop",
        "Stop Device Parity Protection",
        "Type 1 beside the set to stop, then press Enter.",
        SET_HEADINGS,
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            if chosen.len() != 1 {
                return Flow::Error("Type 1 beside one set".into());
            }
            let (set, row) = sets[chosen[0]].clone();
            let lines = vec![
                "Device parity protection stops for this set. Its units stay in the".to_string(),
                "ASP unprotected and get back the space parity used:".into(),
                String::new(),
                format!("     {}", &SET_HEADINGS[5..]),
                format!("     {row}"),
            ];
            Flow::Push(Box::new(
                Confirm::new(
                    "dst-parity-stop-confirm",
                    "Confirm Stop Device Parity Protection",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| env.stop_parity(set)),
                )
                .unwinding(2),
            ))
        }),
    )))
}

fn include(env: &mut dyn DstEnv) -> Flow {
    let sets = sets(env);
    if sets.is_empty() {
        return Flow::Error("No device parity set to include a unit in".into());
    }
    let units = eligible(env);
    let rows: Vec<String> = units.iter().map(disk::status_row).collect();
    Flow::Push(Box::new(Select::new(
        "dst-parity-include",
        "Include Unit in Device Parity Protection",
        "Type 1 beside the unit to include, then press Enter.",
        &format!("Opt  {STATUS_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            if chosen.len() != 1 {
                return Flow::Error("Type 1 beside one unit".into());
            }
            let unit = units[chosen[0]].clone();
            if sets.len() == 1 {
                return Flow::Push(Box::new(confirm_include(sets[0].clone(), &unit, 2)));
            }
            let rows: Vec<String> = sets.iter().map(|(_, r)| r.clone()).collect();
            let sets = sets.clone();
            Flow::Push(Box::new(Select::new(
                "dst-parity-include-set",
                "Select Parity Set",
                "Type 1 beside the set the unit joins, then press Enter.",
                SET_HEADINGS,
                rows,
                Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
                    if chosen.len() != 1 {
                        return Flow::Error("Type 1 beside one set".into());
                    }
                    Flow::Push(Box::new(confirm_include(sets[chosen[0]].clone(), &unit, 3)))
                }),
            )))
        }),
    )))
}

fn confirm_include((set, row): (u8, String), unit: &DiskUnit, unwind: usize) -> Confirm {
    let number = unit.unit;
    let lines = vec![
        format!("Unit {} joins device parity set {set}:", number + 1),
        String::new(),
        format!("     {}", &SET_HEADINGS[5..]),
        format!("     {row}"),
        String::new(),
        "The set's parity is computed again over its new units in the".into(),
        "background; until then its units show DPY/Unprotected.".into(),
    ];
    Confirm::new(
        "dst-parity-include-confirm",
        "Confirm Include Unit in Device Parity Protection",
        lines,
        Box::new(move |env: &mut dyn DstEnv| env.include_parity(set, number)),
    )
    .unwinding(unwind)
}

fn exclude(env: &mut dyn DstEnv) -> Flow {
    let members: Vec<ParityUnit> = env.parity_units().into_iter().filter(|p| p.set != SPARE_SET).collect();
    let rows: Vec<String> = members.iter().map(parity_row).collect();
    Flow::Push(Box::new(Select::new(
        "dst-parity-exclude",
        "Exclude Unit from Device Parity Protection",
        "Type 1 beside the unit to exclude, then press Enter.",
        &format!("Opt  {PARITY_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            if chosen.len() != 1 {
                return Flow::Error("Type 1 beside one unit".into());
            }
            let p = members[chosen[0]].clone();
            let lines = vec![
                format!(
                    "Unit {} leaves device parity set {} and stays in the ASP",
                    p.unit + 1,
                    p.set
                ),
                "unprotected. The parity of the units left is computed again in".into(),
                "the background.".into(),
                String::new(),
                PARITY_HEADINGS.to_string(),
                parity_row(&p),
            ];
            let unit = p.unit;
            Flow::Push(Box::new(
                Confirm::new(
                    "dst-parity-exclude-confirm",
                    "Confirm Exclude Unit from Device Parity Protection",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| env.exclude_parity(unit)),
                )
                .unwinding(2),
            ))
        }),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(status: &str, percent: Option<u8>) -> String {
        let p = ParityUnit {
            set: 1,
            level: "RAID 5".into(),
            status: status.into(),
            percent,
            ..ParityUnit::default()
        };
        parity_row(&p)
    }

    #[test]
    fn status_words_and_percents() {
        assert!(row("DPY/Active", None).ends_with(" Active"));
        assert!(row("DPY/Failed", None).ends_with(" Failed"));
        assert!(row("DPY/Unprotected", None).ends_with(" Unprotected"));
        assert!(row("DPY/Unprotected", Some(7)).ends_with("   7% Resyncing"));
        assert!(row("DPY/Rebuild", Some(42)).ends_with("  42% Rebuilt"));
    }
}
