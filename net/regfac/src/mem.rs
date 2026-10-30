// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A registry in memory, for tests and for a caller that holds the
//! registry itself.

use crate::call::{Called, ExitRegistry};
use crate::error::RegResult;
use crate::model::ExitProgram;
use crate::registry::Registry;

/// A registry and the refusals reported to it.
#[derive(Clone, Debug, Default)]
pub struct MemRegistry {
    /// The points.
    pub registry: Registry,
    /// Refusals, oldest first.
    pub refusals: Vec<Called>,
}

impl MemRegistry {
    /// The shipped points.
    #[must_use]
    pub fn shipped() -> Self {
        Self {
            registry: Registry::shipped(),
            refusals: Vec::new(),
        }
    }
}

impl ExitRegistry for MemRegistry {
    fn programs(&mut self, point: &str, format: &str) -> RegResult<Vec<ExitProgram>> {
        Ok(self.registry.programs(point, format))
    }

    fn refused(&mut self, call: &Called) {
        self.refusals.push(call.clone());
    }
}
