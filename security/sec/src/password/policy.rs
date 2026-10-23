// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Setting a password and when it expires: `CHGPWD` by the user (the
//! current password, `PWDCHGBLK`, the composition rules, the history,
//! `QPWDVLDPGM`) and `CHGUSRPRF PASSWORD` by an administrator.

use crate::password::rules::{Rules, history_len};
use crate::password::{form_for, hashes_for, verify};
use crate::profile::payload::{NOMAX, SYSVAL, UsrPrf};
use crate::{Result, SecError};

/// Seconds in a day.
pub const DAY: u64 = 86_400;

/// The password system values in force.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// `QPWDLVL`.
    pub pwdlvl: u8,
    /// Composition.
    pub rules: Rules,
    /// `QPWDRQDDIF`.
    pub rqddif: u8,
    /// `QPWDEXPITV` days, `None` for `*NOMAX`.
    pub expitv: Option<u32>,
    /// `QPWDEXPWRN` days.
    pub expwrn: u32,
    /// `QPWDCHGBLK` hours, `None` for `*NONE`.
    pub chgblk: Option<u32>,
    /// `QPWDVLDPGM`: `*NONE`, `*REGFAC` or `lib/pgm`.
    pub vldpgm: String,
}

impl Policy {
    /// From the system values; `get` answers a value's text.
    pub fn from_sysvals(get: &dyn Fn(&str) -> String) -> Self {
        let num = |n: &str| get(n).trim().parse::<u32>().ok();
        let vld = get("QPWDVLDPGM");
        let mut w = vld.split_whitespace();
        let vldpgm = match (w.next(), w.next()) {
            (Some(p), Some(l)) => format!("{l}/{p}"),
            (Some(p), None) => p.to_string(),
            _ => "*NONE".into(),
        };
        Self {
            pwdlvl: num("QPWDLVL")
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(0),
            rules: Rules::from_sysvals(get),
            rqddif: num("QPWDRQDDIF")
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(0),
            expitv: num("QPWDEXPITV"),
            expwrn: num("QPWDEXPWRN").unwrap_or(7),
            chgblk: num("QPWDCHGBLK"),
            vldpgm,
        }
    }

    /// The profile's expiry interval in days, `None` for none.
    #[must_use]
    pub fn interval(&self, p: &UsrPrf) -> Option<u32> {
        match p.pwdexpitv {
            SYSVAL => self.expitv,
            NOMAX => None,
            n => Some(n),
        }
    }

    /// Days until the password expires (0 or less: expired), `None`
    /// when it never does by age.
    #[must_use]
    pub fn days_left(&self, p: &UsrPrf, now: u64) -> Option<i64> {
        let itv = self.interval(p)?;
        let age = i64::try_from(now.saturating_sub(p.pwd_changed) / DAY).unwrap_or(i64::MAX);
        Some(i64::from(itv) - age)
    }

    /// Whether the password must be changed before the job starts.
    #[must_use]
    pub fn expired(&self, p: &UsrPrf, now: u64) -> bool {
        p.pwdexp || self.days_left(p, now).is_some_and(|d| d <= 0)
    }

    /// Whether to warn at sign-on: within `QPWDEXPWRN` days.
    #[must_use]
    pub fn warn(&self, p: &UsrPrf, now: u64) -> Option<i64> {
        self.days_left(p, now)
            .filter(|d| *d > 0 && *d <= i64::from(self.expwrn))
    }
}

/// Who sets the password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By<'a> {
    /// The user, with the current password typed (`CHGPWD`, `QSYCHGPW`).
    User {
        /// The current password.
        current: &'a str,
    },
    /// An administrator (`CRTUSRPRF`, `CHGUSRPRF PASSWORD`).
    Admin,
}

/// A validation program's answer: `Ok` or the message it chose.
pub type Validator<'v> = dyn FnMut(&UsrPrf, &str, Option<&str>) -> Result<()> + 'v;

/// Set `new` as `p`'s password, or say why not. `validate` runs
/// `QPWDVLDPGM` when it names a program.
pub fn set(
    p: &mut UsrPrf,
    new: &str,
    by: By<'_>,
    policy: &Policy,
    now: u64,
    validate: &mut Validator<'_>,
) -> Result<()> {
    let user = matches!(by, By::User { .. });
    if let By::User { current } = by {
        let form = form_for(policy.pwdlvl);
        if p.pwd_none || !p.hash(form).is_some_and(|h| verify(h, current)) {
            return Err(SecError::msg(
                "CPF2362",
                "The current password is not correct.",
            ));
        }
        if let Some(h) = policy.chgblk.filter(|h| *h > 0)
            && now < p.pwd_changed + u64::from(h) * 3600
            && !p.pwdexp
        {
            return Err(SecError::msg(
                "CPF22C5",
                format!("The password was changed less than {h} hours ago and cannot change yet."),
            ));
        }
        if new == current {
            return Err(SecError::msg(
                "CPF22C3",
                "The new password must differ from the current one.",
            ));
        }
    }
    let old = match by {
        By::User { current } => Some(current),
        By::Admin => None,
    };
    if user || policy.rules.all_crtchg {
        policy
            .rules
            .check(new, old, &p.name, policy.pwdlvl)
            .map_err(|v| SecError::msg("CPF22C2", format!("{} (reason {})", v.text(), v.reason)))?;
    } else if policy.pwdlvl <= 1 && !super::short_ok(new) {
        return Err(SecError::msg(
            "CPF22C2",
            "The password is not valid at this password level. (reason 24)",
        ));
    }
    let keep = history_len(policy.rqddif);
    if user && keep > 0 {
        let form = form_for(policy.pwdlvl);
        let reused = p
            .hashes
            .iter()
            .chain(p.history.iter().take(keep * 2))
            .filter(|h| h.form == form)
            .take(keep)
            .any(|h| verify(h, new));
        if reused {
            return Err(SecError::msg(
                "CPF22C3",
                "The new password was used too recently.",
            ));
        }
    }
    if user && !policy.vldpgm.starts_with('*') {
        validate(p, new, old)?;
    }
    if !p.pwd_none {
        let mut hist = std::mem::take(&mut p.hashes);
        hist.extend(std::mem::take(&mut p.history));
        hist.retain(|h| !h.form.is_token());
        hist.truncate(64);
        p.history = hist;
    }
    p.hashes = hashes_for(new, policy.pwdlvl);
    p.pwd_none = false;
    if user {
        p.pwdexp = false;
    }
    p.pwd_changed = now;
    Ok(())
}

/// `PASSWORD(*NONE)`.
pub fn set_none(p: &mut UsrPrf) {
    if !p.pwd_none {
        let mut hist = std::mem::take(&mut p.hashes);
        hist.extend(std::mem::take(&mut p.history));
        hist.truncate(64);
        p.history = hist;
    }
    p.pwd_none = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(rqddif: u8) -> Policy {
        let vals = [
            ("QPWDLVL", "3".to_string()),
            ("QPWDRULES", "*PWDSYSVAL".into()),
            ("QPWDMINLEN", "6".into()),
            ("QPWDMAXLEN", "128".into()),
            ("QPWDRQDDGT", "1".into()),
            ("QPWDRQDDIF", rqddif.to_string()),
            ("QPWDEXPITV", "90".into()),
            ("QPWDCHGBLK", "*NONE".into()),
            ("QPWDVLDPGM", "*NONE".into()),
        ];
        Policy::from_sysvals(&|n: &str| {
            vals.iter()
                .find(|(k, _)| *k == n)
                .map_or_else(String::new, |(_, v)| v.clone())
        })
    }

    #[test]
    fn change_history_and_expiry() {
        let pol = policy(8);
        let mut p = UsrPrf::new("BOB", 40);
        let mut ok = |_: &UsrPrf, _: &str, _: Option<&str>| Ok(());
        set(&mut p, "first1", By::Admin, &pol, 0, &mut ok).unwrap();
        let e = set(
            &mut p,
            "second2",
            By::User { current: "wrong" },
            &pol,
            10,
            &mut ok,
        )
        .unwrap_err();
        assert_eq!(e.msgid(), Some("CPF2362"));
        let e = set(
            &mut p,
            "nodigit",
            By::User { current: "first1" },
            &pol,
            10,
            &mut ok,
        )
        .unwrap_err();
        assert_eq!(e.msgid(), Some("CPF22C2"));
        set(
            &mut p,
            "second2",
            By::User { current: "first1" },
            &pol,
            10,
            &mut ok,
        )
        .unwrap();
        let e = set(
            &mut p,
            "first1",
            By::User { current: "second2" },
            &pol,
            20,
            &mut ok,
        )
        .unwrap_err();
        assert_eq!(e.msgid(), Some("CPF22C3"));
        assert!(!pol.expired(&p, 10));
        assert!(pol.expired(&p, 10 + 90 * DAY));
        assert_eq!(pol.warn(&p, 10 + 85 * DAY), Some(5));
        let mut deny = |_: &UsrPrf, _: &str, _: Option<&str>| Err(SecError::msg("CPF22C6", "no"));
        let strict = Policy {
            vldpgm: "QGPL/CHECKPW".into(),
            ..pol
        };
        let e = set(
            &mut p,
            "third3",
            By::User { current: "second2" },
            &strict,
            30,
            &mut deny,
        )
        .unwrap_err();
        assert_eq!(e.msgid(), Some("CPF22C6"));
    }
}
