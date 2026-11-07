// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's commands: the area agent adds its data here.

use crate::env::cmd::Cmd;

/// The sample commands.
#[derive(Debug, Default)]
pub struct FixtureCmd;

impl Cmd for FixtureCmd {}
