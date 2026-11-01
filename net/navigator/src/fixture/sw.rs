// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample licensed programs and PTFs (cumulative level QS400-CUM-26290).

use crate::env::sw::Software;

/// The sample system's software.
#[derive(Default)]
pub struct FixtureSoftware {}

impl Software for FixtureSoftware {}
