// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `C6004`: the install and PTFs at IPL.

use crate::model::{Phase, Src};

/// The installer started writing the load source.
pub const INSTALL_STARTED: Src = Src::new(
    "C6004001",
    "Install started",
    Phase::Os,
    "The installer started writing the load source.",
);
/// Partition table and ESP written.
pub const INSTALL_PARTITIONED: Src = Src::new(
    "C6004010",
    "Load source partitioned",
    Phase::Os,
    "Partition table and EFI system partition written.",
);
/// LIC partition initialized.
pub const INSTALL_LIC_PART: Src = Src::new(
    "C6004020",
    "LIC partition initialized",
    Phase::Os,
    "LIC partition initialized.",
);
/// Slot A holds the Licensed Internal Code.
pub const INSTALL_SLOT_A: Src = Src::new(
    "C6004030",
    "Licensed Internal Code copied to slot A",
    Phase::Os,
    "Slot A holds the Licensed Internal Code.",
);
/// Slot B holds the Licensed Internal Code.
pub const INSTALL_SLOT_B: Src = Src::new(
    "C6004040",
    "Licensed Internal Code copied to slot B",
    Phase::Os,
    "Slot B holds the Licensed Internal Code.",
);
/// LIC record written with the install pending.
pub const INSTALL_LIC_RECORD: Src = Src::new(
    "C6004050",
    "LIC record written",
    Phase::Os,
    "LIC record written with the install pending.",
);
/// The LIC install is done; re-IPL.
pub const INSTALL_LIC_COMPLETE: Src = Src::new(
    "C6004090",
    "Licensed Internal Code installed",
    Phase::Os,
    "The LIC install is done; the system IPLs again.",
);
/// The load source has the LIC and waits for the operating system.
pub const INSTALL_OS_PENDING: Src = Src::new(
    "C6004100",
    "Operating system install pending",
    Phase::Os,
    "The load source has the LIC and waits for the operating system.",
);
/// ASP 1 formatted by the installer.
pub const INSTALL_ASP: Src = Src::new(
    "C6004110",
    "ASP 1 initialized by the installer",
    Phase::Os,
    "ASP 1 formatted by the installer.",
);
/// QSYS and the system libraries created by the installer.
pub const INSTALL_LIBRARIES: Src = Src::new(
    "C6004120",
    "System libraries created",
    Phase::Os,
    "QSYS and the system libraries created by the installer.",
);
/// A LIC install over an installed system wrote the slot it did not boot.
pub const INSTALL_LIC_SLOT: Src = Src::new(
    "C6004060",
    "Licensed Internal Code written to the inactive slot",
    Phase::Os,
    "A LIC install over an installed system wrote the slot it did not boot.",
)
.cause("Installing the LIC over an installed system; that slot becomes the default.");
/// Option 4: ASP 1 is being copied onto the new load source.
pub const INSTALL_RESTORE_UNITS: Src = Src::new(
    "C6004070",
    "Load source restored from its mirrored partner",
    Phase::Os,
    "ASP 1 is being copied onto the new load source.",
)
.cause(
    "A new load source took the place of a failed one whose ASP 1 was mirrored; the copy \
     from the partner runs in the background.",
);
/// The operating system's load is restored into QSYS.
pub const INSTALL_OS_LOAD: Src = Src::new(
    "C6004130",
    "Operating system load restored",
    Phase::Os,
    "The operating system's load is restored into QSYS.",
);
/// Programs translated again after an upgrade.
pub const INSTALL_RETRANSLATED: Src = Src::new(
    "C6004140",
    "Programs translated again",
    Phase::Os,
    "Programs translated again after an upgrade.",
);
/// The recovery's security data and configuration restored.
pub const INSTALL_RECOVERY: Src = Src::new(
    "C6004150",
    "System save data restored",
    Phase::Os,
    "The recovery's security data and configuration restored.",
);
/// Licensed programs installed after the operating system.
pub const INSTALL_LICPGMS: Src = Src::new(
    "C6004160",
    "Licensed programs installed",
    Phase::Os,
    "Licensed programs installed after the operating system.",
);
/// The operating system install is done.
pub const INSTALL_OS_COMPLETE: Src = Src::new(
    "C6004190",
    "Operating system installed",
    Phase::Os,
    "The operating system install is done.",
);
/// Delayed PTFs being applied.
pub const PTF_APPLY: Src = Src::new(
    "C6004501",
    "Applying delayed PTFs",
    Phase::Os,
    "Delayed PTFs being applied.",
);
/// PTFs marked for the next IPL being removed.
pub const PTF_REMOVE: Src = Src::new(
    "C6004502",
    "Removing PTFs",
    Phase::Os,
    "PTFs marked for the next IPL being removed.",
);
/// A LIC PTF took effect from the other slot.
pub const PTF_LIC: Src = Src::new(
    "C6004503",
    "LIC PTF in effect from the other slot",
    Phase::Os,
    "A LIC PTF took effect from the other slot.",
);
/// A delayed PTF failed to apply.
pub const PTF_FAILED: Src = Src::new(
    "C6004504",
    "Delayed PTF failed to apply",
    Phase::Attention,
    "A delayed PTF failed to apply.",
)
.cause("A PTF marked to apply at this IPL could not be applied; it is marked Damaged.");
