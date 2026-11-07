// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The help generators: the command help groups from the definitions.

pub mod cmdhelp;
pub mod dict;
pub mod panels;
pub mod uimtext;

use std::path::Path;

use crate::{AuditError, Result};

/// Write the command help groups; the number written.
pub fn write(root: &Path) -> Result<usize> {
    let groups = cmdhelp::generate(root)?;
    for (name, src) in &groups {
        let p = root.join(cmdhelp::OUT_DIR).join(format!("{name}.pnlgrp"));
        std::fs::write(&p, src).map_err(|e| AuditError::Io(p.clone(), e))?;
    }
    Ok(groups.len())
}

/// The command help groups that differ from what the generator makes.
pub fn stale(root: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for (name, src) in cmdhelp::generate(root)? {
        let p = root.join(cmdhelp::OUT_DIR).join(format!("{name}.pnlgrp"));
        let on_disk = std::fs::read_to_string(&p)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        if on_disk != src {
            out.push(name);
        }
    }
    Ok(out)
}
