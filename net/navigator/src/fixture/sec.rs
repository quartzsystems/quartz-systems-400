// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample audit journal entries.

use crate::env::sec::Security;

/// The sample system's audit journal.
#[derive(Default)]
pub struct FixtureSecurity {}

impl Security for FixtureSecurity {}
