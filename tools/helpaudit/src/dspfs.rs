// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display files: every record format of every shipped display file
//! (`ui/dspf/sys/`) has a help specification whose module resolves.

use std::path::Path;

use crate::{AuditError, Category, Groups, Result, files, read, stem};

/// The shipped display file sources.
pub const DIR: &str = "ui/dspf/sys";

/// Audit every record format.
pub fn audit(root: &Path, groups: &Groups) -> Result<Category> {
    let mut c = Category::new("display file help", 100);
    for p in files(&root.join(DIR), "dspf")? {
        let name = stem(&p);
        let (src, _) = dspf::Source::from_text(&read(&p)?);
        let comp = dspf::compile_dspf(&src, &dspf::NoRefs, &dspf::CompileOpts::default());
        let Some(df) = comp.definition else {
            return Err(AuditError::Compile(format!("{DIR}/{name}"), comp.listing));
        };
        for rec in &df.records {
            let specs = dspf::help::specs(rec);
            let ok = !specs.is_empty()
                && specs.iter().all(|s| {
                    let grp = s.group.rsplit('/').next().unwrap_or_default();
                    groups
                        .get(grp)
                        .is_some_and(|g| groups.has_text(g, Some(&s.module)))
                });
            c.check(ok, || format!("{DIR}/{name}.dspf record {}", rec.name));
        }
    }
    Ok(c)
}
