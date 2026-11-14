// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the entry store refuses, with the result code each answers.

use ldap::ResultCode;

use super::normalize::NormError;

/// An entry store error.
#[derive(Debug, thiserror::Error)]
pub enum DirError {
    /// The object store failed.
    #[error(transparent)]
    Store(#[from] store::StoreError),
    /// A name or value the schema refuses.
    #[error(transparent)]
    Norm(#[from] NormError),
    /// No entry with this ID.
    #[error("no entry {0}")]
    NoSuchEntry(u64),
    /// The name is taken.
    #[error("an entry of that name exists")]
    AlreadyExists,
    /// The entry has children.
    #[error("entry {0} has children")]
    NotLeaf(u64),
    /// A limit of §5.2.
    #[error("{0}")]
    Limit(String),
    /// An operation the store does not do (moving an entry under itself,
    /// renaming a suffix's root, removing an always-on index).
    #[error("{0}")]
    Unwilling(String),
    /// Bytes that are not what the format says.
    #[error("damaged directory store: {0}")]
    Corrupt(String),
}

impl DirError {
    /// The LDAP result code the operation answers.
    #[must_use]
    pub fn code(&self) -> ResultCode {
        match self {
            Self::Norm(e) => e.code,
            Self::NoSuchEntry(_) => ResultCode::NoSuchObject,
            Self::AlreadyExists => ResultCode::EntryAlreadyExists,
            Self::NotLeaf(_) => ResultCode::NotAllowedOnNonLeaf,
            Self::Limit(_) => ResultCode::AdminLimitExceeded,
            Self::Unwilling(_) => ResultCode::UnwillingToPerform,
            Self::Store(_) | Self::Corrupt(_) => ResultCode::OperationsError,
        }
    }
}

/// The entry store's result.
pub type Result<T> = std::result::Result<T, DirError>;
