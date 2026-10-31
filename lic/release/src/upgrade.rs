// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The upgrade rule: which installed release a release on media may be
//! installed over (docs/INSTALL.md, "Upgrade rules").
//!
//! - The same release is a slip install: the objects are replaced, the
//!   release stays.
//! - A newer modification of the same version and release is always
//!   allowed: maintenance never changes formats.
//! - A newer version or release is allowed from its `*PRV` release (the
//!   newest modification 0 release below it in [`crate::KNOWN`]) or
//!   anything newer than that; older systems go through `*PRV` first.
//! - An older release is a downgrade and is refused.

use thiserror::Error;

use crate::{KNOWN, Release};

/// What installing `media` over `installed` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    /// The same release again: objects replaced, release unchanged.
    Slip,
    /// A newer release or modification.
    Upgrade {
        /// The release on the system.
        from: Release,
        /// The release on the media.
        to: Release,
    },
}

/// Why `media` may not go over `installed`.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum UpgradeError {
    /// The media is older than the system.
    #[error("the media holds {media}, older than the installed {installed}; downgrade is not allowed")]
    Downgrade {
        /// On the system.
        installed: Release,
        /// On the media.
        media: Release,
    },
    /// The system is older than the media's `*PRV`.
    #[error("{media} installs over {oldest} or later; the system has {installed}")]
    TooOld {
        /// On the system.
        installed: Release,
        /// On the media.
        media: Release,
        /// The oldest release it goes over.
        oldest: Release,
    },
}

/// The release `*PRV` names for `r`: the newest known release with
/// modification 0 below its version and release.
#[must_use]
pub fn previous(r: Release) -> Option<Release> {
    KNOWN
        .iter()
        .copied()
        .filter(|k| k.m == 0 && (k.v, k.r) < (r.v, r.r))
        .max()
}

/// The rule for installing `media` over `installed`.
pub fn check_upgrade(installed: Release, media: Release) -> Result<InstallKind, UpgradeError> {
    if media == installed {
        return Ok(InstallKind::Slip);
    }
    if media < installed {
        return Err(UpgradeError::Downgrade { installed, media });
    }
    let up = Ok(InstallKind::Upgrade {
        from: installed,
        to: media,
    });
    if (media.v, media.r) == (installed.v, installed.r) {
        return up;
    }
    let oldest = previous(media).unwrap_or(media);
    if (installed.v, installed.r) >= (oldest.v, oldest.r) {
        up
    } else {
        Err(UpgradeError::TooOld {
            installed,
            media,
            oldest,
        })
    }
}

/// Whether this release may open an ASP created by `store`: the same
/// version and release, or an older one it may upgrade (the install
/// then records the new release).
#[must_use]
pub fn store_may_open(store: Release) -> bool {
    crate::RELEASE.is_compatible_store(store)
        || matches!(
            check_upgrade(store, crate::RELEASE),
            Ok(InstallKind::Upgrade { .. })
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Release {
        s.parse().unwrap()
    }

    #[test]
    fn prv_is_the_newest_m0_below() {
        assert_eq!(previous(p("V1R1M0")), Some(p("V1R0M0")));
        assert_eq!(previous(p("V1R1M3")), Some(p("V1R0M0")));
        assert_eq!(previous(p("V1R0M0")), None);
    }

    #[test]
    fn the_rule() {
        assert_eq!(check_upgrade(p("V1R0M0"), p("V1R0M0")), Ok(InstallKind::Slip));
        assert_eq!(
            check_upgrade(p("V1R0M0"), p("V1R1M0")),
            Ok(InstallKind::Upgrade {
                from: p("V1R0M0"),
                to: p("V1R1M0")
            })
        );
        assert!(matches!(
            check_upgrade(p("V1R1M0"), p("V1R1M2")),
            Ok(InstallKind::Upgrade { .. })
        ));
        assert!(matches!(
            check_upgrade(p("V1R1M0"), p("V1R0M0")),
            Err(UpgradeError::Downgrade { .. })
        ));
        assert!(matches!(
            check_upgrade(p("V1R1M2"), p("V1R1M0")),
            Err(UpgradeError::Downgrade { .. })
        ));
        // Two releases on: only from *PRV.
        let e = check_upgrade(p("V0R9M0"), p("V1R1M0")).unwrap_err();
        assert_eq!(
            e,
            UpgradeError::TooOld {
                installed: p("V0R9M0"),
                media: p("V1R1M0"),
                oldest: p("V1R0M0")
            }
        );
        assert!(e.to_string().contains("V1R0M0 or later"));
    }

    #[test]
    fn an_older_store_opens_only_to_upgrade() {
        assert!(store_may_open(crate::RELEASE));
        assert!(!store_may_open(p("V9R0M0")));
    }
}
