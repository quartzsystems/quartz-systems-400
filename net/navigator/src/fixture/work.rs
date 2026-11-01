// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample work management: the prototype's jobs, queues, subsystems, spooled files, writers and schedule.

use crate::env::work::Work;

/// The sample system's work management.
#[derive(Default)]
pub struct FixtureWork {}

impl Work for FixtureWork {}
