// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Shared state for one orchestrator invocation.

use std::num::NonZero;

use anyhow::Result;

use crate::host::paths::Paths;
use crate::product::Product;

/// Everything a step needs besides its own arguments.
pub struct Ctx {
    /// What is being built.
    pub product: Product,
    /// Repository and target directories.
    pub paths: Paths,
    /// Stream tool output to the terminal instead of only to log files.
    pub verbose: bool,
    /// Parallel jobs for `make`.
    pub jobs: usize,
}

impl Ctx {
    /// Paths from the product's repository and environment, and the
    /// host's parallelism.
    pub fn new(product: Product, verbose: bool) -> Result<Self> {
        let paths = Paths::of(&product)?;
        // `<PREFIX>JOBS` caps the kernel's `make -j` (a shared PC); else every core.
        let jobs = product
            .env("JOBS")
            .and_then(|s| s.to_str().and_then(|s| s.parse::<usize>().ok()))
            .filter(|&n| n > 0)
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, NonZero::get));
        Ok(Self {
            product,
            paths,
            verbose,
            jobs,
        })
    }
}
