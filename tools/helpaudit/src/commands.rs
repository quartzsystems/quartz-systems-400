// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Commands: `HLPPNLGRP` and `HLPID` naming a shipped group with the
//! command's module and one `HLPID/KWD` per parameter.

use std::path::Path;

use cmd::def::CmdDef;

use crate::{AuditError, Category, Groups, Result, files, read, stem};

/// The shipped definitions.
pub const DIR: &str = "command/sys-cpps/cmds";

/// Every shipped definition, compiled, in name order.
pub fn definitions(root: &Path) -> Result<Vec<(CmdDef, String)>> {
    let mut out = Vec::new();
    for p in files(&root.join(DIR), "cmd")? {
        let name = stem(&p);
        let text = read(&p)?;
        let def = cmd::source::compile(&name, &text).map_err(|d| {
            AuditError::Compile(format!("{DIR}/{name}.cmd"), format!("{d:?}"))
        })?;
        out.push((def, text));
    }
    Ok(out)
}

/// The parameters a prompt shows (a constant has none).
pub fn prompted(def: &CmdDef) -> impl Iterator<Item = &cmd::def::Parm> {
    def.parms.iter().filter(|p| p.constant.is_none())
}

/// Audit the commands and their parameters.
pub fn audit(root: &Path, groups: &Groups) -> Result<(Category, Category)> {
    let mut cmds = Category::new("command help", 100);
    let mut parms = Category::new("parameter help", 100);
    for (def, _) in definitions(root)? {
        let group = def.hlppnlgrp.as_ref().and_then(|q| groups.get(&q.name));
        let id = def.hlpid.clone().unwrap_or_default();
        let ok = group.is_some_and(|g| !id.is_empty() && groups.has_text(g, Some(&id)));
        cmds.check(ok, || {
            format!(
                "{} (HLPPNLGRP {} HLPID {})",
                def.name,
                def.hlppnlgrp.as_ref().map_or("*NONE", |q| q.name.as_str()),
                if id.is_empty() { "*NONE" } else { id.as_str() }
            )
        });
        for p in prompted(&def) {
            let module = format!("{id}/{}", p.kwd);
            let ok = group.is_some_and(|g| !id.is_empty() && groups.has_text(g, Some(&module)));
            parms.check(ok, || format!("{} {}", def.name, p.kwd));
        }
    }
    Ok((cmds, parms))
}
