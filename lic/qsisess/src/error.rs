// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the caller asked that the session will not do.

use qsi::msgid;
use thiserror::Error;

/// A local refusal. The peer's misbehaviour never comes back as one: it
/// becomes a refusal on the wire or a close.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// This side's own hello is out of bounds or not its role's.
    #[error("this side's hello: {0}")]
    BadHello(&'static str),
    /// The peer's hello has not come yet.
    #[error("the peer's hello has not come yet")]
    NotReady,
    /// The session is closed or closing.
    #[error("the session is closed")]
    Closed,
    /// The other role does that.
    #[error("only {0} does that")]
    WrongRole(&'static str),
    /// A family the two sides did not agree on.
    #[error("family {family:#06x} is not one both sides have")]
    NotAgreed {
        /// The family.
        family: u16,
    },
    /// The peer is not pinned: pairing only.
    #[error("the peer is not pinned: pairing only")]
    NotPinned,
    /// As many requests outstanding as the limit.
    #[error("{limit} requests are outstanding")]
    TooManyRequests {
        /// The limit.
        limit: usize,
    },
    /// As many streams open as the limit.
    #[error("{limit} streams are open")]
    TooManyStreams {
        /// The limit.
        limit: usize,
    },
    /// A message longer than the limit.
    #[error("a message of {len} bytes is longer than the limit of {limit}")]
    TooLong {
        /// Its length.
        len: usize,
        /// The limit.
        limit: usize,
    },
    /// A refusal with more messages than the limit.
    #[error("a refusal of {len} messages after the first; the limit is {limit}")]
    TooManyMessages {
        /// How many.
        len: usize,
        /// The limit.
        limit: usize,
    },
    /// A user name longer than the limit.
    #[error("a user name of {len} bytes is longer than the limit of {limit}")]
    UserTooLong {
        /// Its length.
        len: usize,
        /// The limit.
        limit: usize,
    },
    /// No request of this number is waiting for an answer.
    #[error("no request {0} is outstanding")]
    NoRequest(u32),
    /// A reply of another family or operation than its request's.
    #[error("the reply is {family:#06x}/{op:#06x}, its request was another")]
    WrongReply {
        /// The reply's family.
        family: u16,
        /// The reply's operation.
        op: u16,
    },
    /// The stream is not open.
    #[error("stream {0} is not open")]
    NoStream(u32),
    /// This side has closed its direction of the stream.
    #[error("stream {0} is closed this way")]
    StreamClosed(u32),
}

impl Error {
    /// The message identifier a peer would be given for the same thing.
    #[must_use]
    pub fn msgid(&self) -> &'static str {
        match self {
            Self::NotReady => msgid::OUT_OF_ORDER,
            Self::Closed => msgid::CLOSING,
            Self::BadHello(_)
            | Self::WrongRole(_)
            | Self::WrongReply { .. }
            | Self::NoRequest(_) => msgid::BAD,
            Self::NotAgreed { .. } => msgid::NOT_AGREED,
            Self::NotPinned => msgid::NOT_PINNED,
            Self::TooManyRequests { .. } => msgid::TOO_MANY_REQUESTS,
            Self::TooManyStreams { .. } => msgid::TOO_MANY_STREAMS,
            Self::TooLong { .. } | Self::UserTooLong { .. } | Self::TooManyMessages { .. } => {
                msgid::TOO_LONG
            }
            Self::NoStream(_) | Self::StreamClosed(_) => msgid::NO_STREAM,
        }
    }
}
