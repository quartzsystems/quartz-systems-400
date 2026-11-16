// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Requests and their replies or refusals. Only the console asks. The
//! managed side refuses by itself what the limits, the pin or the
//! agreed capabilities rule out; everything else goes up to its caller,
//! raw when the family or operation is unknown here (the caller refuses
//! it, [`Session::refuse_unknown`]).

use qsi::any::{AnyReply, AnyRequest};
use qsi::{Message, caps, family, msgid};

use super::{Role, Session, Stream, StreamId};
use crate::wire::{self, MAX_TEXT, kind};
use crate::{Error, Incoming, MAX_ALSO, MAX_MSGID, Note, Outcome, Reply, Request};

impl Session {
    /// Send a request; its number. `user` is the console's user who
    /// asked, for the managed side's log.
    ///
    /// # Errors
    /// Not open; the managed side's role; a family not agreed, or any
    /// but pairing before a pin; too many outstanding; too long.
    pub fn request(
        &mut self,
        user: &str,
        family: u16,
        op: u16,
        body: Vec<u8>,
    ) -> Result<u32, Error> {
        let agreed = self.ready()?;
        if self.role != Role::Console {
            return Err(Error::WrongRole("the console"));
        }
        if !self.pinned && family != family::PAIR {
            return Err(Error::NotPinned);
        }
        let cap = caps::of_request(family, op);
        if cap != 0 && !agreed.has(cap) {
            return Err(Error::NotAgreed { family });
        }
        if self.pending.len() >= self.limits.max_requests {
            return Err(Error::TooManyRequests {
                limit: self.limits.max_requests,
            });
        }
        if user.len() > self.limits.max_user {
            return Err(Error::UserTooLong {
                len: user.len(),
                limit: self.limits.max_user,
            });
        }
        let mut id = self.next_id;
        while id == 0 || self.pending.contains_key(&id) {
            id = id.wrapping_add(1);
        }
        let env = ipc::encode(&Request {
            id,
            user: user.to_owned(),
            family,
            op,
            body,
        });
        self.check_len(env.len())?;
        self.next_id = id.wrapping_add(1);
        self.pending.insert(id, (family, op));
        self.out.extend(wire::message_frames(kind::REQUEST, &env));
        Ok(id)
    }

    /// Send a typed request.
    ///
    /// # Errors
    /// As [`Self::request`].
    pub fn request_msg<M: Message>(&mut self, user: &str, m: &M) -> Result<u32, Error> {
        self.request(user, M::FAMILY, m.op(), m.encode())
    }

    /// Send any family's request.
    ///
    /// # Errors
    /// As [`Self::request`].
    pub fn request_any(&mut self, user: &str, r: &AnyRequest) -> Result<u32, Error> {
        self.request(user, r.family(), r.op(), r.encode())
    }

    /// Answer request `id` with a result. A reply longer than the limit
    /// goes as a refusal `QSI0104` instead, so the console is answered.
    ///
    /// # Errors
    /// Closed; no such request outstanding; too long.
    pub fn reply(&mut self, id: u32, body: Vec<u8>) -> Result<(), Error> {
        self.answer(id, None, Outcome::Result { body })
    }

    /// Answer with a typed reply, which must be the request's family and
    /// operation.
    ///
    /// # Errors
    /// As [`Self::reply`]; another family or operation than the request's.
    pub fn reply_msg<M: Message>(&mut self, id: u32, m: &M) -> Result<(), Error> {
        self.check_reply(id, M::FAMILY, m.op())?;
        self.reply(id, m.encode())
    }

    /// Answer with any family's reply.
    ///
    /// # Errors
    /// As [`Self::reply_msg`].
    pub fn reply_any(&mut self, id: u32, r: &AnyReply) -> Result<(), Error> {
        self.check_reply(id, r.family(), r.op())?;
        self.reply(id, r.encode())
    }

    /// Refuse request `id` with a message identifier and its text.
    ///
    /// # Errors
    /// Closed; no such request outstanding.
    pub fn refuse(&mut self, id: u32, msgid: &str, text: &str) -> Result<(), Error> {
        let outcome = Outcome::refused(&wire::clip(msgid, MAX_MSGID), &wire::clip(text, MAX_TEXT));
        self.answer(id, None, outcome)
    }

    /// Refuse request `id` with every message that applies: `main`
    /// first (what a console shows first), then `also` in order, each
    /// with its help. Texts are cut to their bounds.
    ///
    /// # Errors
    /// Closed; no such request outstanding; more than [`MAX_ALSO`]
    /// messages after the first.
    pub fn refuse_with(&mut self, id: u32, main: &Note, also: &[Note]) -> Result<(), Error> {
        if also.len() > MAX_ALSO {
            return Err(Error::TooManyMessages {
                len: also.len(),
                limit: MAX_ALSO,
            });
        }
        let main = main.clipped();
        let outcome = Outcome::Refused {
            msgid: main.msgid,
            text: main.text,
            help: main.help,
            also: also.iter().map(Note::clipped).collect(),
        };
        self.answer(id, None, outcome)
    }

    /// Refuse a request of a family or operation this side does not
    /// know: `QSI0001`.
    ///
    /// # Errors
    /// As [`Self::refuse`].
    pub fn refuse_unknown(&mut self, id: u32) -> Result<(), Error> {
        let Some(&(family, op)) = self.pending.get(&id) else {
            return Err(Error::NoRequest(id));
        };
        let text = format!("family {family:#06x} operation {op:#06x} is not known here");
        self.refuse(id, msgid::UNKNOWN, &text)
    }

    /// Refuse a request whose body did not read: `QSI0002`.
    ///
    /// # Errors
    /// As [`Self::refuse`].
    pub fn refuse_bad(&mut self, id: u32, why: &qsi::Error) -> Result<(), Error> {
        self.refuse(id, msgid::BAD, &why.to_string())
    }

    /// Answer request `id` with a result that opens a stream; the
    /// stream's number. Beyond the limit on open streams the answer
    /// goes as a refusal `QSI0103` instead.
    ///
    /// # Errors
    /// Closed; no such request; the peer not pinned; too many streams;
    /// too long (refused `QSI0104` instead).
    pub fn reply_stream(&mut self, id: u32, body: Vec<u8>) -> Result<StreamId, Error> {
        self.ready()?;
        if !self.pending.contains_key(&id) {
            return Err(Error::NoRequest(id));
        }
        if !self.pinned {
            return Err(Error::NotPinned);
        }
        if self.streams.len() >= self.limits.max_streams || self.max_stream == StreamId::MAX {
            let limit = self.limits.max_streams;
            self.refuse(
                id,
                msgid::TOO_MANY_STREAMS,
                &format!("{limit} streams are open"),
            )?;
            return Err(Error::TooManyStreams { limit });
        }
        let stream = self.max_stream + 1;
        self.answer(id, Some(stream), Outcome::Result { body })?;
        self.max_stream = stream;
        self.open_stream(stream);
        Ok(stream)
    }

    fn check_reply(&self, id: u32, family: u16, op: u16) -> Result<(), Error> {
        match self.pending.get(&id) {
            None => Err(Error::NoRequest(id)),
            Some(&p) if p == (family, op) => Ok(()),
            Some(_) => Err(Error::WrongReply { family, op }),
        }
    }

    fn check_len(&self, len: usize) -> Result<(), Error> {
        let limit = self.limits.message();
        if len > limit {
            return Err(Error::TooLong { len, limit });
        }
        Ok(())
    }

    /// The managed side's answer to `id`, on the wire.
    fn answer(&mut self, id: u32, stream: Option<StreamId>, outcome: Outcome) -> Result<(), Error> {
        self.ready()?;
        if self.role != Role::Managed {
            return Err(Error::WrongRole("the managed side"));
        }
        let Some(&(family, op)) = self.pending.get(&id) else {
            return Err(Error::NoRequest(id));
        };
        let reply = Reply {
            id,
            family,
            op,
            stream,
            outcome,
        };
        let env = ipc::encode(&reply);
        if let Err(e) = self.check_len(env.len()) {
            self.send_reply(&Reply {
                stream: None,
                outcome: Outcome::refused(
                    msgid::TOO_LONG,
                    &format!("a reply of {} bytes is longer than the limit", env.len()),
                ),
                ..reply
            });
            return Err(e);
        }
        self.out.extend(wire::message_frames(kind::REPLY, &env));
        self.pending.remove(&id);
        Ok(())
    }

    fn send_reply(&mut self, r: &Reply) {
        self.pending.remove(&r.id);
        self.out
            .extend(wire::message_frames(kind::REPLY, &ipc::encode(r)));
    }

    /// The session's own refusal of a request it does not pass up.
    fn refuse_here(&mut self, r: &Request, id: &str, text: &str) {
        self.send_reply(&Reply {
            id: r.id,
            family: r.family,
            op: r.op,
            stream: None,
            outcome: Outcome::refused(id, text),
        });
    }

    pub(super) fn on_request(&mut self, env: &[u8]) {
        if self.role != Role::Managed {
            return self.fail(msgid::OUT_OF_ORDER, "a request to the console");
        }
        let Some(r) = wire::read::<Request>(env) else {
            return self.fail(msgid::BAD_FRAME, "a request that does not read");
        };
        if self.pending.contains_key(&r.id) {
            return self.fail(msgid::OUT_OF_ORDER, "a request number already outstanding");
        }
        let agreed = self.agreed().map_or(0, |a| a.caps);
        let cap = caps::of_request(r.family, r.op);
        if r.user.len() > self.limits.max_user {
            self.refuse_here(
                &r,
                msgid::TOO_LONG,
                "the user name is longer than the limit",
            );
        } else if !self.pinned && r.family != family::PAIR {
            self.refuse_here(
                &r,
                msgid::NOT_PINNED,
                "the console is not pinned: pairing only",
            );
        } else if cap != 0 && agreed & cap != cap {
            self.refuse_here(
                &r,
                msgid::NOT_AGREED,
                "a family the two sides did not agree on",
            );
        } else if self.pending.len() >= self.limits.max_requests {
            self.refuse_here(
                &r,
                msgid::TOO_MANY_REQUESTS,
                "too many requests outstanding",
            );
        } else {
            self.pending.insert(r.id, (r.family, r.op));
            self.incoming.push_back(Incoming::Request(r));
        }
    }

    pub(super) fn on_reply(&mut self, env: &[u8]) {
        if self.role != Role::Console {
            return self.fail(msgid::OUT_OF_ORDER, "a reply to the managed side");
        }
        let Some(r) = wire::read::<Reply>(env) else {
            return self.fail(msgid::BAD_FRAME, "a reply that does not read");
        };
        if !r.outcome.fits() {
            return self.fail(msgid::TOO_LONG, "a refusal beyond its bounds");
        }
        if self.pending.get(&r.id) != Some(&(r.family, r.op)) {
            return self.fail(msgid::OUT_OF_ORDER, "a reply to no request outstanding");
        }
        if let Some(s) = r.stream
            && (s <= self.max_stream || matches!(r.outcome, Outcome::Refused { .. }))
        {
            return self.fail(msgid::OUT_OF_ORDER, "a reply with a stream out of order");
        }
        self.pending.remove(&r.id);
        let stream = r.stream;
        self.incoming.push_back(Incoming::Reply(r));
        if let Some(s) = stream {
            self.max_stream = s;
            if self.streams.len() >= self.limits.max_streams {
                self.reset_unknown(s, msgid::TOO_MANY_STREAMS, "too many streams open");
                self.incoming.push_back(Incoming::StreamReset {
                    stream: s,
                    msgid: msgid::TOO_MANY_STREAMS.into(),
                    text: "too many streams open".into(),
                    by_peer: false,
                });
            } else {
                self.open_stream(s);
            }
        }
    }

    /// A stream just opened: this side's window goes to the peer.
    fn open_stream(&mut self, s: StreamId) {
        let window = u64::from(self.limits.window);
        self.streams.insert(
            s,
            Stream {
                recv_window: window,
                granted_total: window,
                ..Stream::default()
            },
        );
        self.send_credit(s, self.limits.window);
    }
}
