// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PTF errors, each with the IBM message it stands for.

/// What went wrong with a PTF.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The save stream.
    #[error(transparent)]
    Stream(#[from] savfmt::Error),
    /// The engine.
    #[error(transparent)]
    Engine(#[from] saverst::Error),
    /// The store.
    #[error(transparent)]
    Store(#[from] store::StoreError),
    /// Not a PTF stream, or its header is damaged.
    #[error("not a PTF: {0}")]
    NotPtf(String),
    /// A signature that does not verify.
    #[error("PTF {0} is not signed by a trusted signer")]
    Signature(String),
    /// Refused, with the message identifier.
    #[error("{msgid} {text}")]
    Refused {
        /// IBM message identifier.
        msgid: &'static str,
        /// Detail.
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

    /// The message identifier.
    #[must_use]
    pub fn msgid(&self) -> &'static str {
        match self {
            Self::Refused { msgid, .. } => msgid,
            Self::NotPtf(_) => "CPF3598",
            _ => "CPF3586",
        }
    }
}

/// PTF result.
pub type Result<T> = std::result::Result<T, Error>;
