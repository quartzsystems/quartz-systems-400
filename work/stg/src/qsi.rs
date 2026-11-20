// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QSI server over the link (`docs/QSID.md` section 12): one
//! request (`qsi`, tag 1600) carrying a [`QsiOp`], answered as a
//! [`QsiReply`]. The identity, the pairing password's attempts and the
//! pins are the server job's (`QSIDSRV`) alone to ask; making and ending
//! a password and removing a console are a security officer's (the
//! commands) or `slic`'s (the service tools); the system's state,
//! problems and dumps are the server job's.

#![allow(missing_docs)] // the fields are the operations'

use ipc::{wire_enum, wire_struct};

/// A console as the service keeps it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QsiConsole {
    /// The name it gave.
    pub name: String,
    /// Its certificate's fingerprint, 64 hexadecimal digits.
    pub fingerprint: String,
    /// When it paired, Unix seconds.
    pub paired: u64,
    /// When it last had a session, Unix seconds; 0 for never.
    pub last_contact: u64,
    /// The address it last connected from.
    pub address: String,
}

wire_struct!(QsiConsole {
    name,
    fingerprint,
    paired,
    last_contact,
    address
});

/// The pairing password's state, without its text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QsiPairing {
    /// A password is out.
    pub out: bool,
    /// When it is gone, Unix seconds.
    pub until: u64,
    /// Attempts left.
    pub attempts: u32,
}

wire_struct!(QsiPairing {
    out,
    until,
    attempts
});

/// Who asks for a password or a removal: the service tools user at DST,
/// else the job's profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QsiOp {
    /// The identity, cert and key; none made yet is `NoIdentity`.
    Identity,
    /// Keep an identity the server made (none kept yet).
    KeepIdentity { cert: Vec<u8>, key: Vec<u8> },
    /// The identity's fingerprint and when it was made, for showing.
    Fingerprint,
    /// The consoles pinned.
    Consoles,
    /// Make a pairing password in place of any out: shown once.
    StartPairing { by: String },
    /// End the pairing password.
    EndPairing { by: String },
    /// Whether a password is out.
    Pairing,
    /// The password for one attempt, counted.
    Attempt,
    /// An attempt failed: `why` its letter (`trust::Failure::code`).
    Failed {
        name: String,
        fingerprint: String,
        address: String,
        why: String,
    },
    /// An attempt succeeded: pin the console, retire the password.
    Paired {
        name: String,
        fingerprint: String,
        address: String,
    },
    /// Remove a console, by its name, its fingerprint or its place (1,
    /// 2).
    Remove { console: String, by: String },
    /// A pinned console had a session.
    Contact {
        fingerprint: String,
        address: String,
    },
    /// `identify`.
    Identify,
    /// Power down.
    Power {
        immediate: bool,
        delay: u32,
        restart: bool,
        user: String,
    },
    /// The clock's offset and zone.
    Time,
    /// The cumulative fix level.
    FixLevel,
    /// A request of the service family: its operation and body.
    Service {
        op: u16,
        body: Vec<u8>,
        user: String,
    },
    /// Every problem with its notes, for the events.
    Problems,
    /// The system's dumps.
    Dumps,
    /// Take a system dump.
    TakeDump { why: String, user: String },
    /// Delete one.
    DeleteDump { name: String, user: String },
    /// What the dumps take.
    DumpSpace,
    /// A piece of a dump's text.
    DumpRead { name: String, offset: u64, len: u32 },
    /// The event log file's bytes; empty when there is none.
    LoadEvents,
    /// Keep the event log file.
    SaveEvents { bytes: Vec<u8> },
}

wire_enum!(QsiOp {
    0 => Identity,
    1 => KeepIdentity { cert, key },
    2 => Fingerprint,
    3 => Consoles,
    4 => StartPairing { by },
    5 => EndPairing { by },
    6 => Pairing,
    7 => Attempt,
    8 => Failed { name, fingerprint, address, why },
    9 => Paired { name, fingerprint, address },
    10 => Remove { console, by },
    11 => Contact { fingerprint, address },
    12 => Identify,
    13 => Power { immediate, delay, restart, user },
    14 => Time,
    15 => FixLevel,
    16 => Service { op, body, user },
    17 => Problems,
    18 => Dumps,
    19 => TakeDump { why, user },
    20 => DeleteDump { name, user },
    21 => DumpSpace,
    22 => DumpRead { name, offset, len },
    23 => LoadEvents,
    24 => SaveEvents { bytes },
});

/// A problem with its notes, for the events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QsiProblem {
    pub summary: qsi::service::Summary,
    pub notes: Vec<qsi::service::Line>,
}

wire_struct!(QsiProblem { summary, notes });

/// The answer to a [`QsiOp`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QsiReply {
    /// Done.
    Done,
    /// Refused: every message, the first first (identifier, text).
    Refused { notes: Vec<(String, String)> },
    /// The identity.
    Identity { cert: Vec<u8>, key: Vec<u8> },
    /// None made yet.
    NoIdentity,
    /// The identity's fingerprint and when it was made.
    Fingerprint { hex: String, made: u64 },
    /// The consoles.
    Consoles { list: Vec<QsiConsole> },
    /// A password made: shown once.
    Password {
        text: String,
        until: u64,
        attempts: u32,
    },
    /// The password's state.
    Pairing { state: QsiPairing },
    /// One attempt's password, or none.
    Attempt { password: Option<String> },
    /// A yes or no.
    Flag { yes: bool },
    /// A console removed.
    Removed { console: QsiConsole },
    /// `identify`'s answer.
    Identify { identity: qsi::base::Identity },
    /// The clock and zone.
    Time { time: qsi::base::Time },
    /// The fix level.
    Level { level: qsi::base::Level },
    /// A service family reply's body.
    Body { body: Vec<u8> },
    /// Every problem.
    Problems { list: Vec<QsiProblem> },
    /// The dumps.
    Dumps { list: Vec<qsi::base::SystemDump> },
    /// A dump's name.
    Name { name: String },
    /// What the dumps take.
    Space { space: qsi::base::Space },
    /// Bytes, and the whole's length.
    Bytes { bytes: Vec<u8>, total: u64 },
}

wire_enum!(QsiReply {
    0 => Done,
    1 => Refused { notes },
    2 => Identity { cert, key },
    3 => NoIdentity,
    4 => Fingerprint { hex, made },
    5 => Consoles { list },
    6 => Password { text, until, attempts },
    7 => Pairing { state },
    8 => Attempt { password },
    9 => Flag { yes },
    10 => Removed { console },
    11 => Identify { identity },
    12 => Time { time },
    13 => Level { level },
    14 => Body { body },
    15 => Problems { list },
    16 => Dumps { list },
    17 => Name { name },
    18 => Space { space },
    19 => Bytes { bytes, total },
});

impl QsiReply {
    /// A refusal of one message.
    #[must_use]
    pub fn refused(msgid: &str, text: &str) -> Self {
        Self::Refused {
            notes: vec![(msgid.to_string(), text.to_string())],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_and_replies_cross_the_wire() {
        let ops = [
            QsiOp::Failed {
                name: "MC".into(),
                fingerprint: "ab".into(),
                address: "a".into(),
                why: "C".into(),
            },
            QsiOp::Service {
                op: 4,
                body: vec![1, 2],
                user: "U".into(),
            },
            QsiOp::DumpRead {
                name: "QSD0000001".into(),
                offset: 9,
                len: 16,
            },
        ];
        for op in ops {
            let back: QsiOp = ipc::decode(&ipc::encode(&op)).unwrap();
            assert_eq!(back, op);
        }
        let r = QsiReply::Consoles {
            list: vec![QsiConsole {
                name: "MC".into(),
                ..QsiConsole::default()
            }],
        };
        let back: QsiReply = ipc::decode(&ipc::encode(&r)).unwrap();
        assert_eq!(back, r);
    }
}
