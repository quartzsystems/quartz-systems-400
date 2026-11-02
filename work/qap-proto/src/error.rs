// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Protocol errors.

use ipc::WireError;
use thiserror::Error;

/// A frame or body that could not be read or written.
#[derive(Debug, Error)]
pub enum ProtoError {
    /// The codec or the stream failed.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// An envelope longer than [`crate::MAX_FRAME`].
    #[error("frame of {len} bytes exceeds the 1 MiB limit")]
    TooLong {
        /// Its encoded length.
        len: usize,
    },
}
