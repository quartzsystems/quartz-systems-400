// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What can go wrong with message files.

/// Result with a [`MsgfError`].
pub type Result<T> = std::result::Result<T, MsgfError>;

/// A message file error.
#[derive(Debug, thiserror::Error)]
pub enum MsgfError {
    /// A source file that does not parse.
    #[error("message source: {0}")]
    Source(String),

    /// A substitution variable definition that is not understood.
    #[error("substitution variable {0:?} is not valid")]
    Var(String),

    /// A payload that cannot be read.
    #[error("*MSGF payload: {0}")]
    Payload(String),

    /// A message identifier that is not `AAAnnnn`.
    #[error("{0:?} is not a message identifier")]
    Id(String),

    /// The description already exists (`CPF2499`).
    #[error("CPF2499: message {0} already exists")]
    Exists(String),

    /// The description does not exist (`CPF2419`).
    #[error("CPF2419: message {0} not found")]
    NotFound(String),

    /// The store refused.
    #[error(transparent)]
    Store(#[from] store::StoreError),

    /// A name that is not an object name.
    #[error(transparent)]
    Name(#[from] qsname::NameError),
}
