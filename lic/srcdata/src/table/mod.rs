// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every code, one file per family, and the table in posting order.

mod attention;
mod fatal;
mod install;
mod lic;
mod os;
mod power;

pub use attention::*;
pub use fatal::*;
pub use install::*;
pub use lic::*;
pub use os::*;
pub use power::*;

use crate::model::Src;

/// Every code, in table order.
pub const TABLE: &[Src] = &[
    IPL_STARTED,
    LIC_LOCATED,
    MEDIA_IPL,
    LIC_READ,
    HW_ENUMERATED,
    CLOCK_SET,
    CONSOLE_ATTACHED,
    NET_ENUMERATED,
    ASP_LOCATED,
    MIRROR_SYNC,
    RECOVERY,
    DB_RECOVERY,
    RECOVERY_FAILED,
    RELEASE_MISMATCH,
    PREV_ABNORMAL,
    OS_STARTED,
    STORE_OPENED,
    ASP_INITIALIZED,
    RCLSTG_REQUIRED,
    SYSVALS_LOADED,
    IFS_CREATED,
    IFS_MOUNTED,
    STG_STARTED,
    STRUP_SKIPPED,
    SYSJOBS_STARTED,
    CTLSBS_STARTED,
    STRUP_ENDED,
    IPL_COMPLETE,
    INSTALL_STARTED,
    INSTALL_PARTITIONED,
    INSTALL_LIC_PART,
    INSTALL_SLOT_A,
    INSTALL_SLOT_B,
    INSTALL_LIC_RECORD,
    INSTALL_LIC_SLOT,
    INSTALL_RESTORE_UNITS,
    INSTALL_REFUSED,
    INSTALL_LIC_COMPLETE,
    INSTALL_OS_PENDING,
    INSTALL_ASP,
    INSTALL_LIBRARIES,
    INSTALL_OS_LOAD,
    INSTALL_RETRANSLATED,
    INSTALL_RECOVERY,
    INSTALL_LICPGMS,
    PTF_APPLY,
    PTF_REMOVE,
    PTF_LIC,
    PTF_FAILED,
    INSTALL_OS_COMPLETE,
    CONSOLE_MISSING,
    DISK_NOT_CONFIGURED,
    MIRROR_MISSING,
    MIRROR_FAILED,
    PARITY_EXPOSED,
    IFS_MISSING,
    IFS_NOT_CREATED,
    IFS_MOUNT_FAILED,
    IFS_NOT_SERVED,
    DEVICE_ERROR,
    MEDIA_ERROR,
    PANIC,
    LIC_NOT_FOUND,
    LIC_UNREADABLE,
    ASP_NOT_FOUND,
    ASP_UNUSABLE,
    ASP_WRONG_RELEASE,
    STG_ENDED,
    PWRDWN_STARTED,
    SBS_ENDED,
    SYSJOBS_ENDED,
    STORE_CLOSED,
    IFS_UNMOUNTED,
    PWRDWN_COMPLETE,
];

/// Find a code in the table.
#[must_use]
pub fn lookup(code: &str) -> Option<&'static Src> {
    TABLE.iter().find(|s| s.code == code)
}
