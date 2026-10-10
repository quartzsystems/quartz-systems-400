// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with mirrored protection`: start and stop.

use crate::dst::common::{Confirm, Select, Submenu};
use crate::dst::disk::config::nonconfigured_row;
use crate::dst::disk::{self, STATUS_HEADINGS};
use crate::env::{DiskUnit, DstEnv, NonConfigured};
use crate::panel::Flow;

/// The `Work with Mirrored Protection` menu.
#[must_use]
pub fn work_with_mirrored_protection() -> Submenu {
    Submenu::new(
        "dst-mirror",
        "Work with Mirrored Protection",
        vec![
            (
                1,
                "Display disk configuration",
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
                "Start mirrored protection",
                Some(Box::new(|env| Flow::Push(Box::new(start(env))))),
            ),
            (
                3,
                "Stop mirrored protection",
                Some(Box::new(|env| Flow::Push(Box::new(stop(env))))),
            ),
            (4, "Enable remote load source mirroring", None),
        ],
    )
}

/// Pair unprotected units with non-configured disks: each unit gets
/// the first spare disk at least as large, a different disk preferred.
#[must_use]
pub fn propose_pairs(
    units: &[DiskUnit],
    spare: &[NonConfigured],
) -> Vec<(DiskUnit, NonConfigured)> {
    let mut free: Vec<NonConfigured> = spare.to_vec();
    let mut pairs = Vec::new();
    for u in units.iter().filter(|u| !u.mirrored) {
        let pick = free
            .iter()
            .position(|d| d.size_mib >= u.size_mib && d.resource != u.resource)
            .or_else(|| free.iter().position(|d| d.size_mib >= u.size_mib));
        if let Some(i) = pick {
            pairs.push((u.clone(), free.remove(i)));
        }
    }
    pairs
}

fn start(_env: &mut dyn DstEnv) -> Select {
    let rows = vec!["  1  ASP 1 (system ASP)".to_string()];
    Select::new(
        "dst-mirror-start-asp",
        "Select ASP to Start Mirrored Protection",
        "Type 1 beside the ASP, then press Enter.",
        "Opt  ASP",
        rows,
        Box::new(|_chosen: &[usize], env: &mut dyn DstEnv| {
            let units = env.disk_units();
            let spare = env.nonconfigured();
            let pairs = propose_pairs(&units, &spare);
            if pairs.is_empty() {
                return Flow::Error(
                    "No non-configured unit of sufficient size for an unprotected unit".into(),
                );
            }
            let mut lines = vec![
                "The following pairs will be formed. The second unit of each pair".to_string(),
                "is copied from the first at the next IPL.".into(),
                String::new(),
                format!("     {STATUS_HEADINGS}"),
            ];
            for (u, d) in &pairs {
                lines.push(format!("  {}", disk::status_row(u)));
                lines.push(format!("     {}", nonconfigured_row(d)));
            }
            let plan: Vec<(u16, String)> = pairs
                .iter()
                .map(|(u, d)| (u.unit, d.device.clone()))
                .collect();
            Flow::Push(Box::new(
                Confirm::new(
                    "dst-mirror-start-confirm",
                    "Confirm Start Mirrored Protection",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| {
                        let mut formed = 0;
                        for (unit, device) in &plan {
                            env.start_mirror(*unit, device)?;
                            formed += 1;
                        }
                        Ok(format!(
                            "{formed} pair(s) formed; mirrored protection synchronization runs at the next IPL"
                        ))
                    }),
                )
                .unwinding(2),
            ))
        }),
    )
}

fn stop(env: &mut dyn DstEnv) -> Select {
    let units: Vec<DiskUnit> = env
        .disk_units()
        .into_iter()
        .filter(|u| u.mirrored)
        .collect();
    let rows: Vec<String> = units.iter().map(disk::status_row).collect();
    Select::new(
        "dst-mirror-stop",
        "Stop Mirrored Protection",
        "Type 1 beside the half to keep, then press Enter. Its partner leaves the configuration.",
        &format!("Opt  {STATUS_HEADINGS}"),
        rows,
        Box::new(move |chosen: &[usize], _env: &mut dyn DstEnv| {
            let keep: Vec<u16> = chosen.iter().map(|&i| units[i].unit).collect();
            let mut lines = vec![
                "Mirrored protection stops for these units; the other half of each".to_string(),
                "pair is removed from the configuration:".into(),
                String::new(),
            ];
            lines.extend(
                chosen
                    .iter()
                    .map(|&i| format!("  {}", disk::status_row(&units[i]))),
            );
            Flow::Push(Box::new(
                Confirm::new(
                    "dst-mirror-stop-confirm",
                    "Confirm Stop Mirrored Protection",
                    lines,
                    Box::new(move |env: &mut dyn DstEnv| {
                        for k in &keep {
                            env.stop_mirror(*k)?;
                        }
                        Ok(format!(
                            "Mirrored protection stopped for {} unit(s)",
                            keep.len()
                        ))
                    }),
                )
                .unwinding(2),
            ))
        }),
    )
}
