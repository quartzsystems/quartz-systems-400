// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample TCP/IP configuration and servers.

use crate::env::net::Net;

/// The sample system's network.
#[derive(Default)]
pub struct FixtureNet {}

impl Net for FixtureNet {}
