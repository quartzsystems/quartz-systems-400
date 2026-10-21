// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a service tools user may do: a set of privileges, one bit each.

use std::fmt;

/// A set of privileges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Privilege(pub u64);

impl Privilege {
    /// Disk units: configuration changes (add, remove, mirror).
    pub const DISK_UNITS_ADMIN: Self = Self(1 << 0);
    /// Disk units: operations (suspend, resume, replace, display).
    pub const DISK_UNITS_OPERATIONS: Self = Self(1 << 1);
    /// Service tools security flags.
    pub const SECURITY: Self = Self(1 << 2);
    /// Service tools user IDs.
    pub const USER_IDS: Self = Self(1 << 3);
    /// LIC-side system values.
    pub const SYSTEM_VALUES: Self = Self(1 << 4);
    /// Display/alter/dump.
    pub const DISPLAY_ALTER_DUMP: Self = Self(1 << 5);
    /// The LIC log and the product activity log.
    pub const LOGS: Self = Self(1 << 6);
    /// Hardware service manager.
    pub const HARDWARE: Self = Self(1 << 7);
    /// Install the operating system.
    pub const INSTALL: Self = Self(1 << 8);
    /// Perform an IPL from DST.
    pub const IPL: Self = Self(1 << 9);
    /// Main storage dumps and traces (nothing uses them yet).
    pub const DUMPS_AND_TRACES: Self = Self(1 << 10);

    /// Every privilege.
    pub const ALL: Self = Self((1 << 11) - 1);
    /// None.
    pub const NONE: Self = Self(0);

    /// Whether every bit of `other` is set.
    #[must_use]
    pub fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Union.
    #[must_use]
    pub fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Difference.
    #[must_use]
    pub fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Each privilege with its display name.
    pub const NAMES: [(Self, &'static str); 11] = [
        (Self::DISK_UNITS_ADMIN, "Disk units - administration"),
        (Self::DISK_UNITS_OPERATIONS, "Disk units - operations"),
        (Self::SECURITY, "Service tool security"),
        (Self::USER_IDS, "Service tool user IDs"),
        (Self::SYSTEM_VALUES, "System values"),
        (Self::DISPLAY_ALTER_DUMP, "Display/alter/dump"),
        (
            Self::LOGS,
            "Licensed internal code log and product activity log",
        ),
        (Self::HARDWARE, "Hardware service manager"),
        (Self::INSTALL, "Install the operating system"),
        (Self::IPL, "Perform an IPL"),
        (Self::DUMPS_AND_TRACES, "Main storage dumps and traces"),
    ];
}

impl fmt::Display for Privilege {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = Self::NAMES
            .iter()
            .filter(|(p, _)| self.has(*p))
            .map(|(_, n)| *n)
            .collect();
        f.write_str(&names.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_combine() {
        let p = Privilege::DISK_UNITS_OPERATIONS.with(Privilege::LOGS);
        assert!(p.has(Privilege::LOGS));
        assert!(!p.has(Privilege::DISK_UNITS_ADMIN));
        assert!(Privilege::ALL.has(p));
        assert_eq!(p.without(Privilege::LOGS), Privilege::DISK_UNITS_OPERATIONS);
        assert_eq!(
            p.to_string(),
            "Disk units - operations, Licensed internal code log and product activity log"
        );
    }
}
