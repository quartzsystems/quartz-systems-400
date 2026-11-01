// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample system values, disks, hardware resources and time.

use crate::env::config::Config;

/// The sample system's system configuration.
#[derive(Default)]
pub struct FixtureConfig {}

impl Config for FixtureConfig {}
