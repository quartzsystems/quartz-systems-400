// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fuzz targets. Nothing may panic, hang or grow without bound, and
//! the sessions' invariants (limits held, credit never overdrawn) must
//! hold after every step.
//!
//! - [`frames`]: bytes in pieces into a console and a managed session
//!   after a good hello, through every decoder down to `qsi::any`.
//! - [`handshake`]: bytes from the first one on, the hello included.
//! - [`streams`]: bytes as a script of operations on two connected
//!   sessions, with raw frames injected.

mod frames;
mod handshake;
mod streams;

pub use frames::frames;
pub use handshake::handshake;
pub use streams::streams;

use crate::{Incoming, Session};

/// Seeds for [`frames`]: the piece size, then frames (a ping; a base
/// `Identify` request from user `A`).
pub const FRAMES_SEEDS: &[&[u8]] = &[
    b"\x07QI\x02\x10\x08\x00\xf5\xef\x01\x00\x00\x00\x00\x00\x00\x00",
    b"\x3cQI\x10\x10\x15\x00\xfa\xef\x11\x00\x00\x00\x01\x00\x00\x00\x01\x00\x00\x00A\x01\x00\x01\x00\x00\x00\x00\x00",
];

/// Seeds for [`handshake`]: the piece size, then a console's hello.
pub const HANDSHAKE_SEEDS: &[&[u8]] = &[
    b"\x05QI\x01\x10\x1b\x00\xe5\xef\x01\x00\x03\x00\x00\x00\x00\x00\x00\x00\x01\x02\x00\x00\x00MC\x02\x00\x00\x00V1\x00\x00\x00\x00",
];

/// Seeds for [`streams`]: scripts that open streams, send, grant, close
/// and reset.
pub const STREAMS_SEEDS: &[&[u8]] = &[
    b"\x00\x07\x00\x07\x01\x00\x40\x07\x02\x01\x40\x07\x03\x00\x10\x07\x05\x00\x07\x06\x01\x07",
    b"\x00\x07\x09\x00\x00\x08\x01\x00\x00\x00\xff\xff\xff\x7f\x07\x08\xff\x08\xff\x08\xff",
];

/// Tokens worth inserting: the magic and the session's kinds.
pub const DICT: &[&[u8]] = &[
    b"QI",
    b"\x01\x10",
    b"\x02\x10",
    b"\x04\x10",
    b"\x10\x10",
    b"\x11\x10",
    b"\x12\x10",
    b"\x13\x10",
    b"\x20\x10",
    b"\x21\x10",
    b"\x22\x10",
    b"\x23\x10",
    b"\xff\xff\xff\xff",
];

/// The most a fuzzed session may hold queued for its caller.
const MAX_QUEUED: usize = 1 << 16;

/// Everything a session hands up, drained; panics when an invariant
/// breaks.
fn drain(s: &mut Session, mut each: impl FnMut(&mut Session, Incoming)) {
    let mut n = 0;
    while let Some(i) = s.poll() {
        n += 1;
        assert!(n < MAX_QUEUED, "a session queued without bound");
        each(s, i);
    }
    if let Err(why) = s.invariants() {
        panic!("invariant: {why}");
    }
}

/// The managed side's answer to whatever comes up: every request
/// decoded, some answered, some refused, some given a stream; data
/// granted back.
fn answer(s: &mut Session, log: &crate::EventLog, i: Incoming) {
    match i {
        Incoming::Request(r) => {
            let _ = r.user.len();
            match r.decode() {
                Err(e) => {
                    let _ = s.refuse_bad(r.id, &e);
                }
                Ok(None) => {
                    let _ = s.refuse_unknown(r.id);
                }
                Ok(Some(qsi::any::AnyRequest::Base(qsi::base::Request::Subscribe(sub)))) => {
                    let _ = s.subscribe(r.id, log, sub.since);
                }
                Ok(Some(_)) if r.id % 3 == 0 => {
                    if let Ok(st) = s.reply_stream(r.id, Vec::new()) {
                        let _ = s.send(st, &r.body);
                    }
                }
                Ok(Some(_)) => {
                    let _ = s.reply(r.id, r.body);
                }
            }
        }
        Incoming::Data { stream, bytes } => {
            let _ = s.grant(stream, u32::try_from(bytes.len()).unwrap_or(u32::MAX));
        }
        _ => {}
    }
}

/// What the console does with what comes up: decode it.
fn look(_: &mut Session, i: Incoming) {
    match i {
        Incoming::Reply(r) => {
            let _ = r.decode();
            let _ = r.refusal();
        }
        Incoming::Event(e) => {
            let _ = e.decode();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn noise(len: usize, seed: u32) -> Vec<u8> {
        let mut x = seed | 1;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                match x % 11 {
                    0 => b'Q',
                    1 => b'I',
                    2 => 0x10,
                    _ => x.to_le_bytes()[1],
                }
            })
            .collect()
    }

    /// Good frames of the session's kinds with bodies that are partly
    /// envelopes, partly noise.
    fn framed_noise(seed: u32) -> Vec<u8> {
        use crate::wire::{frame, kind};
        const KINDS: [u16; 12] = [
            kind::HELLO,
            kind::PING,
            kind::PONG,
            kind::GOODBYE,
            kind::REQUEST,
            kind::REPLY,
            kind::EVENT,
            kind::PART,
            kind::DATA,
            kind::CREDIT,
            kind::CLOSE,
            kind::RESET,
        ];
        let bytes = noise(4000, seed);
        let mut out = vec![bytes[0]];
        for (i, piece) in bytes.chunks(37).enumerate() {
            let k = KINDS[usize::from(piece[0]) % KINDS.len()];
            let body = if i % 3 == 0 {
                let r = crate::Request {
                    id: u32::from(piece[1]),
                    user: "U".into(),
                    family: u16::from(piece[2] % 3),
                    op: u16::from(piece[3] % 8),
                    body: piece[4..].to_vec(),
                };
                out.extend(crate::wire::message_frames(kind::REQUEST, &ipc::encode(&r)));
                continue;
            } else if k == kind::DATA {
                crate::wire::data_body(u32::from(piece[1] % 4), &piece[2..])
            } else {
                piece[1..].to_vec()
            };
            out.extend(frame(k, &body).unwrap());
        }
        out
    }

    #[test]
    fn framed_noise_goes_deep() {
        for seed in 1..60 {
            let input = framed_noise(seed);
            frames(&input);
            handshake(&input);
        }
    }

    #[test]
    fn seeds_are_what_they_say() {
        use crate::wire::{frame, kind};
        let ping = frame(kind::PING, &1u64.to_le_bytes()).unwrap();
        assert_eq!(&FRAMES_SEEDS[0][1..], &ping[..]);
        let req = crate::Request {
            id: 1,
            user: "A".into(),
            family: qsi::family::BASE,
            op: qsi::base::op::IDENTIFY,
            body: Vec::new(),
        };
        let frames = crate::wire::message_frames(kind::REQUEST, &ipc::encode(&req));
        assert_eq!(&FRAMES_SEEDS[1][1..], &frames[..]);
        let hello = qsi::Hello {
            version: 1,
            caps: qsi::caps::MANAGED,
            kind: qsi::PeerKind::Console,
            name: "MC".into(),
            release: "V1".into(),
            serial: String::new(),
        };
        let h = frame(kind::HELLO, &ipc::encode(&hello)).unwrap();
        assert_eq!(&HANDSHAKE_SEEDS[0][1..], &h[..]);
    }

    #[test]
    fn every_entry_with_seeds_and_noise() {
        for s in FRAMES_SEEDS {
            frames(s);
        }
        for s in HANDSHAKE_SEEDS {
            handshake(s);
        }
        for s in STREAMS_SEEDS {
            streams(s);
        }
        for f in [frames, handshake, streams] {
            f(&[]);
            f(&[0]);
            for seed in 1..40 {
                f(&noise(4000, seed));
            }
        }
    }
}
