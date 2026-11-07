// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's jobs: the area agent adds its data here.

use crate::env::jobs::Jobs;

/// The sample jobs.
#[derive(Debug, Default)]
pub struct FixtureJobs;

impl Jobs for FixtureJobs {}
