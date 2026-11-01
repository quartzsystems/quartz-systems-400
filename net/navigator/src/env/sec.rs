// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Security: the audit journal `QSYS/QAUDJRN` as `DSPAUDJRNE` shows it.

use qsweb::msg::Message;

/// One audit journal entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditEntry {
    /// When, seconds since 1970.
    pub at: u64,
    /// The entry type (`AF`, `PW`, ...).
    pub kind: String,
    /// The user profile.
    pub user: String,
    /// The job (`number/user/name`).
    pub job: String,
    /// The object and detail columns, as the command prints them.
    pub detail: String,
}

/// Which entries: an entry type (blank for all), a user (`*ALL`) and a
/// span of seconds since 1970, both ends included.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditFilter {
    /// `PW`, or blank.
    pub kind: String,
    /// A user profile or `*ALL`.
    pub user: String,
    /// The first second.
    pub from: u64,
    /// The last second.
    pub to: u64,
}

/// The journal's attached receiver.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditReceiver {
    /// `QSYS/QAUDJRN`.
    pub journal: String,
    /// `AUDRCV0412`.
    pub receiver: String,
    /// When it was attached, seconds since 1970.
    pub attached: u64,
}

/// What the security pages need, as the user.
pub trait Security: Send + Sync {
    /// The entries that match, newest first (`DSPAUDJRNE`).
    fn audit_entries(&self, user: &str, filter: &AuditFilter) -> Result<Vec<AuditEntry>, Message>;
    /// The audit journal and its receiver (`DSPSECAUD`).
    fn audit_receiver(&self, user: &str) -> AuditReceiver;
}
