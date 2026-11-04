// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The help audit: every shipped UIM panel and display file, every
//! command and parameter and every message of every shipped message
//! file, checked for help. `build test` runs it before the suite;
//! `cargo run -p helpaudit` prints the numbers and what lacks help.
//!
//! What counts (docs/UIM.md "Help everywhere"):
//! - a panel: its own help module, the key list's, and one for every
//!   item F1 can land on (data items, list columns, options with a
//!   legend, menu items with text), from the item, its group or its
//!   area; a module counts when it resolves (this group, an import,
//!   or `GROUP/MODULE`) and has text;
//! - a display file record: a help specification (`H`) or `HLPPNLGRP`
//!   naming a module that resolves;
//! - a command: `HLPPNLGRP` and `HLPID`, module `HLPID` and module
//!   `HLPID/KWD` for each parameter;
//! - a message: second-level text. Every message counts: the oracle
//!   gives second-level text to the messages we ship.

pub mod commands;
pub mod dspfs;
pub mod generate;
pub mod groups;
pub mod messages;
pub mod panels;
pub mod phrasing;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub use groups::Groups;

/// Errors of the audit.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// A source could not be read.
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    /// A shipped source did not compile.
    #[error("{0} does not compile: {1}")]
    Compile(String, String),
}

/// A result of the audit.
pub type Result<T> = std::result::Result<T, AuditError>;

/// One category of the report.
#[derive(Clone, Debug, Default)]
pub struct Category {
    /// Its name.
    pub name: &'static str,
    /// What was checked.
    pub total: usize,
    /// What has help.
    pub covered: usize,
    /// What lacks it, one line each.
    pub missing: Vec<String>,
    /// The percentage the category must reach.
    pub floor: u32,
}

impl Category {
    /// A new category with its floor.
    #[must_use]
    pub fn new(name: &'static str, floor: u32) -> Self {
        Self {
            name,
            floor,
            ..Self::default()
        }
    }

    /// Count one thing; `what` is listed when `ok` is false.
    pub fn check(&mut self, ok: bool, what: impl FnOnce() -> String) {
        self.total += 1;
        if ok {
            self.covered += 1;
        } else {
            self.missing.push(what());
        }
    }

    /// Covered, in hundredths of a percent.
    #[must_use]
    pub fn basis_points(&self) -> u64 {
        if self.total == 0 {
            return 10_000;
        }
        (self.covered as u64 * 10_000) / self.total as u64
    }

    /// Below its floor.
    #[must_use]
    pub fn fails(&self) -> bool {
        self.basis_points() < u64::from(self.floor) * 100
    }
}

/// The audit's report.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// The categories in order.
    pub categories: Vec<Category>,
}

impl Report {
    /// Any category below its floor.
    #[must_use]
    pub fn fails(&self) -> bool {
        self.categories.iter().any(Category::fails)
    }

    /// One line per category.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut s = String::new();
        for c in &self.categories {
            let bp = c.basis_points();
            let _ = writeln!(
                s,
                "{:<18} {:>6} of {:>6}  {:>3}.{:02}%  (floor {}%){}",
                c.name,
                c.covered,
                c.total,
                bp / 100,
                bp % 100,
                c.floor,
                if c.fails() { "  FAILS" } else { "" }
            );
        }
        s
    }

    /// The summary and every missing item.
    #[must_use]
    pub fn details(&self, limit: usize) -> String {
        let mut s = self.summary();
        for c in &self.categories {
            if c.missing.is_empty() {
                continue;
            }
            let _ = writeln!(s, "\n{} without help ({}):", c.name, c.missing.len());
            for m in c.missing.iter().take(limit) {
                let _ = writeln!(s, "  {m}");
            }
            if c.missing.len() > limit {
                let _ = writeln!(s, "  ... and {} more", c.missing.len() - limit);
            }
        }
        s
    }
}

/// Read a file of the tree.
pub(crate) fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| AuditError::Io(path.to_path_buf(), e))
}

/// The files of a directory with an extension, in name order.
pub(crate) fn files(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| AuditError::Io(dir.to_path_buf(), e))?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .collect();
    out.sort();
    Ok(out)
}

/// The stem of a path, upper case.
pub(crate) fn stem(p: &Path) -> String {
    p.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_uppercase()
}

/// Run the audit over the tree at `root`.
pub fn audit(root: &Path) -> Result<Report> {
    let groups = Groups::load(root)?;
    let mut report = Report::default();
    report.categories.push(panels::audit(&groups));
    report.categories.push(dspfs::audit(root, &groups)?);
    let (cmds, parms) = commands::audit(root, &groups)?;
    report.categories.push(cmds);
    report.categories.push(parms);
    report.categories.push(messages::audit(root)?);
    report.categories.push(phrasing::audit(root)?);
    Ok(report)
}

/// The repository root above `dir`.
#[must_use]
pub fn find_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|d| d.join("command/sys-cpps/cmds").is_dir() && d.join("ui/uim/pnlgrp").is_dir())
        .map(Path::to_path_buf)
}
