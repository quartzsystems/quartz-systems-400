// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `C6`: operating system initialization.

use crate::model::{Phase, Src};

/// OS phase started.
pub const OS_STARTED: Src = Src::new(
    "C6001000",
    "OS phase started",
    Phase::Os,
    "OS phase started.",
)
.cause("The LIC part of the IPL ended; the operating system's part begins.")
.recovery("None.");
/// Object store opened.
pub const STORE_OPENED: Src = Src::new(
    "C6001005",
    "Object store opened",
    Phase::Os,
    "Object store opened.",
)
.cause("The objects in ASP 1 were opened: QSYS was found, or made on a new store.")
.recovery("None.");
/// First boot: QSYS and the system libraries created.
pub const ASP_INITIALIZED: Src = Src::new(
    "C6001007",
    "ASP 1 initialized",
    Phase::Os,
    "First boot: QSYS and the system libraries created.",
)
.cause(
    "ASP 1 was blank and formatted at this IPL, so QSYS and the system libraries were made in it.",
)
.recovery("None.");
/// The superblock asks for a reclaim.
pub const RCLSTG_REQUIRED: Src = Src::new(
    "C6001008",
    "Reclaim storage required",
    Phase::Os,
    "The superblock asks for a reclaim.",
)
.cause("The store's superblock has its reclaim flag set.")
.recovery("Run RCLSTG; a full reclaim clears the flag.");
/// System values loaded.
pub const SYSVALS_LOADED: Src = Src::new(
    "C6001010",
    "System values loaded",
    Phase::Os,
    "System values loaded.",
)
.cause(
    "The system values were read from the store, or from the LIC record when the store has none; \
     the serial log names any that could not be read.",
)
.recovery("None.");
/// The IFS volume created in ASP 1 and formatted.
pub const IFS_CREATED: Src = Src::new(
    "C6001011",
    "IFS created in ASP 1",
    Phase::Os,
    "The IFS volume created in ASP 1 and formatted.",
)
.cause("ASP 1 had no formatted IFS volume: its first IPL, or a format cut short by a crash.")
.recovery("None.");
/// The IFS filesystem mounted.
pub const IFS_MOUNTED: Src = Src::new("C6001012", "IFS mounted", Phase::Os, "The IFS mounted.")
.cause(
    "The storage service served and mounted the IFS volume in ASP 1, or the IPL mounted an older \
     system's IFS partition.",
)
.recovery("None.");
/// The storage service process took over the store.
pub const STG_STARTED: Src = Src::new(
    "C6001015",
    "Storage management service started",
    Phase::Os,
    "The storage service process took over the store.",
)
.cause(
    "The IPL handed ASP 1 to the storage service process; from here on every job reaches the store \
     through it.",
)
.recovery("None.");
/// Startup program skipped: `QSTRUPPGM(*NONE)`, no controlling
/// subsystem, or no store.
pub const STRUP_SKIPPED: Src = Src::new(
    "C6001020",
    "Startup program skipped",
    Phase::Os,
    "Startup program skipped.",
)
.cause(
    "QSTRUPPGM is *NONE, the controlling subsystem QCTLSBSD names would not start (the serial \
     log says why), or there is no store.",
)
.recovery(
    "None when QSTRUPPGM is *NONE. Otherwise correct QCTLSBSD or the subsystem description it \
     names and IPL again, or start the subsystem with STRSBS.",
);
/// IPL complete.
pub const IPL_COMPLETE: Src = Src::new("C6001090", "IPL complete", Phase::Os, "IPL complete.")
.cause(
    "Every step of the IPL ended; the IPL history records it as normal and the console becomes a \
     5250 display.",
)
.recovery("None.");
/// The system jobs are running.
pub const SYSJOBS_STARTED: Src = Src::new(
    "C6001030",
    "System jobs started",
    Phase::Os,
    "The system jobs are running.",
)
.cause("The system jobs were started; the serial log names any that would not start.")
.recovery("None.");
/// The controlling subsystem is active.
pub const CTLSBS_STARTED: Src = Src::new(
    "C6001040",
    "Controlling subsystem started",
    Phase::Os,
    "The controlling subsystem is active.",
)
.cause("The subsystem QCTLSBSD names was started, after QSYSWRK.")
.recovery("None.");
/// The startup program's job ended.
pub const STRUP_ENDED: Src = Src::new(
    "C6001050",
    "Startup program ended",
    Phase::Os,
    "The startup program's job ended.",
)
.cause(
    "The startup program's job, QSTRUPJD, ran to its end, or the IPL stopped waiting for it; the \
     serial log says which.",
)
.recovery("None. When it did not end, its job log shows where it was.");
