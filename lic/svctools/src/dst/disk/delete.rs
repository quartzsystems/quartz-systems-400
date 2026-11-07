// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Delete disk unit data` (Work with disk unit recovery, option 10):
//! make non-configured disks blank. Only disks outside the configuration
//! are offered: never a configured unit, the load source, a mirrored
//! half, a parity set member, the install media or a disk with a
//! mounted file system.

use console::Screen;

use crate::dst::common::{Confirm, Select};
use crate::dst::disk::config::{NONCONFIGURED_HEADINGS, nonconfigured_row};
use crate::env::{DstEnv, NonConfigured};
use crate::panel::Flow;
use crate::ui;

/// Whether `node` is the whole disk `disk` or one of its partitions
/// (`/dev/sda1` of `/dev/sda`, `/dev/nvme0n1p2` of `/dev/nvme0n1`).
#[must_use]
pub fn on_disk(node: &str, disk: &str) -> bool {
    if node == disk {
        return true;
    }
    let Some(rest) = node.strip_prefix(disk) else {
        return false;
    };
    let rest = if disk.ends_with(|c: char| c.is_ascii_digit()) {
        match rest.strip_prefix('p') {
            Some(r) => r,
            None => return false,
        }
    } else {
        rest
    };
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

/// The disks whose data may be deleted: every whole disk the system
/// sees, less those holding a configured unit and the reserved ones.
pub fn deletable(env: &mut dyn DstEnv) -> Vec<NonConfigured> {
    let mut taken: Vec<String> = env
        .disk_units()
        .into_iter()
        .map(|u| u.device)
        .filter(|d| !d.is_empty())
        .collect();
    taken.extend(env.reserved_disks());
    // Members of a parity set, by resource and serial as the parity
    // displays name them, even when their device is not known.
    let parity: Vec<(String, String)> = env
        .parity_units()
        .into_iter()
        .map(|p| (p.resource, p.serial))
        .collect();
    let in_parity = |d: &NonConfigured| {
        parity.iter().any(|(r, s)| {
            (!r.is_empty() && !r.starts_with('-') && *r == d.resource)
                || (!s.is_empty() && *s == d.serial)
        })
    };
    env.disks()
        .into_iter()
        .filter(|d| !taken.iter().any(|t| on_disk(t, &d.device)) && !in_parity(d))
        .collect()
}

/// `Select Disk Units for Delete Disk Unit Data`.
pub fn select(env: &mut dyn DstEnv) -> Select {
    let disks = deletable(env);
    let rows: Vec<String> = disks.iter().map(nonconfigured_row).collect();
    Select::new(
        "dst-delete-data",
        "Select Disk Units for Delete Disk Unit Data",
        "Type 1 beside the non-configured units to make blank, then press Enter.",
        &format!("Opt  {NONCONFIGURED_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            let picked: Vec<NonConfigured> = chosen.iter().map(|&i| disks[i].clone()).collect();
            Flow::Push(Box::new(confirm(picked)))
        }),
    )
}

fn confirm(picked: Vec<NonConfigured>) -> Confirm {
    let mut lines = vec![
        "WARNING: all data on these units is lost and cannot be recovered:".to_string(),
        "partitions, QS/400 unit headers and any other system's data.".into(),
        String::new(),
    ];
    // Room for six rows on the display.
    let shown = if picked.len() > 6 { 5 } else { picked.len() };
    lines.extend(
        picked[..shown]
            .iter()
            .map(|d| format!("  {}", nonconfigured_row(d))),
    );
    if shown < picked.len() {
        lines.push(format!("  and {} more", picked.len() - shown));
    }
    lines.extend([
        String::new(),
        "The partition tables, the first and last 8 MiB and every partition's".into(),
        "start are written with zeros; the rest is discarded where the drive".into(),
        "supports it. This is a quick delete, not an overwrite of every page.".into(),
        String::new(),
        "Press F10 to delete the data, F12 to go back without a change.".into(),
    ]);
    Confirm::new(
        "dst-delete-data-confirm",
        "Confirm Delete Disk Unit Data",
        lines,
        Box::new(move |env: &mut dyn DstEnv| {
            let total = picked.len();
            for (i, d) in picked.iter().enumerate() {
                let system = env.system_name();
                let name = d.resource.clone();
                let device = d.device.clone();
                env.delete_unit_data(&d.device, &mut |p: u8, env: &mut dyn DstEnv| {
                    env.show(status(&system, &name, &device, i + 1, total, p));
                    if p.is_multiple_of(25) {
                        env.pace();
                    }
                })?;
            }
            Ok(format!(
                "Data deleted on {total} unit(s); they are non-configured and blank"
            ))
        }),
    )
    .unwinding(2)
}

/// `Function Status` while a unit is made blank.
#[must_use]
pub fn status(
    system: &str,
    resource: &str,
    device: &str,
    n: usize,
    of: usize,
    percent: u8,
) -> Screen {
    ui::text(
        "Function Status",
        system,
        &[
            "Delete disk unit data is running. Do not power off the system.".to_string(),
            String::new(),
            format!(
                "Unit  . . . . . . :  {resource}  {}",
                device.strip_prefix("/dev/").unwrap_or(device)
            ),
            format!("Unit number . . . :  {n} of {of}"),
        ],
        Some(percent),
        "",
    )
}

#[cfg(test)]
mod tests {
    use super::on_disk;

    #[test]
    fn partitions_belong_to_their_disk() {
        assert!(on_disk("/dev/sda", "/dev/sda"));
        assert!(on_disk("/dev/sda4", "/dev/sda"));
        assert!(!on_disk("/dev/sdab", "/dev/sda"));
        assert!(!on_disk("/dev/sdb1", "/dev/sda"));
        assert!(on_disk("/dev/nvme0n1p2", "/dev/nvme0n1"));
        assert!(!on_disk("/dev/nvme0n12", "/dev/nvme0n1"));
        assert!(!on_disk("/dev/vda", "/dev/vdaa"));
    }
}
