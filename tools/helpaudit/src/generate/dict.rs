// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The written help the generator draws on (`command/sys-cpps/help/`):
//! what each command does and what its parameters and values mean,
//! command by command where the meaning is particular, by keyword and
//! by special value where it is shared.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::{AuditError, Result, read};

/// The directory of the help texts.
pub const DIR: &str = "command/sys-cpps/help";

/// The help texts.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Dict {
    /// Shared keyword texts: `OUTPUT = "..."`.
    #[serde(default)]
    pub keywords: BTreeMap<String, String>,
    /// Shared special value texts: `"*LIBL" = "..."`.
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    /// Special value texts for one keyword: `[kwvalues.OUTPUT]`.
    #[serde(default)]
    pub kwvalues: BTreeMap<String, BTreeMap<String, String>>,
    /// Object types: `"*FILE" = "Files."`, for values that are types.
    #[serde(default)]
    pub objtypes: BTreeMap<String, String>,
    /// Per command: `about`, `KWD` and `"KWD.*VALUE"`.
    #[serde(default)]
    pub commands: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    keywords: BTreeMap<String, String>,
    #[serde(default)]
    values: BTreeMap<String, String>,
    #[serde(default)]
    kwvalues: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(default)]
    objtypes: BTreeMap<String, String>,
    #[serde(flatten)]
    commands: BTreeMap<String, BTreeMap<String, String>>,
}

impl Dict {
    /// Every `*.toml` of the help directory, merged.
    pub fn load(root: &Path) -> Result<Self> {
        let mut d = Self::default();
        for p in crate::files(&root.join(DIR), "toml")? {
            let f: File = toml::from_str(&read(&p)?)
                .map_err(|e| AuditError::Compile(p.display().to_string(), e.to_string()))?;
            d.keywords.extend(f.keywords);
            d.values.extend(f.values);
            d.objtypes.extend(f.objtypes);
            for (k, v) in f.kwvalues {
                d.kwvalues.entry(k).or_default().extend(v);
            }
            for (k, v) in f.commands {
                d.commands.entry(k).or_default().extend(v);
            }
        }
        Ok(d)
    }

    /// The text for a command's entry `key` (`about`, `KWD`, `KWD.*V`).
    #[must_use]
    pub fn command(&self, cmd: &str, key: &str) -> Option<&str> {
        self.commands.get(cmd)?.get(key).map(String::as_str)
    }

    /// What a value means for a keyword of a command; `types` when the
    /// parameter's values are object types.
    #[must_use]
    pub fn value(&self, cmd: &str, kwd: &str, value: &str, types: bool) -> Option<&str> {
        self.command(cmd, &format!("{kwd}.{value}"))
            .or_else(|| {
                self.kwvalues
                    .get(kwd)
                    .and_then(|m| m.get(value))
                    .map(String::as_str)
            })
            .or_else(|| {
                types
                    .then(|| self.objtypes.get(value).map(String::as_str))
                    .flatten()
            })
            .or_else(|| self.values.get(value).map(String::as_str))
    }

    /// Whether a list of values is mostly object types.
    #[must_use]
    pub fn are_types<'a>(&self, values: impl Iterator<Item = &'a str>) -> bool {
        values.filter(|v| self.objtypes.contains_key(*v)).count() >= 5
    }
}
