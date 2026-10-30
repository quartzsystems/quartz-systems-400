// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! User authentication (RFC 4252): who the users are and how passwords
//! are checked ([`Accounts`], answered by the Phase 6 store through the
//! job's link in production), the access rules of the configuration,
//! and the request state machine with `MaxAuthTries`.

pub mod access;
pub mod userauth;

pub use userauth::{Authed, Method, Step, UserAuth};

/// A user profile as the server needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    /// The profile name (upper case).
    pub name: String,
    /// `HOMEDIR`.
    pub home: String,
    /// The group profile and the supplemental groups.
    pub groups: Vec<String>,
    /// The UID.
    pub uid: u32,
}

/// What a password check answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordCheck {
    /// Signed on.
    Accepted,
    /// The password is right but expired; the user must change it at a
    /// display first.
    Expired,
    /// Refused: the message the sign-on would show (`CPF1107`,
    /// `CPF1393`, `CPF1394` ...).
    Refused {
        /// Message identifier.
        msgid: String,
        /// Its text.
        text: String,
    },
}

/// The client's network address.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Peer {
    /// The address, as text.
    pub addr: String,
    /// The port.
    pub port: u16,
}

/// User profiles and the password check. The production implementation
/// is the storage service's sign-on (`sec::signon::sign_on`, reached
/// through the job's link), which counts attempts against `QMAXSIGN`,
/// applies `QMAXSGNACN` to the profile, and writes the `PW` audit entry.
pub trait Accounts: Send + Sync {
    /// The profile `name` (already upper case), if it exists and is
    /// enabled.
    fn lookup(&self, name: &str) -> Option<Account>;
    /// Check a password for profile `name`.
    fn check_password(&self, name: &str, password: &str, peer: &Peer) -> PasswordCheck;
}

/// The profile name for a user name typed at the client: upper case, at
/// most ten characters; `None` when it cannot be a profile.
#[must_use]
pub fn profile_name(user: &str) -> Option<String> {
    let u = user.trim().to_ascii_uppercase();
    let ok = !u.is_empty()
        && u.len() <= 10
        && u.chars().all(|c| c.is_ascii_alphanumeric() || "#$@_".contains(c))
        && !u.starts_with(|c: char| c.is_ascii_digit() || c == '_');
    ok.then_some(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names() {
        assert_eq!(profile_name("qstest").as_deref(), Some("QSTEST"));
        assert_eq!(profile_name("toolongname1"), None);
        assert_eq!(profile_name("1abc"), None);
        assert_eq!(profile_name("a b"), None);
    }
}
