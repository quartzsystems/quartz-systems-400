// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One problem entry.

/// IBM's problem statuses, in the order a problem moves through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrbStatus {
    /// Logged, not analyzed.
    Opened,
    /// Analyzed; ready to report.
    Ready,
    /// Prepared for reporting (the report is built).
    Prepared,
    /// Sent to the service provider.
    Sent,
    /// The service provider answered.
    Answered,
    /// The fix was verified.
    Verified,
    /// Closed.
    Closed,
}

impl PrbStatus {
    /// Every status with its `*NAME` and display text.
    pub const ALL: [(Self, &'static str, &'static str); 7] = [
        (Self::Opened, "*OPENED", "OPENED"),
        (Self::Ready, "*READY", "READY"),
        (Self::Prepared, "*PREPARED", "PREPARED"),
        (Self::Sent, "*SENT", "SENT"),
        (Self::Answered, "*ANSWERED", "ANSWERED"),
        (Self::Verified, "*VERIFIED", "VERIFIED"),
        (Self::Closed, "*CLOSED", "CLOSED"),
    ];

    /// The text `WRKPRB` shows.
    #[must_use]
    pub fn display(self) -> &'static str {
        Self::ALL.iter().find(|e| e.0 == self).expect("listed").2
    }

    /// From `*OPENED` and the rest (or the display text).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let u = s.trim().to_ascii_uppercase();
        Self::ALL
            .iter()
            .find(|e| e.1 == u || e.2 == u)
            .map(|e| e.0)
    }
}

/// How the problem came to be logged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrbType {
    /// Machine detected (type 1).
    Machine,
    /// User detected, through `ANZPRB` (type 2).
    User,
    /// A PTF order (type 3).
    PtfOrder,
    /// Application detected (type 4).
    Application,
}

impl PrbType {
    /// IBM's type number.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            Self::Machine => 1,
            Self::User => 2,
            Self::PtfOrder => 3,
            Self::Application => 4,
        }
    }

    /// From the type number.
    #[must_use]
    pub fn from_number(n: u8) -> Option<Self> {
        Some(match n {
            1 => Self::Machine,
            2 => Self::User,
            3 => Self::PtfOrder,
            4 => Self::Application,
            _ => return None,
        })
    }
}

/// What raised the problem.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Origin {
    /// A message to `QSYSOPR`: its identifier.
    Message(String),
    /// A product activity log entry: the resource and its SRC.
    Hardware {
        /// Resource name (`DD001`, `TAP01`).
        resource: String,
        /// The SRC.
        src: String,
    },
    /// A dump: its name in the dump directory.
    Dump(String),
    /// `ANZPRB`.
    Analysis,
}

/// A problem entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Ten digits: `yyddd` of the day it was logged, then a sequence.
    pub id: String,
    /// Status.
    pub status: PrbStatus,
    /// Type.
    pub kind: PrbType,
    /// Severity 1 (high) to 4 (low).
    pub severity: u8,
    /// What raised it.
    pub origin: Origin,
    /// One-line description.
    pub text: String,
    /// The symptom string analysis built (`msgid`, `rsrc`, `src` keywords).
    pub symptoms: String,
    /// The system it was logged on.
    pub system: String,
    /// Unix seconds it was opened.
    pub opened: u64,
    /// Unix seconds of the last change.
    pub changed: u64,
    /// Who it is assigned to (`CHGPRB ASSIGNTO`).
    pub assigned_to: String,
    /// Notes, in order (`WRKPRB` option 12).
    pub notes: Vec<String>,
    /// Fixes the service provider named, PTF identifiers.
    pub fixes: Vec<String>,
}

impl Problem {
    /// Whether the problem is still open (anything before closed).
    #[must_use]
    pub fn open(&self) -> bool {
        self.status != PrbStatus::Closed
    }
}
