// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Why a connection ended.

use std::io;

use crate::wire::WireError;

/// A connection ended or could not go on.
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    /// The byte stream failed.
    #[error("connection: {0}")]
    Io(#[from] io::Error),
    /// The peer broke the protocol.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// A packet failed its integrity check.
    #[error("a packet failed its integrity check")]
    Integrity,
    /// No algorithm in common, or the exchange failed.
    #[error("key exchange failed: {0}")]
    Kex(String),
    /// The peer sent `SSH_MSG_DISCONNECT`.
    #[error("the peer disconnected ({reason}): {text}")]
    Disconnected {
        /// The reason code.
        reason: u32,
        /// The peer's text.
        text: String,
    },
    /// This side disconnected the peer.
    #[error("disconnected the peer ({reason}): {text}")]
    Refused {
        /// The reason code sent.
        reason: u32,
        /// The text sent.
        text: String,
    },
    /// The peer closed the stream.
    #[error("the peer closed the connection")]
    Closed,
}

impl From<WireError> for SshError {
    fn from(e: WireError) -> Self {
        Self::Protocol(e.to_string())
    }
}

/// A result in this crate.
pub type SshResult<T> = Result<T, SshError>;
