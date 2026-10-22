// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The inventory: for every command of `cmds/`, where its processing
//! program runs and what it is called. `inventory.toml` holds what the
//! defaults do not say; everything else is derived from the name.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// The inventory's source.
pub const INVENTORY: &str = include_str!("../inventory.toml");

/// Where a command's processing program runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Site {
    /// In the storage service, over the store (most commands).
    #[default]
    Service,
    /// In the job process: calls, the debugger, the job's own displays.
    Job,
    /// A stub: the processing program sends the message named and
    /// does nothing (a later phase's function).
    Stub,
}

/// One command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The command name.
    pub name: String,
    /// Where it runs.
    pub site: Site,
    /// The processing program, in `QSYS`.
    pub cpp: String,
    /// The completion message the processor sends when it has no
    /// better one, with the parameters whose values are its data.
    pub comp: Option<(String, Vec<String>)>,
    /// For a stub: the message it sends.
    pub stub: Option<String>,
    /// The console word of the Phase 2 to 6 library function the
    /// bridge calls; the command name in lower case by default.
    pub word: String,
    /// A note for the inventory table (`PHASE7.md`).
    pub note: String,
    /// `ALWLMTUSR(*YES)`: a limited user may type it.
    pub alwlmtusr: bool,
}

#[derive(Deserialize, Default)]
struct Over {
    #[serde(default)]
    site: Option<Site>,
    #[serde(default)]
    cpp: Option<String>,
    #[serde(default)]
    comp: Option<String>,
    #[serde(default)]
    comp_data: Vec<String>,
    #[serde(default)]
    stub: Option<String>,
    #[serde(default)]
    word: Option<String>,
    #[serde(default)]
    note: String,
    #[serde(default)]
    alwlmtusr: bool,
}

#[derive(Deserialize)]
struct Table {
    #[serde(default)]
    cmd: HashMap<String, Over>,
}

fn overrides() -> &'static HashMap<String, Over> {
    static T: OnceLock<HashMap<String, Over>> = OnceLock::new();
    T.get_or_init(|| {
        let t: Table = toml::from_str(INVENTORY).expect("inventory.toml parses");
        t.cmd
    })
}

/// The processing program's name for a command without one in the
/// inventory: `QC` and the first eight characters of the name.
#[must_use]
pub fn default_cpp(name: &str) -> String {
    let head: String = name.chars().take(8).collect();
    format!("QC{head}")
}

/// Every command, in name order.
pub fn all() -> &'static [Entry] {
    static T: OnceLock<Vec<Entry>> = OnceLock::new();
    T.get_or_init(|| {
        let over = overrides();
        crate::defs::DEFS
            .iter()
            .map(|(name, _)| {
                let o = over.get(*name);
                let site = o.and_then(|o| o.site).unwrap_or_default();
                Entry {
                    name: (*name).to_string(),
                    site: if o.and_then(|o| o.stub.as_ref()).is_some() {
                        Site::Stub
                    } else {
                        site
                    },
                    cpp: o
                        .and_then(|o| o.cpp.clone())
                        .unwrap_or_else(|| default_cpp(name)),
                    comp: o.and_then(|o| o.comp.clone().map(|c| (c, o.comp_data.clone()))),
                    stub: o.and_then(|o| o.stub.clone()),
                    word: o
                        .and_then(|o| o.word.clone())
                        .unwrap_or_else(|| name.to_ascii_lowercase()),
                    note: o.map(|o| o.note.clone()).unwrap_or_default(),
                    alwlmtusr: o.is_some_and(|o| o.alwlmtusr),
                }
            })
            .collect()
    })
}

/// A command's entry.
#[must_use]
pub fn entry(name: &str) -> Option<&'static Entry> {
    all().iter().find(|e| e.name == name)
}

/// The command a processing program name belongs to.
#[must_use]
pub fn by_cpp(cpp: &str) -> Option<&'static Entry> {
    all().iter().find(|e| e.cpp == cpp)
}

/// Overrides the inventory names for commands that have no definition
/// source (a test catches them).
#[must_use]
pub fn orphans() -> Vec<String> {
    overrides()
        .keys()
        .filter(|k| crate::defs::source(k).is_none())
        .cloned()
        .collect()
}
