// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What can go wrong, with the machine interface message each maps to.

/// Result with a [`DecimalError`].
pub type Result<T> = std::result::Result<T, DecimalError>;

/// A decimal operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecimalError {
    /// Bytes that are not valid packed or zoned data (MCH1202).
    #[error("decimal data error: {what}")]
    DecimalData {
        /// Which byte or nibble was wrong.
        what: String,
    },

    /// A value does not fit the digits it is assigned to (MCH1210; the
    /// RPG runtime maps it to RNQ0103).
    #[error("value {value} does not fit {digits} digits with {scale} decimal positions")]
    SizeOverflow {
        /// The value, as text.
        value: String,
        /// Digits of the target.
        digits: u8,
        /// Decimal positions of the target.
        scale: u8,
    },

    /// Division by zero (MCH1211).
    #[error("division by zero")]
    DivideByZero,

    /// A declaration the machine does not support.
    #[error("precision {digits},{scale} is not valid: {why}")]
    Precision {
        /// Digits asked for.
        digits: u8,
        /// Decimal positions asked for.
        scale: u8,
        /// What is wrong with them.
        why: &'static str,
    },

    /// Text that is not a number.
    #[error("{text:?} is not a decimal number")]
    Parse {
        /// The text.
        text: String,
    },

    /// An edit code or edit word that cannot be applied.
    #[error("edit: {what}")]
    Edit {
        /// What is wrong.
        what: String,
    },

    /// A float that has no decimal value (NaN, infinity).
    #[error("float {what} has no decimal value")]
    NotFinite {
        /// NaN or infinity.
        what: &'static str,
    },
}

impl DecimalError {
    /// The message ID a later phase sends for this error.
    #[must_use]
    pub fn msgid(&self) -> Option<&'static str> {
        Some(match self {
            Self::DecimalData { .. } => "MCH1202",
            Self::SizeOverflow { .. } => "MCH1210",
            Self::DivideByZero => "MCH1211",
            Self::Precision { .. }
            | Self::Parse { .. }
            | Self::NotFinite { .. }
            | Self::Edit { .. } => return None,
        })
    }
}
