// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The installer's screens: Licensed Internal Code onto a load source,
//! then the operating system, then the IPL options. The work itself is
//! the environment's; the panels here ask, confirm, and show progress.

pub mod define;
pub mod lic;
pub mod noos;
pub mod options;
pub mod os;

use console::Screen;

use crate::env::DstEnv;
use crate::ui;

/// Where an installation stands, for the status displays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// Steps in order, each with whether it is done.
    pub steps: Vec<(String, bool)>,
    /// Percent of the current step, when it has one.
    pub percent: Option<u8>,
    /// A line under the steps.
    pub note: String,
}

impl Progress {
    /// With these step names, none done.
    #[must_use]
    pub fn new(steps: &[&str]) -> Self {
        Self {
            steps: steps.iter().map(|s| ((*s).to_string(), false)).collect(),
            percent: None,
            note: String::new(),
        }
    }

    /// Mark step `i` done.
    pub fn done(&mut self, i: usize) {
        if let Some(s) = self.steps.get_mut(i) {
            s.1 = true;
        }
        self.percent = None;
    }

    /// The status screen: each step with a mark, the percent bar when
    /// a step reports one.
    #[must_use]
    pub fn screen(&self, title: &str, system: &str) -> Screen {
        let mut lines: Vec<String> = self
            .steps
            .iter()
            .map(|(s, done)| format!("  {}  {s}", if *done { "X" } else { " " }))
            .collect();
        if !self.note.is_empty() {
            lines.push(String::new());
            lines.push(self.note.clone());
        }
        ui::text(title, system, &lines, self.percent, "")
    }
}

/// A status screen for `Initialize the Disk - Status`: one bar.
#[must_use]
pub fn disk_status(system: &str, device: &str, percent: u8) -> Screen {
    ui::text(
        "Initialize the Disk - Status",
        system,
        &[
            format!("Load source device . . . :  {device}"),
            String::new(),
            "Writing the partition table and the licensed internal code.".to_string(),
        ],
        Some(percent),
        "",
    )
}

/// The steps of the LIC install, in order.
pub const LIC_STEPS: [&str; 5] = [
    "Partition the load source device",
    "Initialize the LIC partition",
    "Copy the Licensed Internal Code to slot A",
    "Copy the Licensed Internal Code to slot B",
    "Write the LIC record",
];

/// The stages of the operating system install, in order: the status
/// display ticks them off as the `QS00SS1` load goes in.
pub const OS_STEPS: [&str; 6] = [
    "Stage 1  Prepare ASP 1 and create the system libraries",
    "Stage 2  Restore the operating system's code into QSYS",
    "Stage 3  Restore the language objects",
    "Stage 4  Check signatures and translate programs again",
    "Stage 5  Create the system values, profiles and configuration",
    "Stage 6  Complete the operating system install",
];

/// The stage of [`OS_STEPS`] each report names.
pub mod stage {
    /// ASP 1 and the libraries.
    pub const LIBRARIES: usize = 0;
    /// The code load restored.
    pub const CODE: usize = 1;
    /// The language load restored.
    pub const LANGUAGE: usize = 2;
    /// Signatures and retranslation.
    pub const CHECK: usize = 3;
    /// System values and the recovery's data.
    pub const VALUES: usize = 4;
    /// Done.
    pub const DONE: usize = 5;
}

/// Render the LIC status screen.
pub fn lic_status(env: &dyn DstEnv, p: &Progress) -> Screen {
    p.screen(
        "Install Licensed Internal Code - Status",
        &env.system_name(),
    )
}

/// Render the operating system status screen.
pub fn os_status(env: &dyn DstEnv, p: &Progress) -> Screen {
    p.screen("Install the Operating System - Status", &env.system_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_marks_steps() {
        let mut p = Progress::new(&LIC_STEPS);
        p.done(0);
        p.percent = Some(30);
        let d = p.screen("Status", "QS400").dump();
        assert!(d.contains("  X  Partition the load source device"));
        assert!(d.contains("     Initialize the LIC partition"));
        assert!(d.contains(" 30%"));
        p.done(1);
        assert_eq!(p.percent, None);
    }
}
