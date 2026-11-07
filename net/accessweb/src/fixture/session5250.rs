// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's saved 5250 sessions, as `web/5250-config.html`
//! lists them.

use crate::env::session5250::Session5250;

/// A saved session: name, system, screen size, CCSID, device name, TLS,
/// colour map, keypad, auto connect, auto sign-on, last used.
pub type Saved = [&'static str; 11];

/// The user's saved sessions.
pub const SESSIONS: &[Saved] = &[
    ["QSHQ400 operator", "QSHQ400", "24 x 80", "37", "QPADEV0003", "Yes", "QS/400 console", "Yes", "Yes", "Yes", "10/29/26 13:10"],
    ["QSHQ400 default", "QSHQ400", "24 x 80", "37", "*DFT", "Yes", "Black on white", "No", "No", "No", "10/22/26 09:41"],
    ["QSHQ400 wide", "QSHQ400", "27 x 132", "37", "QPADEV0004", "Yes", "QS/400 console", "No", "Yes", "Yes", "10/14/26 18:40"],
    ["QSBR1400 branch", "QSBR1400", "24 x 80", "37", "*DFT", "No", "QS/400 console", "Yes", "No", "No", "10/15/26 16:02"],

];

/// The sample 5250 sessions.
#[derive(Debug, Default)]
pub struct Fixture5250;

impl Session5250 for Fixture5250 {}
