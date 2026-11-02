// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Why the machine refused a disk change, as the message line says it.
//! LIC screens have no message IDs, so these texts are the stable form;
//! [`for_unit`] names the unit a reason is about.

/// The unit is half of a mirrored pair.
pub const MIRRORED: &str = "The unit already has mirrored protection";
/// The unit is not half of a mirrored pair.
pub const NOT_MIRRORED: &str = "The unit does not have mirrored protection";
/// The unit is in a device parity set.
pub const IN_PARITY_SET: &str = "The unit is already in a device parity set";
/// The unit is in no device parity set.
pub const NOT_IN_PARITY_SET: &str = "The unit is not in a device parity set";
/// The unit is not attached.
pub const NOT_ATTACHED: &str = "The unit is not attached";
/// The status of the unit does not allow the change.
pub const HALF_STATE: &str = "The status of the unit does not allow this";
/// Only the last unit can leave the configuration.
pub const NOT_LAST: &str = "Only the last unit of the ASP can be removed";
/// Data is still on the unit.
pub const HAS_DATA: &str = "The unit still holds data";
/// The data on the unit does not fit on the others.
pub const CANNOT_MOVE: &str = "The data on the unit does not fit on the other units";
/// RAID 5 needs 3 to 18 units.
pub const RAID5_UNITS: &str = "RAID 5 needs 3 to 18 units";
/// RAID 6 needs 4 to 18 units.
pub const RAID6_UNITS: &str = "RAID 6 needs 4 to 18 units";
/// Data lies in the share of the unit that parity takes.
pub const PARITY_SHARE_IN_USE: &str = "Data lies in the part of the unit that parity needs";
/// The ASP has the most sets it can hold.
pub const PARITY_SETS_FULL: &str = "ASP 1 has the most device parity sets it can hold";
/// The set does not exist.
pub const NO_PARITY_SET: &str = "The device parity set does not exist";
/// The set has a failed or rebuilding unit, or is synchronizing.
pub const PARITY_NOT_ACTIVE: &str = "The parity set is not active: a unit failed, rebuilds or syncs";
/// The unit to replace has not failed.
pub const NOT_FAILED: &str = "The unit has not failed";
/// Part of the IFS lives on the unit, and the IFS never moves.
pub const HOLDS_IFS: &str = "Part of the IFS is on the unit and cannot move";
/// The IFS cannot grow below the ASP threshold.
pub const IFS_NO_ROOM: &str = "The ASP has no room for the IFS below its threshold";
/// The IFS is in its own partition.
pub const IFS_NOT_IN_ASP: &str = "The IFS is not in the ASP";
/// The disk is smaller than the unit needs.
pub const TOO_SMALL: &str = "The disk is too small for the unit";
/// The ASP has no room for the move.
pub const NO_SPACE: &str = "ASP 1 does not have the space this needs";
/// A resume already runs.
pub const RESUME_RUNNING: &str = "A resume is already running; wait for it to end";
/// The disk could not be opened.
pub const DISK_NOT_OPENED: &str = "The disk could not be opened";

/// `reason` with the unit it is about, numbered as the displays number
/// units (from 1).
#[must_use]
pub fn for_unit(reason: &str, unit: Option<u16>) -> String {
    match unit {
        Some(u) => format!("{reason}: unit {}", u + 1),
        None => reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_fit_the_message_line() {
        for r in [
            MIRRORED,
            NOT_MIRRORED,
            IN_PARITY_SET,
            NOT_IN_PARITY_SET,
            NOT_ATTACHED,
            HALF_STATE,
            NOT_LAST,
            HAS_DATA,
            CANNOT_MOVE,
            RAID5_UNITS,
            RAID6_UNITS,
            PARITY_SHARE_IN_USE,
            PARITY_SETS_FULL,
            NO_PARITY_SET,
            PARITY_NOT_ACTIVE,
            NOT_FAILED,
            TOO_SMALL,
            NO_SPACE,
            RESUME_RUNNING,
            DISK_NOT_OPENED,
        ] {
            assert!(for_unit(r, Some(999)).len() <= 78, "{r}");
        }
        assert_eq!(for_unit(MIRRORED, Some(2)), "The unit already has mirrored protection: unit 3");
    }
}
