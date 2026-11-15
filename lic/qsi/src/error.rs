// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What can be wrong with a message.

use thiserror::Error;

/// A message that cannot be read.
#[derive(Debug, Error)]
pub enum Error {
    /// The body does not decode as its operation's message.
    #[error("family {family:#06x} operation {op:#06x}: {source}")]
    Body {
        /// The family.
        family: u16,
        /// The operation.
        op: u16,
        /// Why.
        source: ipc::WireError,
    },
    /// The body decodes but says something impossible or out of range.
    #[error("family {family:#06x} operation {op:#06x}: {why}")]
    Invalid {
        /// The family.
        family: u16,
        /// The operation.
        op: u16,
        /// Why.
        why: &'static str,
    },
}
