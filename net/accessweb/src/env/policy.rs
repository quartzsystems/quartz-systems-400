// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The policy store: for each profile, group or `*PUBLIC`, the
//! functions set to Allow or Deny (`QUSRSYS/QZXWPOL` on the system).
//! What a setting means is [`crate::policy`]'s.

use qsweb::msg::Message;

/// One function's setting for one profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    /// Allowed.
    Allow,
    /// Denied.
    Deny,
}

impl Setting {
    /// `A` or `D`, as the store and the form keep it.
    pub fn code(self) -> char {
        match self {
            Setting::Allow => 'A',
            Setting::Deny => 'D',
        }
    }

    /// From `A` or `D`.
    pub fn from_code(c: &str) -> Option<Self> {
        match c.trim() {
            "A" => Some(Setting::Allow),
            "D" => Some(Setting::Deny),
            _ => None,
        }
    }
}

/// Where policies are kept.
pub trait PolicyStore: Send + Sync {
    /// `subject`'s own settings (a profile, a group or `*PUBLIC`):
    /// function key and setting.
    fn settings(&self, subject: &str) -> Vec<(String, Setting)>;
    /// Replace `subject`'s own settings with `entries`.
    fn put(&self, subject: &str, entries: &[(String, Setting)]) -> Result<(), Message>;
}
