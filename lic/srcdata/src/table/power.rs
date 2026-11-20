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
)
.cause(
    "PWRDWNSYS, from a job, the service console or a hypervisor, or Perform an IPL in DST or the \
     IPL menus.",
)
.recovery(
    "None. If no later code follows within the delay asked for, power the system off; the next IPL \
     recovers the store from its log.",
);
/// Every subsystem ended for power down.
pub const SBS_ENDED: Src = Src::new(
    "D1001005",
    "Subsystems ended",
    Phase::PowerDown,
    "Every subsystem ended for power down.",
)
.cause(
    "Every subsystem ended, controlled within the delay PWRDWNSYS gave or at once; the power down \
     waits at most 15 seconds past the delay.",
)
.recovery("None.");
/// The system jobs ended for power down.
pub const SYSJOBS_ENDED: Src = Src::new(
    "D1001007",
    "System jobs ended",
    Phase::PowerDown,
    "The system jobs ended for power down.",
)
.cause("The system jobs the IPL started were ended; one still running after 3 seconds was killed.")
.recovery("None.");
/// Store checkpointed and closed.
pub const STORE_CLOSED: Src = Src::new(
    "D1001010",
    "Object store closed",
    Phase::PowerDown,
    "Store checkpointed and closed.",
)
.cause("The storage service checkpointed and closed ASP 1 and exited.")
.recovery(
    "None. When the close fails the serial log says why, and the next IPL recovers the store from \
     its log.",
);
/// The IFS filesystem unmounted.
pub const IFS_UNMOUNTED: Src = Src::new(
    "D1001012",
    "IFS unmounted",
    Phase::PowerDown,
    "The IFS unmounted.",
)
.cause(
    "The IFS was unmounted once its data was written; one still in use is detached instead, \
     without this code.",
)
.recovery("None.");
/// Power down complete.
pub const PWRDWN_COMPLETE: Src = Src::new(
    "D1001090",
    "Power down complete",
    Phase::PowerDown,
    "Power down complete.",
)
.cause("Every step of the power down ended; the system powers off, or IPLs again for a restart.")
.recovery("None.");
