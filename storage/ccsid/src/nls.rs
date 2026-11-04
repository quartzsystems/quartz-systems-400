// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! National language versions: the feature code of each language
//! library (`QSYS2924`), its language and country identifiers and the
//! CCSID a job with that language and CCSID 65535 works in (its
//! default CCSID).

/// One national language version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Language {
    /// The feature code, `2924`; the library is `QSYS` followed by it.
    pub feature: u16,
    /// `LANGID`.
    pub langid: &'static str,
    /// `CNTRYID`.
    pub cntryid: &'static str,
    /// The CCSID of its text.
    pub ccsid: u16,
}

impl Language {
    /// Its library, `QSYS2931`.
    #[must_use]
    pub fn library(&self) -> String {
        format!("QSYS{}", self.feature)
    }
}

/// The versions QS/400 knows, the primary (English) first.
pub const LANGUAGES: &[Language] = &[
    Language { feature: 2924, langid: "ENU", cntryid: "US", ccsid: 37 },
    Language { feature: 2928, langid: "FRA", cntryid: "FR", ccsid: 297 },
    Language { feature: 2929, langid: "DEU", cntryid: "DE", ccsid: 273 },
    Language { feature: 2931, langid: "ESP", cntryid: "ES", ccsid: 284 },
    Language { feature: 2932, langid: "ITA", cntryid: "IT", ccsid: 280 },
];

/// The primary language's feature code.
pub const PRIMARY: u16 = 2924;

/// The version of a feature code.
#[must_use]
pub fn by_feature(feature: u16) -> Option<&'static Language> {
    LANGUAGES.iter().find(|l| l.feature == feature)
}

/// The version of a language identifier.
#[must_use]
pub fn by_langid(langid: &str) -> Option<&'static Language> {
    let l = langid.trim();
    LANGUAGES.iter().find(|x| x.langid.eq_ignore_ascii_case(l))
}

/// The feature code of a language library name (`QSYS2931`).
#[must_use]
pub fn feature_of_library(lib: &str) -> Option<u16> {
    let n = lib.trim().strip_prefix("QSYS")?;
    if n.len() != 4 {
        return None;
    }
    n.parse().ok().filter(|f| (2900..3000).contains(f))
}

/// The default CCSID of a job: its CCSID, or with 65535 its language's.
#[must_use]
pub fn default_ccsid(ccsid: u32, langid: &str) -> u32 {
    if ccsid != u32::from(crate::conv::BINARY) {
        return ccsid;
    }
    by_langid(langid).map_or(37, |l| u32::from(l.ccsid))
}

/// Whether a job may run in `ccsid`: 65535, or a single-byte EBCDIC
/// CCSID with a table.
#[must_use]
pub fn job_ccsid_ok(ccsid: u32) -> bool {
    let Ok(c) = u16::try_from(ccsid) else {
        return false;
    };
    c == crate::conv::BINARY
        || matches!(
            crate::conv::Codec::of(c),
            Ok(Some(crate::conv::Codec { form: crate::conv::Form::Sbcs { ebcdic: true, .. }, .. }))
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_and_their_ccsids() {
        assert_eq!(by_feature(2931).unwrap().library(), "QSYS2931");
        assert_eq!(by_langid("esp").unwrap().ccsid, 284);
        assert_eq!(feature_of_library("QSYS2931"), Some(2931));
        assert_eq!(feature_of_library("QSYS2"), None);
        assert_eq!(default_ccsid(65535, "ESP"), 284);
        assert_eq!(default_ccsid(65535, "XXX"), 37);
        assert_eq!(default_ccsid(273, "ESP"), 273);
        for c in [37, 273, 284, 500, 1141, 65535] {
            assert!(job_ccsid_ok(c), "{c}");
        }
        for c in [1208, 819, 930, 300, 1234, 70000] {
            assert!(!job_ccsid_ok(c), "{c}");
        }
        for l in LANGUAGES {
            assert!(job_ccsid_ok(u32::from(l.ccsid)));
        }
    }
}
