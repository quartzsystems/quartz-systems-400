// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with System Capacity`: what the machine has, from the Phase 5
//! statistics and the disk units. This system has no capacity on
//! demand: processors and memory are what the hardware reports, and
//! nothing here changes them.

use svctools::DstEnv;
use svctools::dst::common::{Notice, Submenu};
use svctools::panel::Flow;

/// The `Work with System Capacity` menu.
#[must_use]
pub fn work_with_system_capacity() -> Submenu {
    Submenu::new(
        "sst-capacity",
        "Work with System Capacity",
        vec![
            (
                1,
                "Display processor capacity status",
                Some(Box::new(|env: &mut dyn DstEnv| {
                    Flow::Push(Box::new(processor_status(env)))
                })),
            ),
            (2, "Work with temporary capacity", None),
            (3, "Work with permanent capacity", None),
            (
                4,
                "Display storage capacity",
                Some(Box::new(|env: &mut dyn DstEnv| {
                    Flow::Push(Box::new(storage_status(env)))
                })),
            ),
        ],
    )
}

/// The label of the processor count among the environment's
/// `lic_info` lines.
pub const PROCESSORS: &str = "Processors";

/// The processor count the environment reports; 1 when it does not.
fn processors(env: &mut dyn DstEnv) -> String {
    env.lic_info()
        .into_iter()
        .find(|(k, _)| k == PROCESSORS)
        .map_or_else(|| "1".to_string(), |(_, v)| v)
}

/// A figure from the capacity list.
fn figure(cap: &[(String, String)], label: &str) -> Option<String> {
    cap.iter().find(|(k, _)| k == label).map(|(_, v)| v.clone())
}

/// A label dotted to the value column.
fn line(label: &str, value: &str) -> String {
    let mut l = svctools::ui::leader(label, 32);
    l.truncate(32);
    format!("{l}:  {value}")
}

fn processor_status(env: &mut dyn DstEnv) -> Notice {
    let cap = env.capacity();
    let processors = figure(&cap, "Installed processors").unwrap_or_else(|| processors(env));
    let resources = env.resources().len();
    let mut lines = vec![
        line("Installed processors", &processors),
        line("Active processors", &processors),
    ];
    for k in ["Processor use, %", "Main storage, MB", "Storage pools", "Active jobs"] {
        if let Some(v) = figure(&cap, k) {
            lines.push(line(k, &v));
        }
    }
    lines.push(line("Hardware resources reported", &resources.to_string()));
    lines.push(String::new());
    lines.push("Every installed processor is active: this system has no capacity".into());
    lines.push("held in reserve to be activated later.".into());
    Notice::new("sst-capacity-processor", "Display Processor Capacity Status", lines)
}

fn storage_status(env: &mut dyn DstEnv) -> Notice {
    let cap = env.capacity();
    let mut lines: Vec<String> = ["Disk units configured", "System ASP capacity", "System ASP used, %", "ASP threshold, %"]
        .iter()
        .filter_map(|k| figure(&cap, k).map(|v| line(k, &v)))
        .collect();
    lines.push(String::new());
    lines.push("Unit  ASP    Size (MiB)  Used %  Protection".into());
    for u in env.disk_units() {
        lines.push(format!(
            "{:>4}  {:>3}  {:>12}  {:>5}%  {}",
            u.unit,
            u.asp,
            u.size_mib,
            u.percent_used,
            if u.mirrored { "Mirrored" } else { "Unprotected" }
        ));
    }
    Notice::new("sst-capacity-storage", "Display Storage Capacity", lines)
}
