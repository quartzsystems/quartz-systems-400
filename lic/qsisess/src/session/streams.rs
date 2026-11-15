// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Streams: opened by a reply, numbered by the managed side, with
//! credit per stream per direction so one stalled stream never holds up
//! another or the requests. Each side grants its window when the stream
//! opens and more as its caller consumes; data beyond the credit resets
//! the stream (`QSI0105`). Close ends one direction; reset aborts both.
//!
//! A stream is a byte stream: the boundaries of what was sent are not
//! kept (credit may split a send, frames may split it again), so what
//! a stream carries frames itself (a console stream carries `qsipart`
//! frames, which are self-delimiting).
//!
//! A number at or below the highest seen that is not open is a stream
//! just closed or reset; frames for it still in flight are passed over.
//! A number above it was never opened: the stream is reset `QSI0106`.

use qsi::msgid;

use super::{Session, StreamId};
use crate::wire::{self, Credit, MAX_TEXT, Reset, kind};
use crate::{Error, Incoming};

/// The data a frame holds.
const DATA_ROOM: usize = qsipart::frame::MAX_BODY - 4;

impl Session {
    /// Send on `stream` as much of `bytes` as the credit allows; how
    /// much was taken. The rest waits for [`Incoming::Writable`]. The
    /// peer gets bytes, not this send as one piece.
    ///
    /// # Errors
    /// Closed; the stream not open; this direction closed.
    pub fn send(&mut self, stream: StreamId, bytes: &[u8]) -> Result<usize, Error> {
        self.ready()?;
        let s = self
            .streams
            .get_mut(&stream)
            .ok_or(Error::NoStream(stream))?;
        if s.sent_closed {
            return Err(Error::StreamClosed(stream));
        }
        let n = usize::try_from(s.send_credit)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        s.send_credit -= n as u64;
        s.sent_total += n as u64;
        for chunk in bytes[..n].chunks(DATA_ROOM) {
            self.frame(kind::DATA, &wire::data_body(stream, chunk));
        }
        Ok(n)
    }

    /// The credit this side has on `stream`.
    #[must_use]
    pub fn credit(&self, stream: StreamId) -> Option<u64> {
        self.streams.get(&stream).map(|s| s.send_credit)
    }

    /// Give the peer `n` bytes more credit on `stream`: the caller has
    /// consumed that much.
    ///
    /// # Errors
    /// Closed; the stream not open.
    pub fn grant(&mut self, stream: StreamId, n: u32) -> Result<(), Error> {
        self.ready()?;
        let s = self
            .streams
            .get_mut(&stream)
            .ok_or(Error::NoStream(stream))?;
        if s.recv_closed || n == 0 {
            return Ok(());
        }
        s.recv_window = s.recv_window.saturating_add(u64::from(n));
        s.granted_total = s.granted_total.saturating_add(u64::from(n));
        self.send_credit(stream, n);
        Ok(())
    }

    /// End this side's direction of `stream`; the peer may still send.
    ///
    /// # Errors
    /// Closed; the stream not open; already closed this way.
    pub fn close_stream(&mut self, stream: StreamId) -> Result<(), Error> {
        self.ready()?;
        let s = self
            .streams
            .get_mut(&stream)
            .ok_or(Error::NoStream(stream))?;
        if s.sent_closed {
            return Err(Error::StreamClosed(stream));
        }
        s.sent_closed = true;
        let done = s.recv_closed;
        self.frame(kind::CLOSE, &ipc::encode(&stream));
        if done {
            self.streams.remove(&stream);
        }
        Ok(())
    }

    /// Abort `stream`, both directions.
    ///
    /// # Errors
    /// Closed; the stream not open.
    pub fn reset_stream(&mut self, stream: StreamId, id: &str, text: &str) -> Result<(), Error> {
        self.ready()?;
        if self.streams.remove(&stream).is_none() {
            return Err(Error::NoStream(stream));
        }
        self.reset_unknown(stream, id, text);
        Ok(())
    }

    pub(super) fn send_credit(&mut self, stream: StreamId, grant: u32) {
        self.frame(kind::CREDIT, &ipc::encode(&Credit { stream, grant }));
    }

    /// A reset on the wire, for a stream no longer (or never) open.
    pub(super) fn reset_unknown(&mut self, stream: StreamId, id: &str, text: &str) {
        let r = Reset {
            stream,
            msgid: wire::clip(id, 16),
            text: wire::clip(text, MAX_TEXT),
        };
        self.frame(kind::RESET, &ipc::encode(&r));
    }

    /// This side resets a stream over the peer's misbehaviour.
    fn abort(&mut self, stream: StreamId, id: &str, text: &str) {
        self.streams.remove(&stream);
        self.reset_unknown(stream, id, text);
        self.incoming.push_back(Incoming::StreamReset {
            stream,
            msgid: id.into(),
            text: text.into(),
            by_peer: false,
        });
    }

    /// A frame for a stream not open: passed over when it was open once,
    /// the stream reset when never.
    fn not_open(&mut self, stream: StreamId) {
        if stream > self.max_stream || stream == 0 {
            self.reset_unknown(stream, msgid::NO_STREAM, "a stream that was never opened");
        }
    }

    pub(super) fn on_data(&mut self, body: &[u8]) {
        let Some((stream, bytes)) = wire::read_data(body) else {
            return self.fail(msgid::BAD_FRAME, "data that does not read");
        };
        let Some(s) = self.streams.get_mut(&stream) else {
            return self.not_open(stream);
        };
        if s.recv_closed {
            return self.abort(stream, msgid::NO_STREAM, "data after the stream's close");
        }
        let n = bytes.len() as u64;
        if n > s.recv_window {
            return self.abort(stream, msgid::NO_CREDIT, "data beyond the credit given");
        }
        if n == 0 {
            return;
        }
        s.recv_window -= n;
        s.received_total += n;
        self.incoming.push_back(Incoming::Data {
            stream,
            bytes: bytes.to_vec(),
        });
    }

    pub(super) fn on_credit(&mut self, body: &[u8]) {
        let Some(c) = wire::read::<Credit>(body) else {
            return self.fail(msgid::BAD_FRAME, "credit that does not read");
        };
        let Some(s) = self.streams.get_mut(&c.stream) else {
            return self.not_open(c.stream);
        };
        if s.sent_closed || c.grant == 0 {
            return;
        }
        s.send_credit = s.send_credit.saturating_add(u64::from(c.grant));
        s.credit_total = s.credit_total.saturating_add(u64::from(c.grant));
        let credit = s.send_credit;
        self.incoming.push_back(Incoming::Writable {
            stream: c.stream,
            credit,
        });
    }

    pub(super) fn on_close(&mut self, body: &[u8]) {
        let Some(stream) = wire::read::<u32>(body) else {
            return self.fail(msgid::BAD_FRAME, "a close that does not read");
        };
        let Some(s) = self.streams.get_mut(&stream) else {
            return self.not_open(stream);
        };
        if s.recv_closed {
            return;
        }
        s.recv_closed = true;
        s.recv_window = 0;
        s.granted_total = s.received_total;
        if s.sent_closed {
            self.streams.remove(&stream);
        }
        self.incoming.push_back(Incoming::StreamClosed { stream });
    }

    pub(super) fn on_reset(&mut self, body: &[u8]) {
        let Some(r) = wire::read::<Reset>(body) else {
            return self.fail(msgid::BAD_FRAME, "a reset that does not read");
        };
        if self.streams.remove(&r.stream).is_none() {
            return;
        }
        self.incoming.push_back(Incoming::StreamReset {
            stream: r.stream,
            msgid: wire::clip(&r.msgid, 16),
            text: wire::clip(&r.text, MAX_TEXT),
            by_peer: true,
        });
    }
}
