// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What TCP/IP needs to know of line descriptions. Communications
//! configuration (`*LIND`, `VRYCFG`) implements [`LineState`];
//! [`KernelNames`] stands in until it does: a line named like a kernel
//! link (`LIND(ETH0)` for `eth0`) exists and is varied on.

/// A line description as TCP/IP sees it.
pub trait LineState {
    /// Whether line description `lind` exists.
    fn exists(&self, lind: &str) -> bool;
    /// The kernel link the line runs on, when it has one.
    fn interface(&self, lind: &str) -> Option<String>;
    /// Whether the line is varied on.
    fn varied_on(&self, lind: &str) -> bool;
    /// Vary the line on, as `STRTCPIFC` does for a line that is off.
    fn vary_on(&mut self, lind: &str) -> Result<(), String>;
    /// The line type (`*ELAN` for Ethernet).
    fn line_type(&self, lind: &str) -> Option<String> {
        self.exists(lind).then(|| "*ELAN".to_string())
    }
}

/// No line descriptions at all.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoLines;

impl LineState for NoLines {
    fn exists(&self, _: &str) -> bool {
        false
    }

    fn interface(&self, _: &str) -> Option<String> {
        None
    }

    fn varied_on(&self, _: &str) -> bool {
        false
    }

    fn vary_on(&mut self, lind: &str) -> Result<(), String> {
        Err(format!("line {lind} does not exist"))
    }
}

/// Lines named after kernel links: `LIND(ETH0)` is `eth0`. Every line
/// is varied on. A stand-in for the communications objects.
#[derive(Clone, Debug, Default)]
pub struct KernelNames {
    /// The kernel's link names.
    pub links: Vec<String>,
}

impl KernelNames {
    /// The lines over these links.
    pub fn new(links: impl IntoIterator<Item = String>) -> Self {
        Self {
            links: links.into_iter().filter(|l| l != "lo").collect(),
        }
    }
}

impl LineState for KernelNames {
    fn exists(&self, lind: &str) -> bool {
        self.interface(lind).is_some()
    }

    fn interface(&self, lind: &str) -> Option<String> {
        self.links.iter().find(|l| l.eq_ignore_ascii_case(lind)).cloned()
    }

    fn varied_on(&self, lind: &str) -> bool {
        self.exists(lind)
    }

    fn vary_on(&mut self, lind: &str) -> Result<(), String> {
        if self.exists(lind) {
            Ok(())
        } else {
            Err(format!("line {lind} does not exist"))
        }
    }
}
