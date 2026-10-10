// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Error type.

use thiserror::Error;

/// Anything system value handling can fail with.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SysValError {
    /// The catalog did not parse or is inconsistent.
    #[error("system value catalog: {0}")]
    Catalog(String),
    /// No such system value.
    #[error("system value {0} does not exist")]
    Unknown(String),
    /// The value is set by the system and cannot be changed by a user.
    #[error("system value {0} is set by the system and cannot be changed")]
    ReadOnly(String),
    /// Wrong type for the value.
    #[error("system value {name} takes a {expected} value")]
    WrongType {
        /// Value name.
        name: String,
        /// `char`, `dec` or `list`.
        expected: &'static str,
    },
    /// Too long.
    #[error("system value {name} allows at most {max} characters, got {got}")]
    TooLong {
        /// Value name.
        name: String,
        /// Limit.
        max: usize,
        /// Actual length.
        got: usize,
    },
    /// Too many list elements.
    #[error("system value {name} allows at most {max} elements, got {got}")]
    TooMany {
        /// Value name.
        name: String,
        /// Limit.
        max: usize,
        /// Actual count.
        got: usize,
    },
    /// Not one of the allowed values.
    #[error("system value {name} does not allow '{got}'; allowed: {allowed}")]
    NotAllowed {
        /// Value name.
        name: String,
        /// Rejected value.
        got: String,
        /// Allowed values, comma separated.
        allowed: String,
    },
    /// Outside the numeric range.
    #[error("system value {name} must be between {min} and {max}, got {got}")]
    OutOfRange {
        /// Value name.
        name: String,
        /// Minimum.
        min: i64,
        /// Maximum.
        max: i64,
        /// Rejected value.
        got: i64,
    },
    /// Not a number where one was needed.
    #[error("system value {name} needs a number, got '{got}'")]
    NotANumber {
        /// Value name.
        name: String,
        /// Rejected text.
        got: String,
    },
    /// The stored TLV could not be decoded.
    #[error("stored system values are malformed: {0}")]
    Storage(&'static str),
    /// Writing the LIC partition failed.
    #[error("LIC partition: {0}")]
    Lic(String),
}
