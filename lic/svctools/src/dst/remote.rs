// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with Remote Service Support` (DST option 10). IBM's connects
//! the service tools to IBM over an electronic customer support line;
//! this system has none, so the status is always inactive and the
//! options that need the line say so.

use crate::dst::answers;
use crate::dst::common::{Notice, Submenu, says};
use crate::panel::Flow;

/// The menu.
#[must_use]
pub fn work_with_remote_service() -> Submenu {
    Submenu::new(
        "dst-remote",
        "Work with Remote Service Support",
        vec![
            (
                1,
                "Display remote service support status",
                Some(Box::new(|_env| Flow::Push(Box::new(status())))),
            ),
            (
                2,
                "Activate remote service support",
                says(answers::NO_REMOTE_LINE),
            ),
            (
                3,
                "Deactivate remote service support",
                says(answers::REMOTE_NOT_ACTIVE),
            ),
            (
                4,
                "Change remote service support attributes",
                says(answers::NO_REMOTE_LINE),
            ),
        ],
    )
}

fn status() -> Notice {
    Notice::new(
        "dst-remote-status",
        "Display Remote Service Support Status",
        vec![
            "Remote service support . . . . :  Not active".into(),
            "Line . . . . . . . . . . . . . :  None".into(),
            String::new(),
            "Problems are reported from the problem log (WRKPRB, SNDSRVRQS) and".into(),
            "fixes arrive through the update service (SNDPTFORD).".into(),
        ],
    )
}
