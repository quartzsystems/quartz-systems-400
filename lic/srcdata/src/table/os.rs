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
);
/// Object store opened.
pub const STORE_OPENED: Src = Src::new(
    "C6001005",
    "Object store opened",
    Phase::Os,
    "Object store opened.",
);
/// First boot: QSYS and the system libraries created.
pub const ASP_INITIALIZED: Src = Src::new(
    "C6001007",
    "ASP 1 initialized",
    Phase::Os,
    "First boot: QSYS and the system libraries created.",
);
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
);
/// The IFS volume created in ASP 1 and formatted.
pub const IFS_CREATED: Src = Src::new(
    "C6001011",
    "IFS created in ASP 1",
    Phase::Os,
    "The IFS volume created in ASP 1 and formatted.",
)
.cause("ASP 1 had no formatted IFS volume: its first IPL, or a format cut short by a crash.");
/// The IFS filesystem mounted.
pub const IFS_MOUNTED: Src = Src::new("C6001012", "IFS mounted", Phase::Os, "The IFS mounted.");
/// The storage service process took over the store.
pub const STG_STARTED: Src = Src::new(
    "C6001015",
    "Storage management service started",
    Phase::Os,
    "The storage service process took over the store.",
);
/// Startup program skipped: `QSTRUPPGM(*NONE)`, or no store.
pub const STRUP_SKIPPED: Src = Src::new(
    "C6001020",
    "Startup program skipped",
    Phase::Os,
    "Startup program skipped.",
)
.cause("QSTRUPPGM is *NONE, or there is no store.");
/// IPL complete.
pub const IPL_COMPLETE: Src = Src::new("C6001090", "IPL complete", Phase::Os, "IPL complete.");
/// The system jobs are running.
pub const SYSJOBS_STARTED: Src = Src::new(
    "C6001030",
    "System jobs started",
    Phase::Os,
    "The system jobs are running.",
);
/// The controlling subsystem is active.
pub const CTLSBS_STARTED: Src = Src::new(
    "C6001040",
    "Controlling subsystem started",
    Phase::Os,
    "The controlling subsystem is active.",
);
/// The startup program's job ended.
pub const STRUP_ENDED: Src = Src::new(
    "C6001050",
    "Startup program ended",
    Phase::Os,
    "The startup program's job ended.",
);
