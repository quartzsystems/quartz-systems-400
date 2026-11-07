// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's database: schema `HLINV` with table `ASSETS`, its
//! twelve sample rows as `web/table-view.html` shows them. The area
//! agent shapes them into its trait's answers.

use crate::env::db::Db;

/// The schemas the sample system has.
pub const SCHEMAS: &[&str] = &["HLINV", "QGPL", "WEBSTORE"];

/// `HLINV.ASSETS`'s columns.
pub const ASSETS_COLUMNS: &[&str] = &[
    "ASSET_TAG", "LOCATION", "MANUFACTURER", "MODEL", "SERIAL", "ASSET_TYPE", "STATUS", "ACQUIRED",
];

/// `HLINV.ASSETS`'s rows.
pub const ASSETS: &[[&str; 8]] = &[
    ["QS000101", "QS-HQ-R1", "QUANTA", "T4048-IX8", "QTFCEV2340021", "SWITCH", "ACTIVE", "2025-03-11"],
    ["QS000102", "QS-HQ-R1", "QUANTA", "T4048-IX8", "QTFCEV2340022", "SWITCH", "ACTIVE", "2025-03-11"],
    ["QS000110", "QS-HQ-R1", "EDGECORE", "AS5712-54X", "571254X2041B0019", "SWITCH", "ACTIVE", "2024-11-02"],
    ["QS000111", "QS-HQ-R1", "EDGECORE", "AS5712-54X", "571254X2041B0020", "SWITCH", "ACTIVE", "2024-11-02"],
    ["QS000201", "QS-HQ-R2", "DELL", "R740XD", "7XK2Q33", "SERVER", "ACTIVE", "2024-06-20"],
    ["QS000202", "QS-HQ-R2", "DELL", "R740XD", "7XK2Q34", "SERVER", "ACTIVE", "2024-06-20"],
    ["QS000203", "QS-HQ-R2", "DELL", "R740XD", "7XK2Q35", "SERVER", "ACTIVE", "2024-06-20"],
    ["QS000210", "QS-HQ-R2", "DELL", "QLE2562", "BFE1234567", "ADAPTER", "ACTIVE", "2026-09-20"],
    ["QS000302", "QS-HQ-R3", "WEIDMULLER", "UR20-FBC-EIP", "2024-11-00341", "IO-COUPLER", "ACTIVE", "2026-10-01"],
    ["QS000401", "QS-BR1-R1", "DELL", "S4048-ON", "8H3K2P2", "SWITCH", "ACTIVE", "2024-09-08"],
    ["QS000402", "QS-BR1-R1", "WATCHGUARD", "M370", "80BA0F2B1C33", "FIREWALL", "ACTIVE", "2024-09-08"],
    ["QS000403", "QS-BR1-R1", "ARISTA", "7050SX-64", "JPE14210033", "SWITCH", "SPARE", "2023-05-02"],

];

/// The sample database.
#[derive(Debug, Default)]
pub struct FixtureDb;

impl Db for FixtureDb {}
