// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One error type for journaling. Variants with an IBM message ID
//! report it through [`JrnError::msgid`]; the text is Quartz's.

use store::StoreError;

/// Result with a [`JrnError`].
pub type Result<T> = std::result::Result<T, JrnError>;

/// Why a journal operation failed.
#[derive(Debug, thiserror::Error)]
pub enum JrnError {
    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),

    /// The object is not a journal.
    #[error("{name} is not a journal")]
    NotAJournal {
        /// Object name.
        name: String,
    },

    /// The object is not a journal receiver.
    #[error("{name} is not a journal receiver")]
    NotAReceiver {
        /// Object name.
        name: String,
    },

    /// The journal has no attached receiver.
    #[error("journal {journal} has no receiver attached")]
    NoReceiver {
        /// Journal name.
        journal: String,
    },

    /// The receiver is attached to a journal.
    #[error("receiver {receiver} is attached to journal {journal}")]
    ReceiverAttached {
        /// Receiver name.
        receiver: String,
        /// Journal name.
        journal: String,
    },

    /// The receiver was used by another journal.
    #[error("receiver {receiver} already belongs to journal {journal}")]
    ReceiverUsed {
        /// Receiver name.
        receiver: String,
        /// Journal name.
        journal: String,
    },

    /// The object is journaled already.
    #[error("{name} is already journaled to {journal}")]
    AlreadyJournaled {
        /// Object name.
        name: String,
        /// Journal name.
        journal: String,
    },

    /// The object is not journaled.
    #[error("{name} is not journaled")]
    NotJournaled {
        /// Object name.
        name: String,
    },

    /// Objects are still journaled to the journal.
    #[error("journal {journal} still has {count} journaled objects")]
    ObjectsJournaled {
        /// Journal name.
        journal: String,
        /// How many.
        count: u32,
    },

    /// A sequence or receiver range that does not exist.
    #[error("journal {journal}: {what}")]
    Range {
        /// Journal name.
        journal: String,
        /// What was wrong.
        what: String,
    },

    /// On-disk structures that make no sense.
    #[error("journal object {what}")]
    Corrupt {
        /// What was wrong.
        what: String,
    },
}

impl JrnError {
    /// The message ID a later phase sends for this error, if any.
    #[must_use]
    pub fn msgid(&self) -> Option<&'static str> {
        Some(match self {
            Self::Store(e) => return e.msgid(),
            Self::NotAJournal { .. } | Self::NotAReceiver { .. } => "CPF9801",
            Self::NoReceiver { .. } => "CPF7006",
            Self::ReceiverAttached { .. } => "CPF7022",
            Self::ReceiverUsed { .. } => "CPF7010",
            Self::AlreadyJournaled { .. } => "CPF7030",
            Self::NotJournaled { .. } => "CPF7032",
            Self::ObjectsJournaled { .. } => "CPF7021",
            Self::Range { .. } => "CPF7054",
            Self::Corrupt { .. } => "CPF3113",
        })
    }
}
