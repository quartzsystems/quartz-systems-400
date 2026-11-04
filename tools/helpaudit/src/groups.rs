// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every shipped panel group, compiled, and help module lookup across
//! them as the dialog engine does it.

use std::collections::BTreeMap;
use std::path::Path;

use uim::model::{HelpModule, PanelGroup};

use crate::{AuditError, Result, files, read, stem};

/// The directories of shipped panel groups.
pub const DIRS: &[&str] = &["ui/uim/pnlgrp", "ui/menus/sys", "ui/menus/pnlgrp"];

/// The shipped panel groups by name.
#[derive(Default)]
pub struct Groups {
    /// Name to group, with the directory it came from.
    pub by_name: BTreeMap<String, (PanelGroup, &'static str)>,
}

impl Groups {
    /// Compile every shipped group under `root`.
    pub fn load(root: &Path) -> Result<Self> {
        let mut g = Self::default();
        for dir in DIRS {
            for p in files(&root.join(dir), "pnlgrp")? {
                let name = stem(&p);
                let text = read(&p)?;
                let c = uim::compile(
                    &text,
                    &uim::NoImbeds,
                    &uim::NoImports,
                    &uim::CompileOpts::default(),
                );
                let Some(group) = c.group.filter(|_| c.diagnostics.ok()) else {
                    return Err(AuditError::Compile(format!("{dir}/{name}"), c.listing));
                };
                g.by_name.insert(name, (group, dir));
            }
        }
        Ok(g)
    }

    /// A group by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&PanelGroup> {
        self.by_name.get(&name.to_ascii_uppercase()).map(|(g, _)| g)
    }

    /// The module `name` seen from `group`: its own, an import's, or a
    /// qualified `GROUP/MODULE` or `LIB/GROUP/MODULE` (module names may
    /// hold `/` themselves, as `CMD/KWD` does).
    #[must_use]
    pub fn resolve<'a>(&'a self, group: &'a PanelGroup, name: &str) -> Option<&'a HelpModule> {
        let name = name.trim().to_ascii_uppercase();
        if let Some(h) = group.help(&name) {
            return Some(h);
        }
        for i in &group.imports {
            if i.name.eq_ignore_ascii_case(&name) || i.name == "*" {
                let grp = i.pnlgrp.rsplit('/').next().unwrap_or_default();
                if let Some(h) = self.get(grp).and_then(|g| g.help(&name)) {
                    return Some(h);
                }
            }
        }
        let parts: Vec<&str> = name.split('/').collect();
        for skip in 1..parts.len().min(3) {
            let grp = parts[skip - 1];
            let module = parts[skip..].join("/");
            if let Some(h) = self.get(grp).and_then(|g| g.help(&module)) {
                return Some(h);
            }
        }
        None
    }

    /// `name` resolves to a module with text.
    #[must_use]
    pub fn has_text(&self, group: &PanelGroup, name: Option<&str>) -> bool {
        name.and_then(|n| self.resolve(group, n))
            .is_some_and(|h| !h.body.is_empty())
    }
}
