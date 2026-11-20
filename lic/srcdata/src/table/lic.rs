// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `C1`: LIC initialization.

use crate::model::{Phase, Src};

/// IPL started.
pub const IPL_STARTED: Src = Src::new("C1001000", "IPL started", Phase::Lic, "IPL started.");
/// LIC partition located.
pub const LIC_LOCATED: Src = Src::new(
    "C1001010",
    "LIC partition located",
    Phase::Lic,
    "LIC partition located.",
);
/// Running from installation media: no LIC partition, in-memory record.
pub const MEDIA_IPL: Src = Src::new(
    "C1001015",
    "Running from installation media",
    Phase::Lic,
    "Running from installation media: no LIC partition, a LIC record in memory.",
)
.cause("A D IPL from installation media found no LIC partition, so it runs without one.");
/// LIC record read (or initialized).
pub const LIC_READ: Src = Src::new(
    "C1001020",
    "LIC record read",
    Phase::Lic,
    "LIC record read (or initialized).",
);
/// Hardware resources enumerated.
pub const HW_ENUMERATED: Src = Src::new(
    "C1001030",
    "Hardware resources enumerated",
    Phase::Lic,
    "Hardware resources enumerated.",
);
/// Clock set from RTC.
pub const CLOCK_SET: Src = Src::new(
    "C1001040",
    "Clock set from RTC",
    Phase::Lic,
    "Clock set from the real-time clock.",
);
/// Console attached.
pub const CONSOLE_ATTACHED: Src = Src::new(
    "C1001050",
    "Console attached",
    Phase::Lic,
    "Console attached: a display is in use for the console.",
);
/// Network links enumerated.
pub const NET_ENUMERATED: Src = Src::new(
    "C1001060",
    "Network links enumerated",
    Phase::Lic,
    "Network links enumerated.",
);
/// ASP 1 unit located.
pub const ASP_LOCATED: Src = Src::new(
    "C1001070",
    "ASP units located",
    Phase::Lic,
    "ASP 1 units located.",
);
/// A mirrored half is being copied from its partner before the store
/// is used.
pub const MIRROR_SYNC: Src = Src::new(
    "C1001075",
    "Mirrored protection synchronization",
    Phase::Lic,
    "A mirrored half is being copied from its partner before the store is used.",
)
.cause("Mirrored protection was started on a pair; its new half is copied during the IPL.");
/// WAL replayed (or a blank unit formatted).
pub const RECOVERY: Src = Src::new(
    "C1001080",
    "Storage management recovery",
    Phase::Lic,
    "Write-ahead log replayed (or a blank unit formatted).",
)
.cause("Every IPL replays the store's log, the count on serial; a blank unit is formatted here.");
/// Neither superblock usable, or the store would not open.
pub const RECOVERY_FAILED: Src = Src::new(
    "C1001085",
    "Storage management recovery failed",
    Phase::Lic,
    "Neither superblock usable, or the store would not open.",
)
.cause(
    "ASP 1 could not be formatted, opened, synchronized or recovered, or database recovery \
     failed; the serial log names the error. B1001040 follows when the store cannot be used.",
);
/// Commit cycles a crash left open were rolled back from the journals.
pub const DB_RECOVERY: Src = Src::new(
    "C1001082",
    "Database recovery",
    Phase::Lic,
    "Commit cycles a crash left open were rolled back from the journals.",
)
.cause("A crash left commit cycles open; the count rolled back is on serial.");
/// The store was created by another release.
pub const RELEASE_MISMATCH: Src = Src::new(
    "C1001087",
    "Store release mismatch",
    Phase::Lic,
    "The store was created by another release.",
)
.cause("ASP 1 was created by another release; both releases are on serial. B1001050 follows.");
/// Previous IPL was abnormal. Informational, continues.
pub const PREV_ABNORMAL: Src = Src::new(
    "C1002000",
    "Previous IPL was abnormal",
    Phase::Lic,
    "Previous IPL was abnormal. Informational, the IPL continues.",
)
.cause("The previous IPL never completed (power loss, a killed process); QABNORMSW is 1.");
