// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The envelopes of requests, replies and events, and what a session
//! hands its caller.

use ipc::{wire_enum, wire_struct};
use qsi::any::{AnyEvent, AnyReply, AnyRequest};
use qsi::{Agreed, Hello, Message};

use crate::StreamId;

/// A request: the console asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// Its number; the reply carries it.
    pub id: u32,
    /// The console's user who asked, for the managed side's log. It
    /// grants nothing.
    pub user: String,
    /// The family.
    pub family: u16,
    /// The operation.
    pub op: u16,
    /// The body, the operation's message.
    pub body: Vec<u8>,
}
wire_struct!(Request {
    id,
    user,
    family,
    op,
    body
});

impl Request {
    /// The body as any family's request; `None` for a family or
    /// operation this side does not know.
    ///
    /// # Errors
    /// The body does not read.
    pub fn decode(&self) -> Result<Option<AnyRequest>, qsi::Error> {
        AnyRequest::decode(self.family, self.op, &self.body)
    }

    /// The body as `M`; `None` when the request is another family's or
    /// an operation `M` does not know.
    ///
    /// # Errors
    /// The body does not read.
    pub fn typed<M: Message>(&self) -> Result<Option<M>, qsi::Error> {
        if self.family != M::FAMILY {
            return Ok(None);
        }
        M::decode(self.op, &self.body)
    }
}

/// The most messages a refusal carries after its first.
pub const MAX_ALSO: usize = 16;
/// The longest help text of a message, bytes.
pub const MAX_HELP: usize = 4096;
/// The longest message identifier, bytes.
pub const MAX_MSGID: usize = 16;

/// One message of a refusal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    /// The message identifier.
    pub msgid: String,
    /// Its text.
    pub text: String,
    /// Its help text; empty when none.
    pub help: String,
}
wire_struct!(Note { msgid, text, help });

impl Note {
    /// A message without help.
    #[must_use]
    pub fn new(msgid: &str, text: &str) -> Self {
        Self {
            msgid: msgid.into(),
            text: text.into(),
            help: String::new(),
        }
    }

    /// Whether it is within the bounds: [`MAX_MSGID`],
    /// [`crate::wire::MAX_TEXT`], [`MAX_HELP`].
    #[must_use]
    pub fn fits(&self) -> bool {
        self.msgid.len() <= MAX_MSGID
            && self.text.len() <= crate::wire::MAX_TEXT
            && self.help.len() <= MAX_HELP
    }

    /// Cut to the bounds.
    #[must_use]
    pub fn clipped(&self) -> Self {
        Self {
            msgid: crate::wire::clip(&self.msgid, MAX_MSGID),
            text: crate::wire::clip(&self.text, crate::wire::MAX_TEXT),
            help: crate::wire::clip(&self.help, MAX_HELP),
        }
    }
}

/// What a request came to: a result or a refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Done: the reply's message.
    Result {
        /// The body.
        body: Vec<u8>,
    },
    /// Refused with every message that applies: the first is the main
    /// one (what a console shows first), `also` the rest in order.
    Refused {
        /// The main message's identifier.
        msgid: String,
        /// Its text.
        text: String,
        /// Its help text; empty when none.
        help: String,
        /// The other messages, at most [`MAX_ALSO`].
        also: Vec<Note>,
    },
}
wire_enum!(Outcome {
    0 => Result { body },
    1 => Refused { msgid, text, help, also },
});

impl Outcome {
    /// A refusal with one message and no help.
    #[must_use]
    pub fn refused(msgid: &str, text: &str) -> Self {
        Self::Refused {
            msgid: msgid.into(),
            text: text.into(),
            help: String::new(),
            also: Vec::new(),
        }
    }

    /// Whether a refusal is within its bounds; a result always is.
    #[must_use]
    pub fn fits(&self) -> bool {
        match self {
            Self::Result { .. } => true,
            Self::Refused {
                msgid,
                text,
                help,
                also,
            } => {
                msgid.len() <= MAX_MSGID
                    && text.len() <= crate::wire::MAX_TEXT
                    && help.len() <= MAX_HELP
                    && also.len() <= MAX_ALSO
                    && also.iter().all(Note::fits)
            }
        }
    }
}

/// A reply: the managed side answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// The request's number.
    pub id: u32,
    /// The request's family.
    pub family: u16,
    /// The request's operation.
    pub op: u16,
    /// The stream the reply opened, if it did.
    pub stream: Option<StreamId>,
    /// The result or the refusal.
    pub outcome: Outcome,
}
wire_struct!(Reply {
    id,
    family,
    op,
    stream,
    outcome
});

impl Reply {
    /// The refusal's identifier and text, when it is one.
    #[must_use]
    pub fn refusal(&self) -> Option<(&str, &str)> {
        match &self.outcome {
            Outcome::Refused { msgid, text, .. } => Some((msgid, text)),
            Outcome::Result { .. } => None,
        }
    }

    /// Every message of a refusal, the main one first; empty for a
    /// result.
    #[must_use]
    pub fn notes(&self) -> Vec<Note> {
        match &self.outcome {
            Outcome::Refused {
                msgid,
                text,
                help,
                also,
            } => std::iter::once(Note {
                msgid: msgid.clone(),
                text: text.clone(),
                help: help.clone(),
            })
            .chain(also.iter().cloned())
            .collect(),
            Outcome::Result { .. } => Vec::new(),
        }
    }

    /// The result as any family's reply; `None` when refused, or of a
    /// family or operation this side does not know.
    ///
    /// # Errors
    /// The body does not read.
    pub fn decode(&self) -> Result<Option<AnyReply>, qsi::Error> {
        match &self.outcome {
            Outcome::Result { body } => AnyReply::decode(self.family, self.op, body),
            Outcome::Refused { .. } => Ok(None),
        }
    }

    /// The result as `M`; `None` when refused, another family's, or an
    /// operation `M` does not know.
    ///
    /// # Errors
    /// The body does not read.
    pub fn typed<M: Message>(&self) -> Result<Option<M>, qsi::Error> {
        match &self.outcome {
            Outcome::Result { body } if self.family == M::FAMILY => M::decode(self.op, body),
            _ => Ok(None),
        }
    }
}

/// An event: the managed side pushes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// Its number: counts up by one from 1, never reused by its log.
    pub seq: u64,
    /// The family.
    pub family: u16,
    /// The event's number in its family.
    pub op: u16,
    /// The body.
    pub body: Vec<u8>,
}
wire_struct!(Event {
    seq,
    family,
    op,
    body
});

impl Event {
    /// The body as any family's event; `None` for a family or event
    /// this side does not know.
    ///
    /// # Errors
    /// The body does not read.
    pub fn decode(&self) -> Result<Option<AnyEvent>, qsi::Error> {
        AnyEvent::decode(self.family, self.op, &self.body)
    }
}

/// What a session hands its caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Incoming {
    /// The peer's hello came: the session is open.
    Ready {
        /// The peer's hello.
        peer: Hello,
        /// What the two agree on.
        agreed: Agreed,
    },
    /// A request to answer (the managed side).
    Request(Request),
    /// A reply to a request (the console). Requests outstanding when
    /// the session closes come back as refusals, so none is left
    /// unanswered.
    Reply(Reply),
    /// An event (the console).
    Event(Event),
    /// Data on a stream: the next bytes of a byte stream, not one whole
    /// send (boundaries are not kept). Grant credit as it is consumed
    /// ([`crate::Session::grant`]).
    Data {
        /// The stream.
        stream: StreamId,
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// The peer gave credit: this much may be sent now.
    Writable {
        /// The stream.
        stream: StreamId,
        /// The credit now.
        credit: u64,
    },
    /// The peer will send no more data on the stream.
    StreamClosed {
        /// The stream.
        stream: StreamId,
    },
    /// The stream was aborted, by the peer or by this side over the
    /// peer's misbehaviour or the session's end.
    StreamReset {
        /// The stream.
        stream: StreamId,
        /// Why.
        msgid: String,
        /// In words.
        text: String,
        /// Whether the peer reset it.
        by_peer: bool,
    },
    /// The session is closed: cleanly (no `msgid`) or for a reason.
    Closed {
        /// Why, when it was not a clean close.
        msgid: Option<String>,
        /// In words.
        text: String,
        /// Whether the peer closed it.
        by_peer: bool,
    },
}
