// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Who may change a system value. Security values need `*ALLOBJ` and
//! `*SECADM`; the auditing values `*AUDIT`; and DST's "allow system
//! value security changes" must be on for either. The rest need
//! `*ALLOBJ` until `CHGSYSVAL` is a command with its own authority
//! (`MIGRATE(phase7)`).

use crate::auth::principal::Principal;
use crate::profile::spcaut::SpecialAuth;
use crate::{Result, SecError};

/// The auditing values.
pub const AUDIT: &[&str] = &[
    "QAUDCTL",
    "QAUDENDACN",
    "QAUDFRCLVL",
    "QAUDLVL",
    "QAUDLVL2",
    "QCRTOBJAUD",
];

/// Which rule a value falls under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An auditing value.
    Audit,
    /// Another security value (catalog group `SEC`).
    Security,
    /// Any other.
    Other,
}

/// The rule for `name`; `security` says whether the catalog puts it in
/// the `SEC` group.
#[must_use]
pub fn kind(name: &str, security: bool) -> Kind {
    if AUDIT.contains(&name) {
        Kind::Audit
    } else if security {
        Kind::Security
    } else {
        Kind::Other
    }
}

/// Refuse the change unless `p` may make it; `None` is the system.
pub fn require_change(
    p: Option<&Principal>,
    name: &str,
    security: bool,
    dst_allows: bool,
) -> Result<()> {
    let k = kind(name, security);
    if k != Kind::Other && !dst_allows {
        return Err(SecError::msg(
            "CPF18B4",
            format!(
                "{name} cannot change: security system values are locked in dedicated service tools."
            ),
        ));
    }
    let Some(p) = p else {
        return Ok(());
    };
    match k {
        Kind::Audit => p.require(SpecialAuth::AUDIT, &format!("change {name}")),
        Kind::Security => p.require(
            SpecialAuth::ALLOBJ.union(SpecialAuth::SECADM),
            &format!("change {name}"),
        ),
        Kind::Other => p.require(SpecialAuth::ALLOBJ, &format!("change {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        let mut p = Principal {
            user: "SEC".into(),
            spcaut: SpecialAuth::SECADM.0,
            qsecurity: 40,
            ..Principal::default()
        };
        assert!(require_change(Some(&p), "QPWDMINLEN", true, true).is_err());
        p.spcaut |= SpecialAuth::ALLOBJ.0;
        assert!(require_change(Some(&p), "QPWDMINLEN", true, true).is_ok());
        assert!(require_change(Some(&p), "QAUDCTL", true, true).is_err());
        p.spcaut |= SpecialAuth::AUDIT.0;
        assert!(require_change(Some(&p), "QAUDCTL", true, true).is_ok());
        let e = require_change(Some(&p), "QSECURITY", true, false).unwrap_err();
        assert_eq!(e.msgid(), Some("CPF18B4"));
        assert!(require_change(None, "QDATE", false, false).is_ok());
    }
}
