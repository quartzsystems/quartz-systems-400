// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with disk units`: configuration, mirrored protection and
//! recovery. Rows are built from what the environment reports.

pub mod config;
pub mod mirror;
pub mod recovery;

use crate::dst::common::{Notice, Submenu};
use crate::env::{DiskUnit, DstEnv};
use crate::panel::Flow;
use crate::users::Privilege;

/// The `Work with disk units` menu.
#[must_use]
pub fn work_with_disk_units(privileges: Privilege) -> Submenu {
    let admin = privileges.has(Privilege::DISK_UNITS_ADMIN);
    let ops = privileges.has(Privilege::DISK_UNITS_OPERATIONS);
    Submenu::new(
        "dst-disk",
        "Work with Disk Units",
        vec![
            (
                1,
                "Work with disk configuration",
                Some(Box::new(move |_env| {
                    Flow::Push(Box::new(config::work_with_disk_configuration(admin)))
                })),
            ),
            (
                2,
                "Work with disk unit recovery",
                Some(Box::new(move |_env| {
                    Flow::Push(Box::new(recovery::work_with_disk_unit_recovery(ops, admin)))
                })),
            ),
            (3, "Work with disk unit problem recovery", None),
            (4, "Work with disk unit tracing", None),
        ],
    )
}

/// `ASP Unit  Serial Number     Type Model    Resource Name  Status`.
pub const STATUS_HEADINGS: &str = "ASP Unit  Serial Number     Type Model    Resource  Status";

/// One row of the status display.
#[must_use]
pub fn status_row(u: &DiskUnit) -> String {
    let status = match (u.mirrored, u.resume_percent, u.attached) {
        (false, _, _) => "Unprotected".to_string(),
        (true, Some(p), _) => format!("Resuming {p:>3}%"),
        (true, None, _) => u.status.clone(),
    };
    format!(
        "{:>3} {:>4}  {:<17} {:<13} {:<9} {}",
        u.asp,
        u.unit + 1,
        clip(&u.serial, 17),
        clip(&u.type_model, 13),
        u.resource,
        status
    )
}

/// `ASP Unit  Size (MiB)  Used %  Protection`.
pub const CAPACITY_HEADINGS: &str = "ASP Unit   Size (MiB)  Used %  Protection";

/// One row of the capacity display.
#[must_use]
pub fn capacity_row(u: &DiskUnit) -> String {
    format!(
        "{:>3} {:>4}  {:>11}  {:>5}%  {}",
        u.asp,
        u.unit + 1,
        u.size_mib,
        u.percent_used,
        if u.mirrored {
            "Mirrored"
        } else {
            "Unprotected"
        }
    )
}

/// Cut a string to `n` characters.
#[must_use]
pub fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// A display of every configured unit, one line each.
pub fn display(
    name: &'static str,
    title: &'static str,
    headings: &str,
    env: &mut dyn DstEnv,
    row: fn(&DiskUnit) -> String,
) -> Notice {
    let mut lines = vec![headings.to_string()];
    let units = env.disk_units();
    if units.is_empty() {
        lines.push("  (No configured units)".into());
    }
    lines.extend(units.iter().map(row));
    Notice::new(name, title, lines)
}
