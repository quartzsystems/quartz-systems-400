// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `VxRyMz` release identity, for any product.
//!
//! - [`Release`]: the three numbers, parsed and printed as `VxRyMz`.
//! - [`build`]: what a product's `build.rs` calls to capture its release
//!   (from its own `VERSION` file, or an override in its environment)
//!   and its build ID (`git describe` in its own repository).
//! - [`release_from_env!`](crate::release_from_env): the [`Release`]
//!   that build script captured, as a constant.
//!
//! The product names its repository root and its environment prefix;
//! nothing here looks for either.

#![forbid(unsafe_code)]

pub mod build;
mod version;

pub use version::{ParseReleaseError, Release};

/// A decimal number of at most three digits, at compile time.
#[must_use]
pub const fn num(s: &str) -> u8 {
    let b = s.as_bytes();
    let mut i = 0;
    let mut n: u8 = 0;
    while i < b.len() {
        n = n * 10 + (b[i] - b'0');
        i += 1;
    }
    n
}

/// The [`Release`] a build script captured with
/// [`build::capture`] under the prefix given (a literal, `"QS400_"`).
#[macro_export]
macro_rules! release_from_env {
    ($prefix:literal) => {
        $crate::Release {
            v: $crate::num(env!(concat!($prefix, "RELEASE_V"))),
            r: $crate::num(env!(concat!($prefix, "RELEASE_R"))),
            m: $crate::num(env!(concat!($prefix, "RELEASE_M"))),
        }
    };
}
