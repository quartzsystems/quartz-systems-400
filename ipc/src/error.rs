// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Codec and channel errors.

use thiserror::Error;

/// A message that could not be decoded or moved.
#[derive(Debug, Error)]
pub enum WireError {
    /// The message ended before a value did.
    #[error("message truncated at byte {at}")]
    Truncated {
        /// Offset reached.
        at: usize,
    },
    /// An enum tag no variant has.
    #[error("unknown tag {tag} for {what}")]
    Tag {
        /// The type.
        what: &'static str,
        /// The tag.
        tag: u8,
    },
    /// A string that is not UTF-8.
    #[error("string is not UTF-8")]
    Utf8,
    /// Bytes left over after the value.
    #[error("{extra} byte(s) after the message")]
    Trailing {
        /// How many.
        extra: usize,
    },
    /// A frame longer than the limit.
    #[error("frame of {len} bytes exceeds the limit")]
    TooLong {
        /// Its length.
        len: usize,
    },
    /// The other end closed the channel.
    #[error("channel closed")]
    Closed,
    /// An I/O error on the channel.
    #[error("channel: {0}")]
    Io(#[from] std::io::Error),
}
