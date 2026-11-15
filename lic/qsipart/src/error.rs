// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What can be wrong with a frame or a message.

use thiserror::Error;

/// A frame that cannot be made or a message that cannot be read.
#[derive(Debug, Error)]
pub enum Error {
    /// A body longer than a frame holds.
    #[error("a body of {len} bytes is longer than a frame holds")]
    TooLong {
        /// Its length.
        len: usize,
    },
    /// The body does not decode as its kind's message.
    #[error("message kind {kind:#06x}: {source}")]
    Body {
        /// The kind.
        kind: u16,
        /// Why.
        source: ipc::WireError,
    },
    /// The body decodes but says something impossible.
    #[error("message kind {kind:#06x}: {why}")]
    Invalid {
        /// The kind.
        kind: u16,
        /// Why.
        why: &'static str,
    },
}
