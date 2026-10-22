// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with Licensed Internal Code`: what the LIC on this system is,
//! release, build and slots, read from the LIC record. Installing fixes
//! to it waits for the PTF phase.

use crate::dst::common::Notice;
use crate::env::DstEnv;

/// The display.
#[must_use]
pub fn work_with_lic(env: &mut dyn DstEnv) -> Notice {
    let info = env.lic_info();
    let width = info.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    let mut lines: Vec<String> = info
        .iter()
        .map(|(k, v)| {
            let mut label = crate::ui::leader(k, width + 1);
            label.truncate(width + 1);
            format!("{label} :  {v}")
        })
        .collect();
    if lines.is_empty() {
        lines.push("  (No information)".into());
    }
    lines.push(String::new());
    lines.push("Installing fixes to the Licensed Internal Code is not available in this".into());
    lines.push("release; the installation media replaces it whole.".into());
    Notice::new("dst-lic", "Work with Licensed Internal Code", lines)
}
