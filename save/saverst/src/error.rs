// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Engine errors. An error ends the whole save or restore; an object
//! that cannot be saved or restored is a result, not an error.

/// What stopped a save or restore.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The object store.
    #[error(transparent)]
    Store(#[from] store::StoreError),
    /// The database.
    #[error(transparent)]
    Db(#[from] db::DbError),
    /// Journaling.
    #[error(transparent)]
    Jrn(#[from] jrn::JrnError),
    /// The save stream or its medium.
    #[error(transparent)]
    Stream(#[from] savfmt::Error),
    /// Refused with an IBM message identifier.
    #[error("{msgid} {text}")]
    Refused {
        /// The message.
        msgid: &'static str,
        /// What was refused.
        text: String,
    },
}

impl Error {
    /// A refusal.
    pub fn refused(msgid: &'static str, text: impl Into<String>) -> Self {
        Self::Refused {
            msgid,
            text: text.into(),
        }
    }
}

/// Engine result.
pub type Result<T> = std::result::Result<T, Error>;
