// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `C1`: LIC initialization.

use crate::model::{Phase, Src};

/// IPL started.
pub const IPL_STARTED: Src = Src::new("C1001000", "IPL started", Phase::Lic, "IPL started.")
    .cause("The IPL program started; every IPL posts this first, from disk or from media.")
    .recovery("None.");
/// LIC partition located.
pub const LIC_LOCATED: Src = Src::new(
    "C1001010",
    "LIC partition located",
    Phase::Lic,
    "LIC partition located.",
)
.cause("The LIC partition was found on a disk by its partition type.")
.recovery("None.");
/// Running from installation media: no LIC partition, in-memory record.
pub const MEDIA_IPL: Src = Src::new(
    "C1001015",
    "Running from installation media",
    Phase::Lic,
    "Running from installation media: no LIC partition, a LIC record in memory.",
)
.cause("A D IPL from installation media found no LIC partition, so it runs without one.")
.recovery("None: this is how an install from the media begins.");
/// LIC record read (or initialized).
pub const LIC_READ: Src = Src::new(
    "C1001020",
    "LIC record read",
    Phase::Lic,
    "LIC record read (or initialized).",
)
.cause(
    "The LIC record was read from its partition; at a system's first IPL, with both copies blank, \
     it is written new.",
)
.recovery("None.");
/// Hardware resources enumerated.
pub const HW_ENUMERATED: Src = Src::new(
    "C1001030",
    "Hardware resources enumerated",
    Phase::Lic,
    "Hardware resources enumerated.",
)
.cause(
    "The hardware resource service started and listed the system's devices; if it could not start, \
     the serial log says why.",
)
.recovery("None. WRKHDWRSC and the hardware service manager in DST show what was found.");
/// Clock set from RTC.
pub const CLOCK_SET: Src = Src::new(
    "C1001040",
    "Clock set from RTC",
    Phase::Lic,
    "Clock set from the real-time clock.",
)
.cause(
    "The system clock was set from the real-time clock. It is posted also when the clock could not \
     be read, which the serial log names.",
)
.recovery(
    "None. If the date or time is wrong, set the system values QDATE and QTIME, or keep the clock \
     with an NTP server.",
);
/// Console attached.
pub const CONSOLE_ATTACHED: Src = Src::new(
    "C1001050",
    "Console attached",
    Phase::Lic,
    "Console attached: a display is in use for the console.",
)
.cause(
    "The console opened where DST's console mode puts it: the local display, the serial port, or a \
     partition's console port.",
)
.recovery("None.");
/// Network links enumerated.
pub const NET_ENUMERATED: Src = Src::new(
    "C1001060",
    "Network links enumerated",
    Phase::Lic,
    "Network links enumerated.",
)
.cause(
    "The network links were listed and the loopback brought up; the IPL adds an address only when \
     its command line names one.",
)
.recovery("None. Interfaces are configured with CFGTCP and started with STRTCP.");
/// ASP 1 unit located.
pub const ASP_LOCATED: Src = Src::new(
    "C1001070",
    "ASP units located",
    Phase::Lic,
    "ASP 1 units located.",
)
.cause("At least one ASP 1 unit was found and opened; the serial log names each.")
.recovery("None.");
/// A mirrored half is being copied from its partner before the store
/// is used.
pub const MIRROR_SYNC: Src = Src::new(
    "C1001075",
    "Mirrored protection synchronization",
    Phase::Lic,
    "A mirrored half is being copied from its partner before the store is used.",
)
.cause("Mirrored protection was started on a pair; its new half is copied during the IPL.")
.recovery(
    "None; the console shows the percent copied. After a power loss the next IPL goes on from \
     where the copy stopped.",
);
/// WAL replayed (or a blank unit formatted).
pub const RECOVERY: Src = Src::new(
    "C1001080",
    "Storage management recovery",
    Phase::Lic,
    "Write-ahead log replayed (or a blank unit formatted).",
)
.cause("Every IPL replays the store's log, the count on serial; a blank unit is formatted here.")
.recovery("None; how long it takes depends on the work the log held.");
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
)
.recovery(
    "Read the error in the serial log. When B1001040 follows, IPL from installation media in \
     manual mode and check the units in DST, Work with disk units; a store that still will not \
     open is recovered from the last system save. When the IPL goes on, report the problem with \
     the serial log.",
);
/// Commit cycles a crash left open were rolled back from the journals.
pub const DB_RECOVERY: Src = Src::new(
    "C1001082",
    "Database recovery",
    Phase::Lic,
    "Commit cycles a crash left open were rolled back from the journals.",
)
.cause("A crash left commit cycles open; the count rolled back is on serial.")
.recovery(
    "None. The changes undone are in the journals (DSPJRN); work that ran under those cycles must \
     be run again.",
);
/// The store was created by another release.
pub const RELEASE_MISMATCH: Src = Src::new(
    "C1001087",
    "Store release mismatch",
    Phase::Lic,
    "The store was created by another release.",
)
.cause("ASP 1 was created by another release; both releases are on serial. B1001050 follows.")
.recovery(
    "IPL the release that created the store, or install this release over it from its installation \
     media to upgrade it.",
);
/// Previous IPL was abnormal. Informational, continues.
pub const PREV_ABNORMAL: Src = Src::new(
    "C1002000",
    "Previous IPL was abnormal",
    Phase::Lic,
    "Previous IPL was abnormal. Informational, the IPL continues.",
)
.cause("The previous IPL never completed (power loss, a killed process); QABNORMSW is 1.")
.recovery(
    "None; the IPL recovers the store and the journals. An attended IPL offers DST first. The \
     product activity log and WRKPRB may show why the system ended.",
);
