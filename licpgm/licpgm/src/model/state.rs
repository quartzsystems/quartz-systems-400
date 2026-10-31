// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Load states, with IBM's two-character codes (QSZRTVPR's "load
//! state" field), their symbolic names and error indicators.

/// The state of one product load.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LoadState {
    /// `10`: in the definition, no load object yet.
    Defined,
    /// `20`: the load object exists, not packaged.
    Created,
    /// `30`: packaged.
    Packaged,
    /// `32`: packaged, then a library or folder was renamed.
    PackagedRenamed,
    /// `33`: packaged, an object was damaged at the last check.
    PackagedDamaged,
    /// `34`: packaged, an object was missing at the last check.
    PackagedMissing,
    /// `35`: RSTLICPGM running over a packaged load.
    RestoringPackaged,
    /// `38`: DLTLICPGM running over a packaged load.
    DeletingPackaged,
    /// `3E`: RSTLICPGM stopped by a preoperation exit; was packaged.
    PreExitFailedPackaged,
    /// `3F`: RSTLICPGM failed otherwise; was packaged.
    RestoreFailedPackaged,
    /// `50`: RSTLICPGM running over an installed load.
    RestoringInstalled,
    /// `53`: DLTLICPGM running over an installed load.
    DeletingInstalled,
    /// `59`: too old for this release.
    Incompatible,
    /// `60`: restored by RSTLICPGM, not yet checked.
    Loaded,
    /// `61`: restored, the postoperation exit failed.
    PostExitFailed,
    /// `62`: installed, then a library or folder was renamed.
    InstalledRenamed,
    /// `63`: installed, an object is damaged.
    InstalledDamaged,
    /// `64`: installed, an object was missing at the last check.
    InstalledMissing,
    /// `67`: CHKPRDOPT's exit program failed or reported an error.
    CheckExitFailed,
    /// `6E`: RSTLICPGM stopped by a preoperation exit; was installed.
    PreExitFailedInstalled,
    /// `6F`: RSTLICPGM failed otherwise; was installed.
    RestoreFailedInstalled,
    /// `90`: installed.
    Installed,
}

/// Every state, in code order.
pub const ALL_STATES: &[LoadState] = &[
    LoadState::Defined,
    LoadState::Created,
    LoadState::Packaged,
    LoadState::PackagedRenamed,
    LoadState::PackagedDamaged,
    LoadState::PackagedMissing,
    LoadState::RestoringPackaged,
    LoadState::DeletingPackaged,
    LoadState::PreExitFailedPackaged,
    LoadState::RestoreFailedPackaged,
    LoadState::RestoringInstalled,
    LoadState::DeletingInstalled,
    LoadState::Incompatible,
    LoadState::Loaded,
    LoadState::PostExitFailed,
    LoadState::InstalledRenamed,
    LoadState::InstalledDamaged,
    LoadState::InstalledMissing,
    LoadState::CheckExitFailed,
    LoadState::PreExitFailedInstalled,
    LoadState::RestoreFailedInstalled,
    LoadState::Installed,
];

impl LoadState {
    /// IBM's two-character code.
    #[must_use]
    pub fn code(self) -> &'static str {
        use LoadState::*;
        match self {
            Defined => "10",
            Created => "20",
            Packaged => "30",
            PackagedRenamed => "32",
            PackagedDamaged => "33",
            PackagedMissing => "34",
            RestoringPackaged => "35",
            DeletingPackaged => "38",
            PreExitFailedPackaged => "3E",
            RestoreFailedPackaged => "3F",
            RestoringInstalled => "50",
            DeletingInstalled => "53",
            Incompatible => "59",
            Loaded => "60",
            PostExitFailed => "61",
            InstalledRenamed => "62",
            InstalledDamaged => "63",
            InstalledMissing => "64",
            CheckExitFailed => "67",
            PreExitFailedInstalled => "6E",
            RestoreFailedInstalled => "6F",
            Installed => "90",
        }
    }

    /// The state with an IBM code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        ALL_STATES.iter().copied().find(|s| s.code() == code)
    }

    /// QSZRTVPR's symbolic load state.
    #[must_use]
    pub fn symbolic(self) -> &'static str {
        use LoadState::*;
        match self {
            Defined => "*DEFINED",
            Created => "*CREATED",
            PackagedDamaged | InstalledDamaged => "*DAMAGED",
            Packaged
            | PackagedRenamed
            | PackagedMissing
            | RestoringPackaged
            | DeletingPackaged
            | PreExitFailedPackaged
            | RestoreFailedPackaged
            | Incompatible => "*PACKAGED",
            Installed => "*INSTALLED",
            _ => "*LOADED",
        }
    }

    /// Whether QSZRTVPR's load error indicator reads `*ERROR`.
    #[must_use]
    pub fn is_error(self) -> bool {
        !matches!(
            self,
            LoadState::Defined
                | LoadState::Created
                | LoadState::Packaged
                | LoadState::Loaded
                | LoadState::Installed
        )
    }

    /// Whether the load counts as installed on this system (its
    /// objects were put here by RSTLICPGM): codes `5x`, `6x` and `90`.
    #[must_use]
    pub fn is_installed(self) -> bool {
        self.code().starts_with(['5', '6', '9'])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_classify() {
        for s in ALL_STATES {
            assert_eq!(LoadState::from_code(s.code()), Some(*s));
        }
        assert_eq!(LoadState::Installed.symbolic(), "*INSTALLED");
        assert_eq!(LoadState::InstalledDamaged.symbolic(), "*DAMAGED");
        assert_eq!(LoadState::PostExitFailed.symbolic(), "*LOADED");
        assert!(!LoadState::Installed.is_error());
        assert!(LoadState::InstalledMissing.is_error());
        assert!(LoadState::Loaded.is_installed());
        assert!(!LoadState::Packaged.is_installed());
        assert_eq!(LoadState::from_code("99"), None);
    }
}
