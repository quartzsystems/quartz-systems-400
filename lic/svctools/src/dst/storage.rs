// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with Save Storage and Restore Storage` (DST option 9). IBM's
//! save storage copies every disk page to tape from DST; QS/400 saves
//! the system with `SAVSYS` and recovers from the install media
//! (docs/RECOVERY.md), so both options say so.

use crate::dst::answers;
use crate::dst::common::{Submenu, says};

/// The menu.
#[must_use]
pub fn work_with_save_storage() -> Submenu {
    Submenu::new(
        "dst-save-storage",
        "Work with Save Storage and Restore Storage",
        vec![
            (1, "Save storage", says(answers::SAVE_STORAGE)),
            (2, "Restore storage", says(answers::SAVE_STORAGE)),
        ],
    )
}
