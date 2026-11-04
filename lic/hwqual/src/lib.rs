// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSHWQUAL`, the hardware qualification run
//! (`docs/QUALIFIED_HARDWARE.md`): what the machine has and whether
//! QS/400 can use it, graded `PASS`, `WARN` or `FAIL` line by line in
//! `WRKHDWRSC` resource names, with one result for the whole machine.
//! The same report comes from the install media's menu, the service
//! console's `qshwqual` and the `QSHWQUAL` command.
//!
//! - [`checks`]: the checks, over the resource table and a few sysfs
//!   and procfs files.
//! - [`report`]: the report and its text.
//! - [`usb`]: writing the report to a FAT USB stick.

#![forbid(unsafe_code)]

pub mod checks;
pub mod report;
pub mod usb;

use std::path::{Path, PathBuf};

pub use report::{Check, Grade, Report};

/// Where the checks look.
#[derive(Clone, Debug)]
pub struct Roots {
    /// sysfs (`/sys`).
    pub sys: PathBuf,
    /// procfs (`/proc`).
    pub proc: PathBuf,
}

impl Default for Roots {
    fn default() -> Self {
        Self {
            sys: PathBuf::from("/sys"),
            proc: PathBuf::from("/proc"),
        }
    }
}

/// Run every check over `table` (the hardware resource service's, or a
/// fresh walk) and the files under `roots`, at Unix time `now`.
#[must_use]
pub fn run(table: &hwrsrc::Table, roots: &Roots, now: u64) -> Report {
    checks::all(table, roots, now)
}

/// Walk the sysfs at `roots.sys` and run every check: what the install
/// media and DST do, before the hardware resource service is reachable.
#[must_use]
pub fn run_fresh(roots: &Roots, now: u64) -> Report {
    run(&hwrsrc::Table::scan(&roots.sys), roots, now)
}

/// The name the report is written under on a USB stick.
pub const USB_FILE: &str = "QSHWQUAL.TXT";

/// The sysfs root of a test tree, for doc examples and tests.
#[must_use]
pub fn roots_at(dir: &Path) -> Roots {
    Roots {
        sys: dir.join("sys"),
        proc: dir.join("proc"),
    }
}
