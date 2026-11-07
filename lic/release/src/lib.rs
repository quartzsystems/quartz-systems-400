// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Release identity. One source of truth for every crate that prints a
//! version.
//!
//! `VxRyMz`: V changes for a new generation, R for a release that adds
//! function (and may change on-disk formats), M for maintenance only.
//! V1R0M0 was the development release; V1R1M0 is the first release meant for use
//! outside development. Pre-release builds carry the stream plus a build
//! ID: `V1R1M0 build a1b2c3d[-dirty]`. No alpha, beta or rc words.

#![forbid(unsafe_code)]

mod ptf;
pub mod upgrade;
mod version;

pub use ptf::{Cumulative, Ptf};
pub use upgrade::{InstallKind, UpgradeError, check_upgrade, previous, store_may_open};
pub use version::Release;

/// The current development stream. A test build may claim another
/// with `QS400_RELEASE` at build time (the upgrade path's test).
pub const RELEASE: Release = Release {
    v: num(env!("QS400_RELEASE_V")),
    r: num(env!("QS400_RELEASE_R")),
    m: num(env!("QS400_RELEASE_M")),
};

/// Every release Quartz has defined, ascending: what `*PRV` and the
/// upgrade rule look in.
pub const KNOWN: &[Release] = &[Release { v: 1, r: 0, m: 0 }, Release { v: 1, r: 1, m: 0 }];

/// A decimal number of at most three digits, at compile time.
const fn num(s: &str) -> u8 {
    let b = s.as_bytes();
    let mut i = 0;
    let mut n: u8 = 0;
    while i < b.len() {
        n = n * 10 + (b[i] - b'0');
        i += 1;
    }
    n
}

/// The operating system's product identifier (`DSPSFWRSC`, `DSPPTF`,
/// `QSZRTVPR`). IBM's `5770SS1` is accepted for it on input
/// ([`product_id`]).
pub const PRODUCT_ID: &str = "QS00SS1";

/// The development tools' product identifier (the ILE RPG compiler's
/// listing heading); IBM's `5770WDS` is accepted for it on input.
pub const DEV_TOOLS_ID: &str = "QS00WDS";

/// The Licensed Internal Code's resource ID (`DSPSFWRSC`); IBM's
/// `5770999` is accepted for it on input.
pub const LIC_ID: &str = "QS00999";

/// IBM's product IDs and the Quartz products that answer to them, so
/// that software asking for IBM's product finds ours. Input only:
/// what the system shows is always the Quartz ID. The licensed program
/// catalog's `ibm_alias` entries agree with this table (a test there
/// checks).
pub const IBM_ALIASES: &[(&str, &str)] = &[
    ("5770999", LIC_ID),
    ("5770SS1", PRODUCT_ID),
    ("5770WDS", DEV_TOOLS_ID),
    ("5770TC1", "QS00TC1"),
    ("5770ST1", "QS00ST1"),
    ("5770DG1", "QS00DG1"),
    ("5770PT1", "QS00PT1"),
    ("5770XH2", "QS00AFW"),
];

/// The Quartz product ID for `id`: an IBM alias becomes the product
/// answering to it; anything else is returned as it is. Case and
/// blanks are the caller's.
#[must_use]
pub fn product_id(id: &str) -> &str {
    IBM_ALIASES
        .iter()
        .find(|(ibm, _)| *ibm == id)
        .map_or(id, |(_, ours)| ours)
}

/// The product's description.
pub const PRODUCT_TEXT: &str = "Quartz Systems/400 Operating System";

/// A licensed program option installed beside the operating system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Product {
    /// The product identifier.
    pub id: &'static str,
    /// `*BASE` or the option number.
    pub option: &'static str,
    /// Our description.
    pub text: &'static str,
    /// The library its objects are in.
    pub lib: &'static str,
}

/// The licensed programs past the operating system, at [`RELEASE`]:
/// the ILE RPG compiler is option 31 of `QS00WDS`, where IBM's ILE RPG
/// stands in `5770WDS`.
pub const PRODUCTS: &[Product] = &[
    Product {
        id: DEV_TOOLS_ID,
        option: "*BASE",
        text: "Quartz Systems/400 Application Development",
        lib: "QSYS",
    },
    Product {
        id: DEV_TOOLS_ID,
        option: "31",
        text: "Quartz Systems/400 ILE RPG Compiler",
        lib: "QSYS",
    },
];

/// The build ID captured at compile time: `git describe --always --dirty`,
/// or `QS400_BUILD_ID` from the environment, else `unknown`.
pub fn build_id() -> &'static str {
    env!("QS400_BUILD_ID")
}

/// `V1R1M0 build a1b2c3d`.
pub fn full() -> String {
    RELEASE.full()
}

/// `QS/400 V1R1M0 build a1b2c3d`, the line slic prints at IPL.
pub fn banner() -> String {
    format!("QS/400 {}", full())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_and_banner() {
        let r = RELEASE.to_string();
        assert!(KNOWN.contains(&RELEASE), "{r}");
        assert!(full().starts_with(&format!("{r} build ")));
        assert!(banner().starts_with(&format!("QS/400 {r} build ")));
        assert!(!build_id().is_empty());
        for word in ["alpha", "beta", "rc"] {
            assert!(!full().to_lowercase().contains(word));
        }
    }

    #[test]
    fn ibm_ids_are_aliases_on_input() {
        assert_eq!(product_id("5770SS1"), PRODUCT_ID);
        assert_eq!(product_id("5770WDS"), DEV_TOOLS_ID);
        assert_eq!(product_id("5770999"), LIC_ID);
        assert_eq!(product_id("QS00SS1"), "QS00SS1");
        assert_eq!(product_id("1ABCDEF"), "1ABCDEF");
        for (ibm, ours) in IBM_ALIASES {
            assert!(ibm.starts_with("5770") && ours.starts_with("QS00"));
        }
        assert!(PRODUCTS.iter().all(|p| p.id.starts_with("QS00")));
    }
}
