// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Who may connect, as every connection sees it: the consoles pinned and
//! whether a pairing password is out ([`SharedPins`], refreshed by the
//! server from where they are kept), and what keeps the password and
//! the pins ([`Keeper`]).

use std::collections::BTreeSet;
use std::sync::{Arc, PoisonError, RwLock};

use qsitrust::Fingerprint;

/// A console's pairing attempt, for the keeper and the audit journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    /// The name it gave.
    pub name: String,
    /// Its certificate's fingerprint.
    pub fingerprint: Fingerprint,
    /// The address it connected from.
    pub address: String,
}

/// Why a pairing attempt failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// No password out, expired, or out of attempts.
    NoPassword,
    /// The confirmation did not hold: a wrong password or a relay.
    NotConfirmed,
    /// The console started and went away without confirming: it found
    /// the system's answer wrong, as a wrong password makes it.
    Abandoned,
    /// It confirmed, but the pin could not be kept.
    NotKept,
}

impl Failure {
    /// One letter for the audit entry.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::NoPassword => "N",
            Self::NotConfirmed => "C",
            Self::Abandoned => "A",
            Self::NotKept => "K",
        }
    }

    /// In words.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::NoPassword => "no pairing password was out, or it had no attempts left",
            Self::NotConfirmed => "the console's confirmation did not hold",
            Self::Abandoned => "the console ended the exchange before confirming",
            Self::NotKept => "the console could not be pinned",
        }
    }
}

/// What keeps the pairing password and the pins: the storage service
/// for the job, memory in a test. The password never leaves it but
/// for one attempt.
pub trait Keeper: Send + Sync {
    /// The password for one attempt, counted; none out, expired or
    /// spent is `None`.
    fn attempt(&self) -> Option<String>;
    /// An attempt failed.
    fn failed(&self, attempt: &Attempt, why: Failure);
    /// An attempt succeeded: pin the console and retire the password.
    /// `false` when it could not be kept.
    fn paired(&self, attempt: &Attempt) -> bool;
    /// A pinned console had a session from `address`.
    fn contact(&self, fingerprint: &Fingerprint, address: &str) {
        let _ = (fingerprint, address);
    }
}

#[derive(Debug, Default)]
struct PinState {
    pinned: BTreeSet<Fingerprint>,
    pairing: bool,
}

/// The pins and whether pairing is open, as the TLS handshakes judge
/// them; the server keeps it current.
#[derive(Clone, Debug, Default)]
pub struct SharedPins(Arc<RwLock<PinState>>);

impl SharedPins {
    /// Replace what is pinned and whether a password is out.
    pub fn set(&self, pinned: impl IntoIterator<Item = Fingerprint>, pairing: bool) {
        let mut s = self.0.write().unwrap_or_else(PoisonError::into_inner);
        s.pinned = pinned.into_iter().collect();
        s.pairing = pairing;
    }

    /// Pin one more at once (the pairing just done), before the next
    /// refresh.
    pub fn add(&self, fp: Fingerprint) {
        let mut s = self.0.write().unwrap_or_else(PoisonError::into_inner);
        s.pinned.insert(fp);
        s.pairing = false;
    }
}

impl qsitrust::Pins for SharedPins {
    fn pinned(&self, peer: &Fingerprint) -> bool {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .pinned
            .contains(peer)
    }

    fn pairing_open(&self) -> bool {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .pairing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsitrust::Pins;

    #[test]
    fn the_pins_follow_what_is_set() {
        let p = SharedPins::default();
        let a = Fingerprint([1; 32]);
        assert!(!p.pinned(&a) && !p.pairing_open());
        p.set([a], true);
        assert!(p.pinned(&a) && p.pairing_open());
        p.add(Fingerprint([2; 32]));
        assert!(p.pinned(&Fingerprint([2; 32])) && !p.pairing_open());
        assert_eq!(Failure::Abandoned.code(), "A");
    }
}
