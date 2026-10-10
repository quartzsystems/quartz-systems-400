// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System reference codes (SRC) used in Phase 0.

/// IPL started.
pub const IPL_STARTED: &str = "C1001000";
/// Heartbeat, every five seconds.
pub const HEARTBEAT: &str = "C1001005";
/// Panic or fatal error; the system parks.
pub const FATAL: &str = "B1001000";
/// Printed after the IPL SRC.
pub const BANNER: &str = "QS/400 IPL - phase 0";

/// An SRC on its own line, IBM style.
pub fn line(code: &str) -> String {
    format!("SRC {code}\n")
}
