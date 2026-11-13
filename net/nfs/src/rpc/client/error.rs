// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Why a call failed.

use std::io;

use crate::rpc::msg::{AcceptError, Denied};
use crate::xdr;

/// A failed call.
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// No reply within the timeout (every retransmission included).
    #[error("no reply within the timeout")]
    Timeout,
    /// The caller's interrupt flag was set while waiting.
    #[error("the call was interrupted")]
    Interrupted,
    /// The server could not be reached.
    #[error("cannot connect: {0}")]
    Connect(io::Error),
    /// The connection broke while the call was out.
    #[error("the connection was lost: {0}")]
    ConnectionLost(io::Error),
    /// A socket error other than those.
    #[error("socket error: {0}")]
    Io(io::Error),
    /// Accepted but not run: no program, version or procedure, bad arguments.
    #[error("{0}")]
    Refused(AcceptError),
    /// Denied: an RPC version mismatch or an authentication error.
    #[error("{0}")]
    Denied(Denied),
    /// A reply record over the limit (the connection is closed).
    #[error("a reply of more than {0} bytes")]
    TooLarge(usize),
    /// The arguments break a bound of their description.
    #[error("the arguments cannot be encoded: {0}")]
    Args(xdr::Error),
    /// The reply or its results do not decode.
    #[error("the reply does not decode: {0}")]
    BadReply(xdr::Error),
}

impl RpcError {
    /// Whether the call may have reached nobody or been lost: a retry
    /// with the same XID is safe and a `hard` mount retries it.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::Connect(_) | Self::ConnectionLost(_)
        )
    }

    /// The version range of a `PROG_MISMATCH`.
    pub fn mismatch(&self) -> Option<(u32, u32)> {
        match self {
            Self::Refused(AcceptError::ProgMismatch { low, high }) => Some((*low, *high)),
            _ => None,
        }
    }
}
