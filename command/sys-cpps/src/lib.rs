// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system command set: every `*CMD` of `QSYS` compiled from a
//! definition source under `cmds/`, the inventory that says which
//! processing program runs each one and where, the installer that
//! puts them in `QSYS`, and the bridge the processing programs use to
//! hand a parameter block to the library functions of earlier phases.
//!
//! IBM's command names, keywords, types, defaults and special values
//! are the interface; every prompt text and message is Quartz-written
//! (`tests/defs.rs` keeps a denylist of IBM phrasings).

#![deny(missing_docs)]

pub mod bridge;
pub mod install;
pub mod inventory;

/// The definition sources, by name (generated from `cmds/`).
pub mod defs {
    include!(concat!(env!("OUT_DIR"), "/defs.rs"));

    /// The source of a command, by name.
    #[must_use]
    pub fn source(name: &str) -> Option<&'static str> {
        DEFS.binary_search_by(|(n, _)| (*n).cmp(name))
            .ok()
            .map(|i| DEFS[i].1)
    }
}

pub use inventory::{Entry, Site, all, entry};
