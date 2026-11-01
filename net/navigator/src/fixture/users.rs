// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample user and group profiles and authorization lists.

use crate::env::users::Users;

/// The sample system's users and groups.
#[derive(Default)]
pub struct FixtureUsers {}

impl Users for FixtureUsers {}
