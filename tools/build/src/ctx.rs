// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Shared state for one orchestrator invocation.

use std::num::NonZero;

use anyhow::Result;

use crate::host::paths::Paths;

/// Everything a subcommand needs besides its own arguments.
pub struct Ctx {
    /// Repository and target directories.
    pub paths: Paths,
    /// Stream tool output to the terminal instead of only to log files.
    pub verbose: bool,
    /// Parallel jobs for `make`.
    pub jobs: usize,
}

impl Ctx {
    /// Discover paths and host parallelism.
    pub fn new(verbose: bool) -> Result<Self> {
        let paths = Paths::discover()?;
        let jobs = std::thread::available_parallelism().map_or(1, NonZero::get);
        Ok(Self {
            paths,
            verbose,
            jobs,
        })
    }
}
