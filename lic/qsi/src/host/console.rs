// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console family: a partition's console, addressed by partition
//! ID. [`Attach`]'s reply opens a stream both ways: the host sends
//! [`Out`] (a whole frame first, then each update, as `qsipart`'s
//! console port has them, or the serial port's bytes), the console
//! sends [`In`] (keys, a refresh, bytes for the serial port). The
//! stream is bytes without boundaries (credit may split a send), so
//! each value goes in a frame of `qsipart`'s, kind [`PIECE`]: write
//! [`Out::piece`] / [`In::piece`], read with [`Pieces`]. One viewer at a
//! time: a second attach is refused unless it takes over, and the one
//! it displaces is sent [`Out::Ended`] and its stream closed. A
//! [`Screen`] is the whole frame alone, without a stream.
//!
//! ID 0 ([`super::model::SYSTEM_ID`]) is the managed system itself: a
//! standalone system answers its own console there and refuses any
//! other ID, a host refuses 0.

use ipc::{Dec, Wire, WireError, wire_struct};
use qsipart::console::{Frame, Key, Update};

use super::model::{PartitionId, check_target};
use crate::{Family, family, messages};

/// Attach to a partition's console.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attach {
    /// The partition, or 0 for the managed system itself.
    pub id: PartitionId,
    /// Take it over from another viewer.
    pub take_over: bool,
}
wire_struct!(Attach { id, take_over });

/// The console's stream is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attached {
    /// The console is the serial port's: bytes flow, not frames.
    pub serial: bool,
}
wire_struct!(Attached { serial });

/// Ask for the console's screen as it is now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    /// The partition, or 0 for the managed system itself.
    pub id: PartitionId,
}
wire_struct!(Screen { id });

/// A console's screen, captured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capture {
    /// The console is the serial port's: there is no frame.
    pub serial: bool,
    /// The whole frame.
    pub frame: Option<Frame>,
    /// The screen as text, a line a row.
    pub text: String,
}
wire_struct!(Capture {
    serial,
    frame,
    text
});

impl Capture {
    /// The checks a receiver makes of the frame (`qsipart`'s).
    pub fn check(&self) -> Result<(), &'static str> {
        match &self.frame {
            Some(f) if f.check().is_err() => Err("a frame that does not check"),
            _ => Ok(()),
        }
    }
}

/// What the host sends on a console's stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    /// A whole frame: the first after attaching, and when the system
    /// sent one.
    Frame(Frame),
    /// What changed.
    Update(Update),
    /// The serial port's bytes, when that is the console.
    Serial(Vec<u8>),
    /// The viewer is detached: taken over, or the partition ended. The
    /// stream closes after it.
    Ended(String),
    /// A kind a later version has: its tag; the rest is passed over.
    Other(u8),
}

impl Wire for Out {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Frame(f) => {
                out.push(1);
                f.put(out);
            }
            Self::Update(u) => {
                out.push(2);
                u.put(out);
            }
            Self::Serial(b) => {
                out.push(3);
                b.put(out);
            }
            Self::Ended(why) => {
                out.push(4);
                why.put(out);
            }
            Self::Other(t) => out.push(*t),
        }
    }

    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            1 => Self::Frame(Frame::get(d)?),
            2 => Self::Update(Update::get(d)?),
            3 => Self::Serial(Wire::get(d)?),
            4 => Self::Ended(Wire::get(d)?),
            t => Self::Other(t),
        })
    }
}

/// What a console sends on its stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum In {
    /// A key, for the console port.
    Key(Key),
    /// Ask for a whole frame.
    Refresh,
    /// Bytes for the serial port, when that is the console.
    Bytes(Vec<u8>),
    /// A kind a later version has: its tag; the rest is passed over.
    Other(u8),
}

impl Wire for In {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Key(k) => {
                out.push(1);
                k.put(out);
            }
            Self::Refresh => out.push(2),
            Self::Bytes(b) => {
                out.push(3);
                b.put(out);
            }
            Self::Other(t) => out.push(*t),
        }
    }

    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            1 => Self::Key(Key::get(d)?),
            2 => Self::Refresh,
            3 => Self::Bytes(Wire::get(d)?),
            t => Self::Other(t),
        })
    }
}

/// The longest piece of serial bytes either side sends at once: a
/// piece is one frame.
pub const MAX_SERIAL: usize = 16 * 1024;

/// The frame kind of a piece on a console's stream.
pub const PIECE: u16 = 0x0401;

fn frame_of<T: Wire>(value: &T) -> Result<Vec<u8>, crate::Error> {
    qsipart::frame::encode(PIECE, &ipc::encode(value)).map_err(|_| crate::Error::Invalid {
        family: family::CONSOLE,
        op: op::ATTACH,
        why: "a piece longer than a frame",
    })
}

/// Pieces out of a console stream's bytes, in whatever pieces they
/// arrive; a frame of another kind is passed over.
#[derive(Debug, Default)]
pub struct Pieces {
    reader: qsipart::frame::Reader,
}

impl Pieces {
    /// Bytes from the stream.
    pub fn push(&mut self, bytes: &[u8]) {
        self.reader.push(bytes);
    }

    /// The next whole piece's body, for [`Out::decode`] or
    /// [`In::decode`]; `None` until one is in.
    pub fn next_piece(&mut self) -> Option<Vec<u8>> {
        while let Some(f) = self.reader.next_frame() {
            if f.kind == PIECE {
                return Some(f.body);
            }
        }
        None
    }

    /// Bytes passed over as junk: on a stream inside TLS, a reason to
    /// reset it.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.reader.skipped()
    }
}

impl Out {
    /// The value framed for the stream.
    ///
    /// # Errors
    /// It is longer than a frame holds.
    pub fn piece(&self) -> Result<Vec<u8>, crate::Error> {
        frame_of(self)
    }

    /// A piece of the stream, read leniently.
    pub fn decode(piece: &[u8]) -> Result<Self, crate::Error> {
        crate::message::body(family::CONSOLE, op::ATTACH, piece)
    }

    /// The checks a receiver makes: a frame or update on its grid (an
    /// update's grid is the last frame's, `qsipart`'s `Grid` checks
    /// it), serial bytes within [`MAX_SERIAL`].
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::Frame(f) if f.check().is_err() => Err("a frame that does not check"),
            Self::Serial(b) if b.len() > MAX_SERIAL => Err("more than 16 KiB of serial bytes"),
            _ => Ok(()),
        }
    }
}

impl In {
    /// The value framed for the stream.
    ///
    /// # Errors
    /// It is longer than a frame holds.
    pub fn piece(&self) -> Result<Vec<u8>, crate::Error> {
        frame_of(self)
    }

    /// A piece of the stream, read leniently.
    pub fn decode(piece: &[u8]) -> Result<Self, crate::Error> {
        crate::message::body(family::CONSOLE, op::ATTACH, piece)
    }

    /// The checks a receiver makes: serial bytes within [`MAX_SERIAL`].
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::Bytes(b) if b.len() > MAX_SERIAL => Err("more than 16 KiB of serial bytes"),
            _ => Ok(()),
        }
    }
}

/// Operation numbers.
pub mod op {
    /// [`super::Attach`].
    pub const ATTACH: u16 = 0x0001;
    /// [`super::Screen`].
    pub const SCREEN: u16 = 0x0002;
}

/// The family's own refusals (`QSI0601` to `QSI0699`), for a managed
/// side with no identifiers of its own (a standalone system); a host
/// refuses with its own (`QVS2314`).
pub mod msgid {
    /// Another viewer has the console and this attach did not take it
    /// over.
    pub const VIEWED: &str = "QSI0601";
}

messages! {
    /// What a console asks.
    pub enum Request: family::CONSOLE => {
        /// Attach: the reply opens the stream.
        0x0001 => Attach(Attach),
        /// The screen alone.
        0x0002 => Screen(Screen),
    }
}

messages! {
    /// What the host answers.
    pub enum Reply: family::CONSOLE => {
        /// Attached.
        0x0001 => Attach(Attached),
        /// The screen.
        0x0002 => Screen(Capture),
    }
}

messages! {
    /// The family has no events: what a viewer is told is on its stream.
    pub enum Event: family::CONSOLE => {}
}

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::Attach(Attach { id, .. }) | Self::Screen(Screen { id }) => check_target(*id),
        }
    }
}

/// The console family.
#[derive(Debug)]
pub struct Console;

impl Family for Console {
    const ID: u16 = family::CONSOLE;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::CONSOLE;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
