// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages: second-level text for every message of every shipped
//! message file (`runtime/msgf/msgs/`).

use std::path::Path;

use crate::{AuditError, Category, Result, files, read};

/// The message file sources.
pub const DIR: &str = "runtime/msgf/msgs";

/// Audit every message.
pub fn audit(root: &Path) -> Result<Category> {
    let mut c = Category::new("message help", 100);
    for p in files(&root.join(DIR), "toml")? {
        let f = msgf::parse(&read(&p)?)
            .map_err(|e| AuditError::Compile(p.display().to_string(), e.to_string()))?;
        for m in &f.msgs {
            c.check(!m.help.trim().is_empty(), || format!("{} {}", f.name, m.id));
        }
    }
    Ok(c)
}
