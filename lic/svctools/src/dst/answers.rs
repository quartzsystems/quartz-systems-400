// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What an option says on the message line when its function does not
//! apply to this system, or is not in this release. IBM's service tools
//! answer such an option there too, without a message ID (LIC panels
//! have none): the hardware or the state the function needs is not
//! present. The words are Quartz's; docs/DST.md pairs each option with
//! its case.

/// No disk unit sits behind an IOP or IOA: nothing to recover or trace
/// at that level (problem recovery, disk unit tracing).
pub const NO_IOA: &str = "Disk units here have no IOP or IOA to recover or trace";

/// Checksum protection: gone from IBM i before V7R1, never here.
pub const NO_CHECKSUM: &str = "Checksum protection is not supported; use mirrored or device parity";

/// Compression needs a controller that compresses.
pub const NO_COMPRESSION: &str = "No disk unit on this system is capable of compression";

/// The system has ASP 1 only.
pub const ONE_ASP: &str = "This system has only ASP 1; units cannot move between ASPs";

/// Remote load source mirroring enables a pair across buses; there are
/// none, and the pairing already prefers another controller.
pub const NO_REMOTE_BUS: &str = "No remote bus to enable; load source pairs prefer another controller";

/// A missing unit is assigned by replacing it.
pub const ASSIGN_MISSING: &str = "Replace a missing mirrored or parity unit with option 3";

/// Every unit carries the configuration; the IPL reads it.
pub const RECOVER_CONFIG: &str = "The configuration is on every unit and is read at each IPL";

/// No remote service line.
pub const NO_REMOTE_LINE: &str = "Remote service needs an ECS line; this system has none";

/// Remote service is never active.
pub const REMOTE_NOT_ACTIVE: &str = "Remote service support is not active";

/// One partition, the whole machine.
pub const NOT_PARTITIONED: &str = "This system is not partitioned; it runs as one partition";

/// No capacity on demand.
pub const NO_COD: &str = "This system has no capacity on demand; every processor is active";

/// SST's diskette data recovery.
pub const NO_DISKETTE: &str = "This system has no diskette unit";

/// DST's option 14.
pub const NOT_RESTRICTED: &str = "The system is not in batch restricted state";

/// Device concurrent maintenance: disks come and go while the system
/// runs and the hardware resources are rescanned.
pub const CONCURRENT: &str = "Devices are added and removed with the system up; no action is needed";

/// `BLOCKED(rel)`: save and restore storage (every disk page to tape
/// from DST) are not built; SAVSYS and the install media recover the
/// system (docs/RECOVERY.md).
pub const SAVE_STORAGE: &str = "Save and restore storage are not in this release; use SAVSYS";

/// `BLOCKED(rel)`: saving, restoring or copying one unprotected unit's
/// data from DST needs a page-level copy the store does not have.
pub const UNIT_DATA: &str = "Saving or copying one unit's data is not in this release";

/// `BLOCKED(rel)`: balancing data over new units needs the page moves
/// `STRASPBAL` will bring; option 2 adds the units.
pub const BALANCE: &str = "Balancing data is not in this release; add the units with option 2";

/// `BLOCKED(rel)`: the IPL menu's option 5; SAVSYS saves the Licensed
/// Internal Code with the system up.
pub const SAVE_LIC: &str = "Save the Licensed Internal Code with SAVSYS; not from this menu";

/// The installer needs the installation media booted.
pub const NEEDS_MEDIA: &str = "Installing needs an IPL from the installation media (type D)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_fit_the_message_line() {
        for t in [
            NO_IOA,
            NO_CHECKSUM,
            NO_COMPRESSION,
            ONE_ASP,
            NO_REMOTE_BUS,
            ASSIGN_MISSING,
            RECOVER_CONFIG,
            NO_REMOTE_LINE,
            REMOTE_NOT_ACTIVE,
            NOT_PARTITIONED,
            NO_COD,
            NO_DISKETTE,
            NOT_RESTRICTED,
            CONCURRENT,
            SAVE_STORAGE,
            UNIT_DATA,
            BALANCE,
            SAVE_LIC,
            NEEDS_MEDIA,
        ] {
            assert!(t.len() <= 78, "{t}");
        }
    }
}
