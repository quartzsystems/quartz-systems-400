// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System reference codes. One table, no scattered strings.
//!
//! Code families: `C1` LIC init, `C6` OS init, `A6` attention (non-fatal),
//! `B1` fatal, `D1` power down.

/// Where in the IPL a code belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// LIC initialization.
    Lic,
    /// Operating system initialization.
    Os,
    /// Attention, continues.
    Attention,
    /// Fatal, parks.
    Fatal,
    /// Power down.
    PowerDown,
}

/// One reference code.
#[derive(Debug, PartialEq, Eq)]
pub struct Src {
    /// Eight-character code.
    pub code: &'static str,
    /// Short Quartz-written text.
    pub text: &'static str,
    /// Phase.
    pub phase: Phase,
}

const fn src(code: &'static str, text: &'static str, phase: Phase) -> Src {
    Src { code, text, phase }
}

/// IPL started.
pub const IPL_STARTED: Src = src("C1001000", "IPL started", Phase::Lic);
/// LIC partition located.
pub const LIC_LOCATED: Src = src("C1001010", "LIC partition located", Phase::Lic);
/// Running from installation media: no LIC partition, in-memory record.
pub const MEDIA_IPL: Src = src("C1001015", "Running from installation media", Phase::Lic);
/// LIC record read (or initialized).
pub const LIC_READ: Src = src("C1001020", "LIC record read", Phase::Lic);
/// Hardware resources enumerated.
pub const HW_ENUMERATED: Src = src("C1001030", "Hardware resources enumerated", Phase::Lic);
/// Clock set from RTC.
pub const CLOCK_SET: Src = src("C1001040", "Clock set from RTC", Phase::Lic);
/// Console attached. Emitted once the console crate attaches a display.
#[allow(dead_code)]
pub const CONSOLE_ATTACHED: Src = src("C1001050", "Console attached", Phase::Lic);
/// Network links enumerated.
pub const NET_ENUMERATED: Src = src("C1001060", "Network links enumerated", Phase::Lic);
/// ASP 1 unit located.
pub const ASP_LOCATED: Src = src("C1001070", "ASP units located", Phase::Lic);
/// A mirrored half is being copied from its partner before the store
/// is used.
pub const MIRROR_SYNC: Src = src(
    "C1001075",
    "Mirrored protection synchronization",
    Phase::Lic,
);
/// WAL replayed (or a blank unit formatted).
pub const RECOVERY: Src = src("C1001080", "Storage management recovery", Phase::Lic);
/// Neither superblock usable, or the store would not open.
pub const RECOVERY_FAILED: Src = src("C1001085", "Storage management recovery failed", Phase::Lic);
/// Commit cycles a crash left open were rolled back from the journals.
pub const DB_RECOVERY: Src = src("C1001082", "Database recovery", Phase::Lic);
/// The store was created by another release.
pub const RELEASE_MISMATCH: Src = src("C1001087", "Store release mismatch", Phase::Lic);
/// Previous IPL was abnormal. Informational, continues.
pub const PREV_ABNORMAL: Src = src("C1002000", "Previous IPL was abnormal", Phase::Lic);
/// OS phase started.
pub const OS_STARTED: Src = src("C6001000", "OS phase started", Phase::Os);
/// Object store opened.
pub const STORE_OPENED: Src = src("C6001005", "Object store opened", Phase::Os);
/// First boot: QSYS and the system libraries created.
pub const ASP_INITIALIZED: Src = src("C6001007", "ASP 1 initialized", Phase::Os);
/// The superblock asks for a reclaim.
pub const RCLSTG_REQUIRED: Src = src("C6001008", "Reclaim storage required", Phase::Os);
/// System values loaded.
pub const SYSVALS_LOADED: Src = src("C6001010", "System values loaded", Phase::Os);
/// Startup program skipped. MIGRATE(phase5): run QSTRUPPGM.
pub const STRUP_SKIPPED: Src = src("C6001020", "Startup program skipped", Phase::Os);
/// IPL complete.
pub const IPL_COMPLETE: Src = src("C6001090", "IPL complete", Phase::Os);
/// The installer started writing the load source.
pub const INSTALL_STARTED: Src = src("C6004001", "Install started", Phase::Os);
/// Partition table and ESP written.
pub const INSTALL_PARTITIONED: Src = src("C6004010", "Load source partitioned", Phase::Os);
/// LIC partition initialized.
pub const INSTALL_LIC_PART: Src = src("C6004020", "LIC partition initialized", Phase::Os);
/// Slot A holds the Licensed Internal Code.
pub const INSTALL_SLOT_A: Src = src(
    "C6004030",
    "Licensed Internal Code copied to slot A",
    Phase::Os,
);
/// Slot B holds the Licensed Internal Code.
pub const INSTALL_SLOT_B: Src = src(
    "C6004040",
    "Licensed Internal Code copied to slot B",
    Phase::Os,
);
/// LIC record written with the install pending.
pub const INSTALL_LIC_RECORD: Src = src("C6004050", "LIC record written", Phase::Os);
/// The LIC install is done; re-IPL.
pub const INSTALL_LIC_COMPLETE: Src =
    src("C6004090", "Licensed Internal Code installed", Phase::Os);
/// The load source has the LIC and waits for the operating system.
pub const INSTALL_OS_PENDING: Src = src("C6004100", "Operating system install pending", Phase::Os);
/// ASP 1 formatted by the installer.
pub const INSTALL_ASP: Src = src("C6004110", "ASP 1 initialized by the installer", Phase::Os);
/// QSYS and the system libraries created by the installer.
pub const INSTALL_LIBRARIES: Src = src("C6004120", "System libraries created", Phase::Os);
/// The operating system install is done.
pub const INSTALL_OS_COMPLETE: Src = src("C6004190", "Operating system installed", Phase::Os);
/// Console not found; continuing on serial.
pub const CONSOLE_MISSING: Src = src(
    "A6005008",
    "Console not found, continuing on serial",
    Phase::Attention,
);
/// Half of a mirrored pair was not found; the pair runs on one half.
pub const MIRROR_MISSING: Src = src(
    "A6001255",
    "Mirrored unit missing, protection suspended",
    Phase::Attention,
);
/// Half of a mirrored pair failed I/O and was taken out of service.
pub const MIRROR_FAILED: Src = src(
    "A6001256",
    "Mirrored unit not operational, protection suspended",
    Phase::Attention,
);
/// Disks beyond the load source belong to no ASP yet.
pub const DISK_NOT_CONFIGURED: Src = src("A6001260", "Disk unit not configured", Phase::Attention);
/// slic panicked.
pub const PANIC: Src = src("B1001000", "slic panic", Phase::Fatal);
/// LIC partition not found.
pub const LIC_NOT_FOUND: Src = src("B1001010", "LIC partition not found", Phase::Fatal);
/// LIC record unreadable in both copies.
pub const LIC_UNREADABLE: Src = src(
    "B1001020",
    "LIC record unreadable, both copies",
    Phase::Fatal,
);
/// A disk unit the ASP needs is not there: no ASP 1 partition at all,
/// an unprotected unit, or both halves of a pair.
pub const ASP_NOT_FOUND: Src = src("B1001030", "Disk unit missing", Phase::Fatal);
/// The store could not be opened or initialized.
pub const ASP_UNUSABLE: Src = src("B1001040", "Object store unusable", Phase::Fatal);
/// The store belongs to another release.
pub const ASP_WRONG_RELEASE: Src = src(
    "B1001050",
    "Object store created by another release",
    Phase::Fatal,
);
/// Power down started.
pub const PWRDWN_STARTED: Src = src("D1001000", "Power down started", Phase::PowerDown);
/// Store checkpointed and closed.
pub const STORE_CLOSED: Src = src("D1001010", "Object store closed", Phase::PowerDown);
/// Power down complete.
pub const PWRDWN_COMPLETE: Src = src("D1001090", "Power down complete", Phase::PowerDown);

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
    STRUP_SKIPPED,
    IPL_COMPLETE,
    INSTALL_STARTED,
    INSTALL_PARTITIONED,
    INSTALL_LIC_PART,
    INSTALL_SLOT_A,
    INSTALL_SLOT_B,
    INSTALL_LIC_RECORD,
    INSTALL_LIC_COMPLETE,
    INSTALL_OS_PENDING,
    INSTALL_ASP,
    INSTALL_LIBRARIES,
    INSTALL_OS_COMPLETE,
    CONSOLE_MISSING,
    DISK_NOT_CONFIGURED,
    MIRROR_MISSING,
    MIRROR_FAILED,
    PANIC,
    LIC_NOT_FOUND,
    LIC_UNREADABLE,
    ASP_NOT_FOUND,
    ASP_UNUSABLE,
    ASP_WRONG_RELEASE,
    PWRDWN_STARTED,
    STORE_CLOSED,
    PWRDWN_COMPLETE,
];

/// Find a code in the table.
pub fn lookup(code: &str) -> Option<&'static Src> {
    TABLE.iter().find(|s| s.code == code)
}

impl Src {
    /// The serial line: `SRC <code>  <text>`.
    pub fn line(&self) -> String {
        format!("SRC {}  {}\n", self.code, self.text)
    }

    /// The code as fixed 8 ASCII bytes for the LIC history record.
    pub fn bytes(&self) -> [u8; 8] {
        let mut b = *b"        ";
        for (dst, src) in b.iter_mut().zip(self.code.bytes()) {
            *dst = src;
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_well_formed() {
        for (i, s) in TABLE.iter().enumerate() {
            assert_eq!(s.code.len(), 8, "{}", s.code);
            assert!(
                s.code.bytes().all(|b| b.is_ascii_alphanumeric()),
                "{}",
                s.code
            );
            assert!(!s.text.is_empty());
            assert!(
                TABLE[..i].iter().all(|o| o.code != s.code),
                "duplicate {}",
                s.code
            );
        }
        assert_eq!(lookup("C6001090"), Some(&IPL_COMPLETE));
    }
}
