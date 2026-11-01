// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample database: schema HLINV and its tables.

use crate::env::db::Db;

/// The sample system's database.
#[derive(Default)]
pub struct FixtureDb {}

impl Db for FixtureDb {}
