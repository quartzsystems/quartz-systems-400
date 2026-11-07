// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's download: the area agent adds its data here.

use crate::env::download::Download;

/// The sample download.
#[derive(Debug, Default)]
pub struct FixtureDownload;

impl Download for FixtureDownload {}
