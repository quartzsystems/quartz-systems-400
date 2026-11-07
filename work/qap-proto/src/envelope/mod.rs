// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The envelope every frame is, its framing, and replies larger than a
//! frame: the encoded body cut into pieces, `More` on all but the last,
//! the status on each and the message on the last; [`Reassembly`] joins
//! them again.

mod kinds;
mod message;

use std::collections::HashMap;
use std::io::{Read, Write};

use ipc::frame::{read_frame_max, write_frame_max};
use ipc::{Wire, WireError, decode, encode, wire_struct};

pub use kinds::{Kind, Service, Status};
pub use message::ServerMessage;

use crate::ops::Op;
use crate::{MAX_FRAME, ProtoError};

/// Flag bit 0: further reply frames follow for this request.
pub const MORE: u16 = 1;

/// One frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// What the frame is.
    pub kind: Kind,
    /// The client's ID for the request; 0 on events.
    pub request_id: u32,
    /// The service.
    pub service: Service,
    /// The operation within the service.
    pub op: u16,
    /// Bit set; only [`MORE`] is defined.
    pub flags: u16,
    /// How a reply or event ended; `Ok` on requests.
    pub status: Status,
    /// A message, when the reply or event carries one.
    pub message: Option<ServerMessage>,
    /// The Wire-encoded body.
    pub body: Vec<u8>,
}

wire_struct!(Envelope {
    kind,
    request_id,
    service,
    op,
    flags,
    status,
    message,
    body
});

impl Envelope {
    /// A request for `op` with `body`.
    pub fn request<B: Wire>(request_id: u32, op: Op, body: &B) -> Self {
        let (service, code) = op.code();
        Self {
            kind: Kind::Request,
            request_id,
            service,
            op: code,
            flags: 0,
            status: Status::Ok,
            message: None,
            body: encode(body),
        }
    }

    /// A one-frame reply to `req`.
    #[must_use]
    pub fn reply(
        req: &Envelope,
        status: Status,
        message: Option<ServerMessage>,
        body: Vec<u8>,
    ) -> Self {
        Self {
            kind: Kind::Reply,
            request_id: req.request_id,
            service: req.service,
            op: req.op,
            flags: 0,
            status,
            message,
            body,
        }
    }

    /// An event.
    #[must_use]
    pub fn event(
        service: Service,
        op: u16,
        status: Status,
        message: Option<ServerMessage>,
    ) -> Self {
        Self {
            kind: Kind::Event,
            request_id: 0,
            service,
            op,
            flags: 0,
            status,
            message,
            body: Vec::new(),
        }
    }

    /// The same frame with `body` encoded as its body.
    #[must_use]
    pub fn with_body<B: Wire>(self, body: &B) -> Self {
        Self {
            body: encode(body),
            ..self
        }
    }

    /// Whether a version-`v` peer may be sent it: its kind, service,
    /// status and, for an event, the event are all defined in `v`.
    #[must_use]
    pub fn defined_in(&self, v: u16) -> bool {
        let event = self.kind != Kind::Event
            || crate::ops::event::since(self.service, self.op).is_some_and(|s| s <= v);
        event && self.service.since() <= v && self.status.since() <= v
    }

    /// Cancel request `request_id`. The server reads only the ID.
    #[must_use]
    pub fn cancel(request_id: u32) -> Self {
        Self {
            kind: Kind::Cancel,
            request_id,
            service: Service::Connection,
            op: 0,
            flags: 0,
            status: Status::Ok,
            message: None,
            body: Vec::new(),
        }
    }

    /// Whether more reply frames follow.
    #[must_use]
    pub fn more(&self) -> bool {
        self.flags & MORE != 0
    }

    /// The operation, when the service and code name one.
    #[must_use]
    pub fn operation(&self) -> Option<Op> {
        Op::of(self.service, self.op)
    }

    /// The body as `T`.
    pub fn body_as<T: Wire>(&self) -> Result<T, WireError> {
        decode(&self.body)
    }

    /// Encoded, at most [`MAX_FRAME`] bytes.
    pub fn to_frame(&self) -> Result<Vec<u8>, ProtoError> {
        let b = encode(self);
        if b.len() > MAX_FRAME {
            return Err(ProtoError::TooLong { len: b.len() });
        }
        Ok(b)
    }

    /// Decoded from one frame of at most [`MAX_FRAME`] bytes.
    pub fn from_frame(b: &[u8]) -> Result<Self, ProtoError> {
        if b.len() > MAX_FRAME {
            return Err(ProtoError::TooLong { len: b.len() });
        }
        Ok(decode(b)?)
    }

    /// Write as one length-prefixed frame.
    pub fn write_to(&self, w: &mut impl Write) -> Result<(), ProtoError> {
        let b = self.to_frame()?;
        write_frame_max(w, &b, MAX_FRAME)?;
        Ok(())
    }

    /// Read one length-prefixed frame; a longer one is refused before
    /// its bytes are read.
    pub fn read_from(r: &mut impl Read) -> Result<Self, ProtoError> {
        match read_frame_max(r, MAX_FRAME) {
            Ok(b) => Ok(decode(&b)?),
            Err(WireError::TooLong { len }) => Err(ProtoError::TooLong { len }),
            Err(e) => Err(e.into()),
        }
    }

    /// The reply to `req` as frames of at most [`MAX_FRAME`] bytes: the
    /// body cut in order, `More` on all but the last, the message on
    /// the last.
    pub fn replies(
        req: &Envelope,
        status: Status,
        message: Option<ServerMessage>,
        body: Vec<u8>,
    ) -> Result<Vec<Self>, ProtoError> {
        // The head without the body, so a large body is not copied twice.
        let empty = Self::reply(req, status, message, Vec::new());
        let head = encode(&empty).len();
        if head + body.len() <= MAX_FRAME {
            return Ok(vec![Self { body, ..empty }]);
        }
        let room = MAX_FRAME
            .checked_sub(head)
            .filter(|n| *n > 0)
            .ok_or(ProtoError::TooLong { len: head })?;
        let n = body.len().div_ceil(room);
        let mut out = Vec::with_capacity(n);
        for (i, part) in body.chunks(room).enumerate() {
            let last = i + 1 == n;
            out.push(Self {
                flags: if last { 0 } else { MORE },
                message: if last { empty.message.clone() } else { None },
                body: part.to_vec(),
                ..Self::reply(req, status, None, Vec::new())
            });
        }
        Ok(out)
    }
}

/// Joins the frames of split replies: each reply comes back whole once
/// its last frame is in.
#[derive(Debug, Default)]
pub struct Reassembly {
    partial: HashMap<u32, Vec<u8>>,
}

impl Reassembly {
    /// A new one.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Take a frame; the whole reply when it was the last, events,
    /// cancels and requests as they are.
    pub fn push(&mut self, mut env: Envelope) -> Option<Envelope> {
        if env.kind != Kind::Reply {
            return Some(env);
        }
        if env.more() {
            self.partial
                .entry(env.request_id)
                .or_default()
                .extend_from_slice(&env.body);
            return None;
        }
        if let Some(mut head) = self.partial.remove(&env.request_id) {
            head.extend_from_slice(&env.body);
            env.body = head;
        }
        Some(env)
    }

    /// Requests with frames in but not the last.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.partial.len()
    }
}
