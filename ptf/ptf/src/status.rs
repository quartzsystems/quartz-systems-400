// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PTF states `DSPPTF` shows (`docs/PTF.md` §2).

/// A PTF's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    /// Known, not on the system.
    NotLoaded,
    /// Loaded, not applied.
    Loaded,
    /// Applied; the replaced objects wait in `QRPLOBJ`.
    TempApplied,
    /// Applied for good.
    PermApplied,
    /// Removed after a temporary apply; still loaded.
    TempRemoved,
    /// Removed, save file deleted.
    PermRemoved,
    /// A later PTF supersedes it.
    Superseded,
    /// Did not verify.
    Damaged,
    /// Ordered.
    OnOrder,
    /// Only its save file is here.
    SaveFileOnly,
    /// Only its cover letter is here.
    CoverLetterOnly,
}

impl Status {
    const ALL: [(Self, &'static str, &'static str); 11] = [
        (Self::NotLoaded, "NOTLOADED", "Not loaded"),
        (Self::Loaded, "LOADED", "Loaded"),
        (Self::TempApplied, "TEMPAPPLIED", "Temporarily applied"),
        (Self::PermApplied, "PERMAPPLIED", "Permanently applied"),
        (Self::TempRemoved, "TEMPREMOVED", "Temporarily removed"),
        (Self::PermRemoved, "PERMREMOVED", "Permanently removed"),
        (Self::Superseded, "SUPERSEDED", "Superseded"),
        (Self::Damaged, "DAMAGED", "Damaged"),
        (Self::OnOrder, "ONORDER", "On order"),
        (Self::SaveFileOnly, "SAVFONLY", "Save file only"),
        (Self::CoverLetterOnly, "COVERONLY", "Cover letter only"),
    ];

    /// The text `DSPPTF` shows.
    #[must_use]
    pub fn display(self) -> &'static str {
        Self::ALL.iter().find(|e| e.0 == self).expect("listed").2
    }

    /// The code the PTF database keeps.
    #[must_use]
    pub fn code(self) -> &'static str {
        Self::ALL.iter().find(|e| e.0 == self).expect("listed").1
    }

    /// From a database code.
    #[must_use]
    pub fn from_code(c: &str) -> Option<Self> {
        Self::ALL.iter().find(|e| e.1 == c).map(|e| e.0)
    }

    /// Applied, temporarily or for good.
    #[must_use]
    pub fn applied(self) -> bool {
        matches!(self, Self::TempApplied | Self::PermApplied)
    }
}

/// What the next IPL does with a PTF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Action {
    /// Nothing.
    #[default]
    None,
    /// Apply it (`Apply at next IPL`).
    ApplyAtIpl,
    /// Remove it (`Remove at next IPL`).
    RemoveAtIpl,
}

impl Action {
    /// The text `DSPPTF` shows in the action column.
    #[must_use]
    pub fn display(self) -> &'static str {
        match self {
            Self::None => "",
            Self::ApplyAtIpl => "Apply at next IPL",
            Self::RemoveAtIpl => "Remove at next IPL",
        }
    }
}
