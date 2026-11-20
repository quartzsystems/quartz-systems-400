// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `D1`: power down.

use crate::model::{Phase, Src};

/// Power down started.
pub const PWRDWN_STARTED: Src = Src::new(
    "D1001000",
    "Power down started",
    Phase::PowerDown,
    "Power down started.",
);
/// Every subsystem ended for power down.
pub const SBS_ENDED: Src = Src::new(
    "D1001005",
    "Subsystems ended",
    Phase::PowerDown,
    "Every subsystem ended for power down.",
);
/// The system jobs ended for power down.
pub const SYSJOBS_ENDED: Src = Src::new(
    "D1001007",
    "System jobs ended",
    Phase::PowerDown,
    "The system jobs ended for power down.",
);
/// Store checkpointed and closed.
pub const STORE_CLOSED: Src = Src::new(
    "D1001010",
    "Object store closed",
    Phase::PowerDown,
    "Store checkpointed and closed.",
);
/// The IFS filesystem unmounted.
pub const IFS_UNMOUNTED: Src = Src::new(
    "D1001012",
    "IFS unmounted",
    Phase::PowerDown,
    "The IFS unmounted.",
);
/// Power down complete.
pub const PWRDWN_COMPLETE: Src = Src::new(
    "D1001090",
    "Power down complete",
    Phase::PowerDown,
    "Power down complete.",
);
