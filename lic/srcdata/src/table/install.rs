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
)
.cause(
    "Install Licensed Internal Code began writing to the load source, from DST on the media or an \
     automatic install.",
)
.recovery(
    "None; the install goes on. If it stops, the status screen and the serial log name the error; \
     IPL from the media and start the install again.",
);
/// Partition table and ESP written.
pub const INSTALL_PARTITIONED: Src = Src::new(
    "C6004010",
    "Load source partitioned",
    Phase::Os,
    "Partition table and EFI system partition written.",
)
.cause(
    "Option 2 or 4 of Install Licensed Internal Code wrote the load source's partition table and \
     its EFI system partition.",
)
.recovery(
    "None; the install goes on. If it stops, the status screen and the serial log name the error; \
     IPL from the media and start the install again.",
);
/// LIC partition initialized.
pub const INSTALL_LIC_PART: Src = Src::new(
    "C6004020",
    "LIC partition initialized",
    Phase::Os,
    "LIC partition initialized.",
)
.cause("The new LIC partition was formatted with a fresh record.")
.recovery(
    "None; the install goes on. If it stops, the status screen and the serial log name the error; \
     IPL from the media and start the install again.",
);
/// Slot A holds the Licensed Internal Code.
pub const INSTALL_SLOT_A: Src = Src::new(
    "C6004030",
    "Licensed Internal Code copied to slot A",
    Phase::Os,
    "Slot A holds the Licensed Internal Code.",
)
.cause("The media's Licensed Internal Code is in slot A of the EFI system partition.")
.recovery(
    "None; the install goes on. If it stops, the status screen and the serial log name the error; \
     IPL from the media and start the install again.",
);
/// Slot B holds the Licensed Internal Code.
pub const INSTALL_SLOT_B: Src = Src::new(
    "C6004040",
    "Licensed Internal Code copied to slot B",
    Phase::Os,
    "Slot B holds the Licensed Internal Code.",
)
.cause("The media's Licensed Internal Code is in slot B of the EFI system partition.")
.recovery(
    "None; the install goes on. If it stops, the status screen and the serial log name the error; \
     IPL from the media and start the install again.",
);
/// LIC record written with the install pending.
pub const INSTALL_LIC_RECORD: Src = Src::new(
    "C6004050",
    "LIC record written",
    Phase::Os,
    "LIC record written with the install pending.",
)
.cause(
    "The LIC record was written with what the next IPL needs: the system values, the console mode, \
     the slot to boot and whether an operating system install follows.",
)
.recovery(
    "None; the install goes on. If it stops, the status screen and the serial log name the error; \
     IPL from the media and start the install again.",
);
/// The LIC install is done; re-IPL.
pub const INSTALL_LIC_COMPLETE: Src = Src::new(
    "C6004090",
    "Licensed Internal Code installed",
    Phase::Os,
    "The LIC install is done; the system IPLs again.",
)
.cause("Every step of the LIC install ended.")
.recovery(
    "None; the system IPLs from the load source next, where C6004100 follows when the operating \
     system is still to be installed.",
);
/// The load source has the LIC and waits for the operating system.
pub const INSTALL_OS_PENDING: Src = Src::new(
    "C6004100",
    "Operating system install pending",
    Phase::Os,
    "The load source has the LIC and waits for the operating system.",
)
.cause(
    "The LIC record says the operating system is still to be installed; every IPL posts this until \
     it is.",
)
.recovery(
    "Sign on to DST and take option 2, Install the operating system, with the media attached; an \
     automatic install goes on without prompts.",
);
/// ASP 1 formatted by the installer.
pub const INSTALL_ASP: Src = Src::new(
    "C6004110",
    "ASP 1 initialized by the installer",
    Phase::Os,
    "ASP 1 formatted by the installer.",
)
.cause(
    "The operating system install opened ASP 1: formatted on a new load source, or the system's \
     own for an install over it.",
)
.recovery(
    "None; the install goes on. If it stops, the status screen and the serial log name the error; \
     correct it and take DST option 2 again.",
);
/// QSYS and the system libraries created by the installer.
pub const INSTALL_LIBRARIES: Src = Src::new(
    "C6004120",
    "System libraries created",
    Phase::Os,
    "QSYS and the system libraries created by the installer.",
)
.cause(
    "QSYS and the system libraries were made on a new ASP 1, or the release rule allowed the \
     install over the system's.",
)
.recovery(
    "None; the install goes on. If it stops, the status screen and the serial log name the error; \
     correct it and take DST option 2 again.",
);
/// A LIC install over an installed system wrote the slot it did not boot.
pub const INSTALL_LIC_SLOT: Src = Src::new(
    "C6004060",
    "Licensed Internal Code written to the inactive slot",
    Phase::Os,
    "A LIC install over an installed system wrote the slot it did not boot.",
)
.cause("Installing the LIC over an installed system; that slot becomes the default.")
.recovery("None; the operating system install follows at the next IPL (C6004100).");
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
)
.recovery(
    "None. Work with disk units in DST shows the copy's progress; the pair is protected again when \
     it ends.",
);
/// The operating system's load is restored into QSYS.
pub const INSTALL_OS_LOAD: Src = Src::new(
    "C6004130",
    "Operating system load restored",
    Phase::Os,
    "The operating system's load is restored into QSYS.",
)
.cause(
    "The operating system's product load was restored from the media into QSYS; the system's work \
     management objects are kept across it.",
)
.recovery(
    "None; the install goes on. If it stops, the status screen and the serial log name the error; \
     correct it and take DST option 2 again.",
);
/// Programs translated again after an upgrade.
pub const INSTALL_RETRANSLATED: Src = Src::new(
    "C6004140",
    "Programs translated again",
    Phase::Os,
    "Programs translated again after an upgrade.",
)
.cause(
    "An install over an existing system: the programs that need it were translated again for this \
     release.",
)
.recovery("None; the serial log counts the programs translated.");
/// The recovery's security data and configuration restored.
pub const INSTALL_RECOVERY: Src = Src::new(
    "C6004150",
    "System save data restored",
    Phase::Os,
    "The recovery's security data and configuration restored.",
)
.cause(
    "The media is a system save: its user profiles and configuration were restored, then the \
     private authorities.",
)
.recovery(
    "None; the serial log has a line for each restore and what it could not restore. Restore any \
     saves taken after this one.",
);
/// Licensed programs installed after the operating system.
pub const INSTALL_LICPGMS: Src = Src::new(
    "C6004160",
    "Licensed programs installed",
    Phase::Os,
    "Licensed programs installed after the operating system.",
)
.cause("The licensed programs asked for were installed from the media.")
.recovery(
    "None. A product that did not install is named on serial; install it later with GO LICPGM or \
     RSTLICPGM.",
);
/// The operating system install is done.
pub const INSTALL_OS_COMPLETE: Src = Src::new(
    "C6004190",
    "Operating system installed",
    Phase::Os,
    "The operating system install is done.",
)
.cause("The IPL after the operating system install cleared the install from the LIC record.")
.recovery("None.");
/// Delayed PTFs being applied.
pub const PTF_APPLY: Src = Src::new(
    "C6004501",
    "Applying delayed PTFs",
    Phase::Os,
    "Delayed PTFs being applied.",
)
.cause("A PTF marked Apply at next IPL is applied now, before any job runs.")
.recovery("None; DSPPTF shows the result.");
/// PTFs marked for the next IPL being removed.
pub const PTF_REMOVE: Src = Src::new(
    "C6004502",
    "Removing PTFs",
    Phase::Os,
    "PTFs marked for the next IPL being removed.",
)
.cause("A PTF marked Remove at next IPL is removed now, before any job runs.")
.recovery("None; DSPPTF shows the result.");
/// A LIC PTF took effect from the other slot.
pub const PTF_LIC: Src = Src::new(
    "C6004503",
    "LIC PTF in effect from the other slot",
    Phase::Os,
    "A LIC PTF took effect from the other slot.",
)
.cause(
    "The IPL came up on the slot a LIC PTF was written to: the PTF is temporarily applied and that \
     slot boots from now on.",
)
.recovery(
    "None. RMVPTF and an IPL return to the code before it; APYPTF APY(*PERM) writes it into both \
     slots.",
);
/// A delayed PTF failed to apply.
pub const PTF_FAILED: Src = Src::new(
    "C6004504",
    "Delayed PTF failed to apply",
    Phase::Attention,
    "A delayed PTF failed to apply.",
)
.cause("A PTF marked to apply at this IPL could not be applied; it is marked Damaged.")
.recovery(
    "DSPPTF shows it Damaged. Load it again (LODPTF from its save file, or order it with \
     SNDPTFORD) and apply it with APYPTF.",
);
