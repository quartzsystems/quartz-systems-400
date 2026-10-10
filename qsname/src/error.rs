// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Why a name was rejected.

/// A name that breaks the rules. Surfaces as CPF0001 in later phases.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    /// Nothing between the quotes, or an empty string.
    #[error("name is empty")]
    Empty,
    /// More than 10 characters, or more than 8 inside quotes.
    #[error("name {name:?} is too long")]
    TooLong {
        /// The name as given.
        name: String,
    },
    /// A character the rules do not allow at that position.
    #[error("name {name:?} has an invalid character {ch:?}")]
    BadChar {
        /// The name as given.
        name: String,
        /// The offending character.
        ch: char,
    },
    /// A quoted name without its closing quote.
    #[error("name {name:?} is not closed")]
    Unclosed {
        /// The name as given.
        name: String,
    },
    /// A special value where a name was required, or an unknown one.
    #[error("{value:?} is not allowed here")]
    BadSpecial {
        /// The value as given.
        value: String,
    },
}
