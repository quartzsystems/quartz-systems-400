// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `A6`: attention; the system goes on.

use crate::model::{Phase, Src};

/// The release rule refused the install.
pub const INSTALL_REFUSED: Src = Src::new(
    "A6004080",
    "Install refused by the release rule",
    Phase::Attention,
    "The release rule refused the install.",
)
.cause("The media's release is older than the system's, or newer than its previous release allows.")
.recovery("Install the previous release first; older media cannot be installed.");
/// Console not found; continuing on serial.
pub const CONSOLE_MISSING: Src = Src::new(
    "A6005008",
    "Console not found, continuing on serial",
    Phase::Attention,
    "Console not found; the IPL continues on serial.",
)
.cause(
    "No display, or a mode that is not drawn (only 16, 24 and 32 bits per pixel are); in a \
     partition, no console port.",
)
.recovery(
    "The serial console shows every screen meanwhile. Attach a display that is drawn, or choose \
     the serial console in DST (Select DST console mode), and IPL again.",
);
/// Half of a mirrored pair was not found; the pair runs on one half.
pub const MIRROR_MISSING: Src = Src::new(
    "A6001255",
    "Mirrored unit missing, protection suspended",
    Phase::Attention,
    "Half of a mirrored pair was not found; the pair runs on one half.",
)
.cause("A half of a mirrored pair was not found at IPL.")
.recovery(
    "A hot spare that fits takes its place. Otherwise return the disk, whose half then \
     resumes, or use Replace configured unit in DST.",
);
/// Half of a mirrored pair failed I/O and was taken out of service.
pub const MIRROR_FAILED: Src = Src::new(
    "A6001256",
    "Mirrored unit not operational, protection suspended",
    Phase::Attention,
    "Half of a mirrored pair failed I/O and was taken out of service.",
)
.cause("A half of a mirrored pair failed I/O while the system ran.")
.recovery(
    "A hot spare that fits takes its place. Otherwise use Replace configured unit in DST; \
     the copy runs in the background.",
);
/// A unit of a device parity set is missing or failed; the set runs
/// exposed, its pages computed from the others.
pub const PARITY_EXPOSED: Src = Src::new(
    "A6001257",
    "Device parity unit failed, parity set exposed",
    Phase::Attention,
    "A unit of a device parity set is missing or failed; the set runs exposed, its pages \
     computed from the others.",
)
.cause("A member of a device parity set was not found at IPL or failed I/O while running.")
.recovery(
    "A hot spare that fits is rebuilt in its place. Otherwise use Replace configured unit \
     in DST, or return the disk to have it rebuilt.",
);
/// Disks beyond the load source belong to no ASP yet.
pub const DISK_NOT_CONFIGURED: Src = Src::new(
    "A6001260",
    "Disk unit not configured",
    Phase::Attention,
    "Disks beyond the load source belong to no ASP yet.",
)
.cause("An install is pending with disks that are in no ASP, or that hold an earlier install.")
.recovery("Add the disks to ASP 1 in DST, or blank them with Delete disk unit data.");
/// No IFS partition on any disk; the IPL goes on without the IFS.
pub const IFS_MISSING: Src = Src::new(
    "A6001270",
    "IFS partition missing",
    Phase::Attention,
    "No IFS partition on any disk; the IPL goes on without the IFS.",
)
.cause("A store from before the IFS moved into ASP 1 lost its IFS partition.")
.recovery(
    "The IPL makes a new IFS in ASP 1 in its place (C6001011); restore its directories from the \
     last save with RST.",
);
/// ASP 1 has no room for the IFS volume; the IPL goes on without it.
pub const IFS_NOT_CREATED: Src = Src::new(
    "A6001271",
    "IFS not created: ASP 1 is too full",
    Phase::Attention,
    "ASP 1 has no room for the IFS volume; the IPL goes on without it.",
)
.cause("ASP 1 cannot hold the IFS volume's first 64 MiB.")
.recovery(
    "Free space in ASP 1, or add disk units to it in DST, then IPL again; the IPL makes the IFS \
     once it fits.",
);
/// The IFS would not mount; the IPL goes on without it.
pub const IFS_MOUNT_FAILED: Src = Src::new(
    "A6001275",
    "IFS mount failed",
    Phase::Attention,
    "The IFS would not mount; the IPL goes on without it.",
)
.cause("The kernel refused to mount the IFS volume; the serial log names the error.")
.recovery(
    "Note the error and IPL again. If it fails again, report the problem with the serial log; \
     until then the IFS, QSH and the file servers are unavailable.",
);
/// The IFS volume's block device could not be served.
pub const IFS_NOT_SERVED: Src = Src::new(
    "A6001276",
    "IFS device could not be served",
    Phase::Attention,
    "The IFS volume's block device could not be served; the IPL goes on without the IFS.",
)
.cause("The IFS volume's block device would not start; the serial log names the error.")
.recovery(
    "Note the error and IPL again. If it fails again, report the problem with the serial log; \
     until then the IFS, QSH and the file servers are unavailable.",
);
/// A tape or optical device failed; its resource is in the log entry.
pub const DEVICE_ERROR: Src = Src::new(
    "A6006300",
    "Media device error",
    Phase::Attention,
    "A tape or optical device failed; its resource is in the log entry.",
)
.cause("A drive or media changer failed; the operator is told and a problem is logged.")
.recovery(
    "Read the message in QSYSOPR and the problem in WRKPRB. Check the drive, its cabling and \
     cleaning, vary it off and on with VRYCFG, then run the operation again.",
);
/// A volume could not be read or written.
pub const MEDIA_ERROR: Src = Src::new(
    "A6006310",
    "Media error on a volume",
    Phase::Attention,
    "A volume could not be read or written.",
)
.cause("A volume's labels, frames or image are damaged; a problem is logged.")
.recovery(
    "Read the message in QSYSOPR and the problem in WRKPRB. Run the operation again on another \
     volume, or clean the drive; saves on a damaged volume may not restore.",
);
