// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The session on the wire: `qsipart`'s frames (an eight-byte header
//! with the magic `QI`, a kind, a length and a check; bodies at most
//! [`qsipart::frame::MAX_BODY`] bytes) with the kinds in [`kind`].
//!
//! A request, reply or event is an envelope ([`crate::Request`],
//! [`crate::Reply`], [`crate::Event`], each an [`ipc::Wire`] value) in
//! one or more frames ([`parts`]). The other bodies are small and fit
//! one frame. Every body is read leniently: what follows the fields
//! this version knows is passed over.

pub mod kind;
pub mod parts;

use ipc::{Dec, Wire, wire_struct};

pub use parts::{FIRST_ROOM, message_frames};

/// The longest text the session puts in a goodbye, a reset or a
/// refusal of its own making.
pub const MAX_TEXT: usize = 1024;

/// A body of `T`, leniently; `None` when it does not read.
pub fn read<T: Wire>(body: &[u8]) -> Option<T> {
    T::get(&mut Dec::new(body)).ok()
}

/// A goodbye: the session ends. `msgid` is empty for a clean close and
/// names the reason otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Goodbye {
    /// Why, or empty.
    pub msgid: String,
    /// In words.
    pub text: String,
}
wire_struct!(Goodbye { msgid, text });

/// More credit on a stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Credit {
    /// The stream.
    pub stream: u32,
    /// Bytes more the sender may send.
    pub grant: u32,
}
wire_struct!(Credit { stream, grant });

/// A stream aborted, both directions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reset {
    /// The stream.
    pub stream: u32,
    /// Why.
    pub msgid: String,
    /// In words.
    pub text: String,
}
wire_struct!(Reset {
    stream,
    msgid,
    text
});

/// A data frame's body: the stream's number, then the bytes as they
/// are (no length: the frame has it).
pub fn data_body(stream: u32, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&stream.to_le_bytes());
    out.extend_from_slice(bytes);
    out
}

/// A data frame's stream and bytes; `None` when shorter than a number.
pub fn read_data(body: &[u8]) -> Option<(u32, &[u8])> {
    let (n, rest) = body.split_first_chunk::<4>()?;
    Some((u32::from_le_bytes(*n), rest))
}

/// `s` cut to at most `max` bytes, on a character boundary.
pub(crate) fn clip(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}

/// A whole frame's bytes; `None` when the body is longer than a frame.
pub fn frame(kind: u16, body: &[u8]) -> Option<Vec<u8>> {
    qsipart::frame::encode(kind, body).ok()
}
