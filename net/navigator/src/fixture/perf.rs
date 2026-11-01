// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample performance data.

use crate::env::perf::Perf;

/// The sample system's performance.
#[derive(Default)]
pub struct FixturePerf {}

impl Perf for FixturePerf {}
