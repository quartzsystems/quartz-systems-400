// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Pairing: how a console and a managed system come to pin each other.
//!
//! The managed system shows its operator a one-time password
//! ([`one_time_password`]). A console that is not yet pinned connects
//! with its own certificate (the managed side lets it in while pairing
//! is open, for this and nothing else), and the two run SPAKE2 with the
//! password: the console as A with its certificate's fingerprint, the
//! managed side as B with its own. Each then proves it holds the key by
//! a MAC over the transcript: both fingerprints, a value exported from
//! this TLS connection's keys, and both SPAKE2 messages. So:
//!
//! - a listener learns nothing of the password (SPAKE2);
//! - a console without the password gets one guess per attempt, and
//!   the managed side counts attempts ([`Guard`]);
//! - someone in the middle, with a certificate of its own on each leg,
//!   fails the confirmation: the fingerprints and the exported value
//!   each end saw are not the other's;
//! - an exchange replayed fails: the managed side's SPAKE2 message is
//!   new each time, and so is the connection's exported value.
//!
//! The three messages: [`Start`] (console), [`Answer`] (managed side),
//! [`Confirm`] (console); then the managed side says it is done. On
//! success each side has a [`Paired`]: the other's fingerprint for its
//! caller to pin, with the peer's name.

mod otp;

use graviola::hashing::Sha256;
use graviola::hashing::hmac::Hmac;

use spake2::{Ed25519Group, Identity as SpakeId, Password, Spake2};

use crate::tls::Binding;
use crate::{Error, Fingerprint};

pub use otp::{normalize, one_time_password};

/// The longest name an end gives itself in pairing.
pub const MAX_NAME: usize = 256;
/// A SPAKE2 message's length (a side byte and a point).
pub const SPAKE_LEN: usize = 33;
/// A confirmation's length.
pub const CONFIRM_LEN: usize = 32;

/// What the managed side keeps for pairing; this crate keeps nothing.
pub trait Guard {
    /// The one-time password now out, if there is one that has not
    /// expired and has attempts left. Called once per attempt, before
    /// anything is answered: the caller counts it. `None` refuses.
    fn password(&mut self) -> Option<String>;
    /// An attempt failed its confirmation (a wrong password, or a
    /// relay).
    fn failed(&mut self) {}
    /// An attempt succeeded: pin the console. The caller retires the
    /// password.
    fn paired(&mut self, console: &Paired);
}

/// The console's first message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    /// SPAKE2's message A.
    pub spake: Vec<u8>,
    /// The console's name, for the managed side's operator.
    pub name: String,
}

/// The managed side's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// SPAKE2's message B.
    pub spake: Vec<u8>,
    /// The managed side's confirmation.
    pub confirm: Vec<u8>,
    /// The managed side's name.
    pub name: String,
}

/// The console's confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    /// The console's confirmation.
    pub confirm: Vec<u8>,
}

/// The outcome at either end: who to pin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paired {
    /// The peer's certificate fingerprint.
    pub fingerprint: Fingerprint,
    /// The peer's name, as it gave it.
    pub name: String,
}

/// The confirmation labels: one per side, so neither is the other's.
const LABEL_A: &[u8] = b"qsi pairing 1 console";
const LABEL_B: &[u8] = b"qsi pairing 1 managed";

/// The transcript: both fingerprints (console first), the connection's
/// exported value, both messages, both names.
fn transcript(
    console: &Fingerprint,
    managed: &Fingerprint,
    exporter: &[u8; 32],
    start: &Start,
    spake_b: &[u8],
    name_b: &str,
) -> Vec<u8> {
    let mut t = Vec::with_capacity(256);
    t.extend_from_slice(&console.0);
    t.extend_from_slice(&managed.0);
    t.extend_from_slice(exporter);
    for part in [
        &start.spake[..],
        spake_b,
        start.name.as_bytes(),
        name_b.as_bytes(),
    ] {
        t.extend_from_slice(&u32::try_from(part.len()).unwrap_or(u32::MAX).to_be_bytes());
        t.extend_from_slice(part);
    }
    t
}

fn mac(key: &[u8], label: &[u8], transcript: &[u8]) -> Vec<u8> {
    let mut h = Hmac::<Sha256>::new(key);
    h.update(label);
    h.update(transcript);
    h.finish().as_ref().to_vec()
}

fn verify(key: &[u8], label: &[u8], transcript: &[u8], tag: &[u8]) -> bool {
    let mut h = Hmac::<Sha256>::new(key);
    h.update(label);
    h.update(transcript);
    tag.len() == CONFIRM_LEN && h.verify(tag).is_ok()
}

fn check_name(name: &str) -> Result<(), Error> {
    if name.len() > MAX_NAME {
        return Err(Error::Pairing("a name longer than 256 bytes"));
    }
    Ok(())
}

/// The random source SPAKE2 draws from: the system's, through graviola.
struct Rng;

impl rand_core::RngCore for Rng {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.fill_bytes(&mut b);
        u32::from_le_bytes(b)
    }
    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        // No random source is a broken system: there is nothing safe to
        // fall back to.
        graviola::random::fill(dest).expect("the system's random source");
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl rand_core::CryptoRng for Rng {}

/// The console's side, between its [`Start`] and the [`Answer`].
pub struct Console {
    spake: Spake2<Ed25519Group>,
    binding: Binding,
    start: Start,
}

impl std::fmt::Debug for Console {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Console")
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}

impl Console {
    /// Begin pairing over the connection `binding` with the password
    /// the operator typed. `name` is the console's.
    ///
    /// # Errors
    /// [`Error::Pairing`] when the name is too long.
    pub fn start(password: &str, binding: Binding, name: &str) -> Result<(Self, Start), Error> {
        check_name(name)?;
        let (spake, msg) = Spake2::<Ed25519Group>::start_a_with_rng(
            &Password::new(normalize(password)),
            &SpakeId::new(&binding.mine.0),
            &SpakeId::new(&binding.peer.0),
            Rng,
        );
        let start = Start {
            spake: msg,
            name: name.to_string(),
        };
        Ok((
            Self {
                spake,
                binding,
                start: start.clone(),
            },
            start,
        ))
    }

    /// The managed side's answer: checked, then the console's
    /// confirmation and who it paired with.
    ///
    /// # Errors
    /// [`Error::Pairing`] when the answer does not confirm: a wrong
    /// password, or someone in the middle.
    pub fn answer(self, answer: &Answer) -> Result<(Confirm, Paired), Error> {
        check_name(&answer.name)?;
        if answer.spake.len() != SPAKE_LEN {
            return Err(Error::Pairing("the answer's message is not one"));
        }
        let key = self
            .spake
            .finish(&answer.spake)
            .map_err(|_| Error::Pairing("the answer's message is not one"))?;
        let t = transcript(
            &self.binding.mine,
            &self.binding.peer,
            &self.binding.exporter,
            &self.start,
            &answer.spake,
            &answer.name,
        );
        if !verify(&key, LABEL_B, &t, &answer.confirm) {
            return Err(Error::Pairing(
                "the managed side's confirmation does not hold",
            ));
        }
        Ok((
            Confirm {
                confirm: mac(&key, LABEL_A, &t),
            },
            Paired {
                fingerprint: self.binding.peer,
                name: answer.name.clone(),
            },
        ))
    }
}

/// The managed side's side, between its [`Answer`] and the [`Confirm`].
pub struct Managed {
    key: Vec<u8>,
    transcript: Vec<u8>,
    console: Paired,
}

impl std::fmt::Debug for Managed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Managed")
            .field("console", &self.console)
            .finish_non_exhaustive()
    }
}

impl Managed {
    /// Answer a console's [`Start`] over the connection `binding`, with
    /// the password `guard` has out. `name` is the managed side's.
    ///
    /// # Errors
    /// [`Error::Pairing`] when no password is out (or none is left to
    /// try) or the start is not one.
    pub fn answer(
        guard: &mut dyn Guard,
        binding: Binding,
        start: &Start,
        name: &str,
    ) -> Result<(Self, Answer), Error> {
        check_name(name)?;
        check_name(&start.name)?;
        if start.spake.len() != SPAKE_LEN {
            return Err(Error::Pairing("the start's message is not one"));
        }
        let password = guard
            .password()
            .ok_or(Error::Pairing("no pairing password is out"))?;
        let (spake, msg) = Spake2::<Ed25519Group>::start_b_with_rng(
            &Password::new(normalize(&password)),
            &SpakeId::new(&binding.peer.0),
            &SpakeId::new(&binding.mine.0),
            Rng,
        );
        let key = spake
            .finish(&start.spake)
            .map_err(|_| Error::Pairing("the start's message is not one"))?;
        let t = transcript(
            &binding.peer,
            &binding.mine,
            &binding.exporter,
            start,
            &msg,
            name,
        );
        let answer = Answer {
            confirm: mac(&key, LABEL_B, &t),
            spake: msg,
            name: name.to_string(),
        };
        Ok((
            Self {
                key,
                transcript: t,
                console: Paired {
                    fingerprint: binding.peer,
                    name: start.name.clone(),
                },
            },
            answer,
        ))
    }

    /// The console's confirmation: checked, and on success the console
    /// is given to `guard` to pin.
    ///
    /// # Errors
    /// [`Error::Pairing`] when it does not hold; `guard` hears of it.
    pub fn confirm(self, guard: &mut dyn Guard, confirm: &Confirm) -> Result<Paired, Error> {
        if !verify(&self.key, LABEL_A, &self.transcript, &confirm.confirm) {
            guard.failed();
            return Err(Error::Pairing("the console's confirmation does not hold"));
        }
        guard.paired(&self.console);
        Ok(self.console)
    }
}

#[cfg(test)]
mod tests;
