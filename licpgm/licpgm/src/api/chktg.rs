// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSZCHKTG (Check Target Release): validate a `TGTRLS` value and find
//! the newest supported release no newer than it.

use release::Release;

use crate::api::ApiError;
use crate::id;
use crate::system::SystemInfo;

/// What QSZCHKTG returns: the validated target release and the
/// equivalent supported release, both `VxRyMz`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedTarget {
    /// The target, `*CURRENT` and `*PRV` resolved.
    pub validated: String,
    /// The newest supported release not newer than the target.
    pub equivalent: String,
}

/// The release `*PRV` names: the newest known release with
/// modification 0 below the current version and release.
#[must_use]
pub fn previous(sys: &SystemInfo) -> Option<Release> {
    let cur = sys.release;
    sys.known_releases
        .iter()
        .copied()
        .filter(|r| r.m == 0 && (r.v, r.r) < (cur.v, cur.r))
        .max()
}

/// QSZCHKTG: target release (10), supported releases (6 each), their
/// count. `CPF0C34` for a target that is not a known release, `CPF0C35`
/// for one older than every supported release, `CPF0C36` for a bad
/// supported list.
pub fn qszchktg(
    sys: &SystemInfo,
    target: &str,
    supported: &[String],
) -> Result<CheckedTarget, ApiError> {
    if supported.is_empty() {
        return Err(ApiError::new("CPF0C36"));
    }
    let sav = supported.iter().any(|s| s == "*SAV");
    if sav && supported.len() != 1 {
        return Err(ApiError::new("CPF0C36"));
    }
    let validated = match target {
        "*CURRENT" => sys.release,
        "*PRV" => previous(sys).ok_or_else(|| ApiError::new("CPF0C34").text(target, 10))?,
        t => {
            let r = id::parse_release(t).map_err(|_| ApiError::new("CPF0C34").text(t, 10))?;
            if !sys.known_releases.contains(&r) {
                return Err(ApiError::new("CPF0C34").text(t, 10));
            }
            r
        }
    };
    let vt = id::release_text(validated);
    if sav {
        return Ok(CheckedTarget {
            validated: vt.clone(),
            equivalent: vt,
        });
    }
    let mut list = Vec::new();
    for s in supported {
        let r = id::parse_release(s).map_err(|_| ApiError::new("CPF0C36").text(s, 6))?;
        if list.last().is_some_and(|p| *p >= r) {
            return Err(ApiError::new("CPF0C36").text(s, 6));
        }
        list.push(r);
    }
    let eq = list
        .iter()
        .copied()
        .filter(|r| *r <= validated)
        .max()
        .ok_or_else(|| ApiError::new("CPF0C35").text(&vt, 6))?;
    Ok(CheckedTarget {
        validated: vt,
        equivalent: id::release_text(eq),
    })
}
