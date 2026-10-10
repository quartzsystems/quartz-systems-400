// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Release identity. One source of truth for every crate that prints a
//! version.
//!
//! `VxRyMz`: V changes for a new generation, R for a release that adds
//! function (and may change on-disk formats), M for maintenance only.
//! R0 is the development release; V1R1M0 is the first release meant for use
//! outside development. Pre-release builds carry the stream plus a build
//! ID: `V1R0M0 build a1b2c3d[-dirty]`. No alpha, beta or rc words.

#![forbid(unsafe_code)]

mod ptf;
mod version;

pub use ptf::{Cumulative, Ptf};
pub use version::Release;

/// The current development stream.
pub const RELEASE: Release = Release { v: 1, r: 0, m: 0 };

/// The build ID captured at compile time: `git describe --always --dirty`,
/// or `QS400_BUILD_ID` from the environment, else `unknown`.
pub fn build_id() -> &'static str {
    env!("QS400_BUILD_ID")
}

/// `V1R0M0 build a1b2c3d`.
pub fn full() -> String {
    RELEASE.full()
}

/// `QS/400 V1R0M0 build a1b2c3d`, the line slic prints at IPL.
pub fn banner() -> String {
    format!("QS/400 {}", full())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_and_banner() {
        assert_eq!(RELEASE.to_string(), "V1R0M0");
        assert!(full().starts_with("V1R0M0 build "));
        assert!(banner().starts_with("QS/400 V1R0M0 build "));
        assert!(!build_id().is_empty());
        for word in ["alpha", "beta", "rc"] {
            assert!(!full().to_lowercase().contains(word));
        }
    }
}
