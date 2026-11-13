// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What can go wrong coding XDR.

/// An XDR encoding or decoding error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The data ends before the item does.
    #[error("the data ends: {need} bytes needed, {left} left")]
    Short {
        /// Bytes the item needs.
        need: u64,
        /// Bytes left.
        left: usize,
    },
    /// A length is over the description's bound.
    #[error("a length of {len} is over the bound {max}")]
    TooLong {
        /// The length found or given.
        len: u64,
        /// The bound.
        max: u32,
    },
    /// A fixed array not of its declared length (on encode).
    #[error("{len} items where {want} are declared")]
    FixedLength {
        /// The items given.
        len: usize,
        /// The declared count.
        want: u32,
    },
    /// Padding bytes that are not zero.
    #[error("padding bytes not zero")]
    Padding,
    /// A bool that is neither 0 nor 1.
    #[error("{0} is not a bool")]
    Bool(u32),
    /// An enum value or union discriminant the description does not have.
    #[error("{ty}: no member or arm for {value}")]
    Discriminant {
        /// The type.
        ty: &'static str,
        /// The value found.
        value: i64,
    },
    /// A union's default arm holding a value that has its own arm (on encode).
    #[error("{ty}: the default arm holds a value that has its own arm")]
    DefaultArm {
        /// The type.
        ty: &'static str,
    },
    /// Items nested deeper than the decoder allows.
    #[error("nested deeper than {0}")]
    TooDeep(u32),
    /// Bytes left after the item.
    #[error("{0} bytes left over")]
    Trailing(usize),
}

/// A result of XDR coding.
pub type Result<T> = std::result::Result<T, Error>;
