// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Password policy (docs/DIRSRV.md §11): IBM's global, group and
//! individual policies (draft-behera-ldap-password-policy as IBM applies
//! it), the state a user's entry keeps, and what a bind or a password
//! change does to it.

use ldap::control::{PolicyError, PolicyWarning};
use ldap::msg::{Attribute, ModOp, Modification};

use super::scheme;
use crate::ops::clock::{gentime, parse_gentime};

/// The global policy's DN.
pub const GLOBAL_DN: &str = "cn=pwdpolicy,cn=ibmpolicies";

const SEC: u64 = 1_000_000;

/// One policy entry's settings; `None` for an attribute it does not set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Partial {
    /// `ibm-pwdPolicy`: the policy is in force.
    pub enabled: Option<bool>,
    /// `pwdMinAge`, seconds.
    pub min_age: Option<u64>,
    /// `pwdMaxAge`, seconds (0: never expires).
    pub max_age: Option<u64>,
    /// `pwdInHistory`.
    pub in_history: Option<u64>,
    /// `pwdCheckSyntax`: 0 none, 1 when the value is clear, 2 always.
    pub check_syntax: Option<u64>,
    /// `pwdMinLength`.
    pub min_length: Option<u64>,
    /// `pwdExpireWarning`, seconds.
    pub expire_warning: Option<u64>,
    /// `pwdGraceLoginLimit`.
    pub grace_limit: Option<u64>,
    /// `pwdLockout`.
    pub lockout: Option<bool>,
    /// `pwdLockoutDuration`, seconds (0: until an administrator unlocks).
    pub lockout_duration: Option<u64>,
    /// `pwdMaxFailure`.
    pub max_failure: Option<u64>,
    /// `pwdFailureCountInterval`, seconds (0: failures never expire).
    pub failure_interval: Option<u64>,
    /// `pwdMustChange`.
    pub must_change: Option<bool>,
    /// `pwdAllowUserChange`.
    pub allow_user_change: Option<bool>,
    /// `pwdSafeModify`.
    pub safe_modify: Option<bool>,
    /// `passwordMinAlphaChars`.
    pub min_alpha: Option<u64>,
    /// `passwordMinOtherChars`.
    pub min_other: Option<u64>,
    /// `passwordMaxRepeatedChars` (0: no limit).
    pub max_repeated: Option<u64>,
    /// `passwordMinDiffChars`.
    pub min_diff: Option<u64>,
}

fn first<'a>(attrs: &'a [Attribute], name: &str) -> Option<&'a [u8]> {
    attrs
        .iter()
        .find(|a| a.ty.eq_ignore_ascii_case(name))
        .and_then(|a| a.vals.first())
        .map(Vec::as_slice)
}

fn num(attrs: &[Attribute], name: &str) -> Option<u64> {
    first(attrs, name).and_then(|v| std::str::from_utf8(v).ok()?.trim().parse().ok())
}

fn flag(attrs: &[Attribute], name: &str) -> Option<bool> {
    first(attrs, name).map(|v| v.eq_ignore_ascii_case(b"TRUE"))
}

impl Partial {
    /// A policy entry's settings.
    #[must_use]
    pub fn from_attrs(a: &[Attribute]) -> Self {
        Self {
            enabled: flag(a, "ibm-pwdPolicy"),
            min_age: num(a, "pwdMinAge"),
            max_age: num(a, "pwdMaxAge"),
            in_history: num(a, "pwdInHistory"),
            check_syntax: num(a, "pwdCheckSyntax"),
            min_length: num(a, "pwdMinLength"),
            expire_warning: num(a, "pwdExpireWarning"),
            grace_limit: num(a, "pwdGraceLoginLimit"),
            lockout: flag(a, "pwdLockout"),
            lockout_duration: num(a, "pwdLockoutDuration"),
            max_failure: num(a, "pwdMaxFailure"),
            failure_interval: num(a, "pwdFailureCountInterval"),
            must_change: flag(a, "pwdMustChange"),
            allow_user_change: flag(a, "pwdAllowUserChange"),
            safe_modify: flag(a, "pwdSafeModify"),
            min_alpha: num(a, "passwordMinAlphaChars"),
            min_other: num(a, "passwordMinOtherChars"),
            max_repeated: num(a, "passwordMaxRepeatedChars"),
            min_diff: num(a, "passwordMinDiffChars"),
        }
    }

    /// `self` with every setting `over` has taken from `over` (an
    /// individual policy over the rest).
    #[must_use]
    pub fn overlay(&self, over: &Self) -> Self {
        macro_rules! pick {
            ($($f:ident),*) => { Self { $($f: over.$f.or(self.$f)),* } };
        }
        pick!(
            enabled,
            min_age,
            max_age,
            in_history,
            check_syntax,
            min_length,
            expire_warning,
            grace_limit,
            lockout,
            lockout_duration,
            max_failure,
            failure_interval,
            must_change,
            allow_user_change,
            safe_modify,
            min_alpha,
            min_other,
            max_repeated,
            min_diff
        )
    }

    /// The most restrictive of two group policies, setting by setting.
    #[must_use]
    pub fn strictest(&self, o: &Self) -> Self {
        fn hi(a: Option<u64>, b: Option<u64>) -> Option<u64> {
            a.max(b)
        }
        // Smallest non-zero; zero means "no limit".
        fn lo(a: Option<u64>, b: Option<u64>) -> Option<u64> {
            match (a, b) {
                (Some(x), Some(y)) if x != 0 && y != 0 => Some(x.min(y)),
                (Some(0), Some(y)) | (Some(y), Some(0)) => Some(y),
                (x, None) => x,
                (None, y) => y,
                (Some(x), Some(_)) => Some(x),
            }
        }
        fn yes(a: Option<bool>, b: Option<bool>) -> Option<bool> {
            match (a, b) {
                (None, None) => None,
                _ => Some(a == Some(true) || b == Some(true)),
            }
        }
        fn no(a: Option<bool>, b: Option<bool>) -> Option<bool> {
            match (a, b) {
                (None, None) => None,
                _ => Some(a != Some(false) && b != Some(false)),
            }
        }
        Self {
            enabled: yes(self.enabled, o.enabled),
            min_age: hi(self.min_age, o.min_age),
            max_age: lo(self.max_age, o.max_age),
            in_history: hi(self.in_history, o.in_history),
            check_syntax: hi(self.check_syntax, o.check_syntax),
            min_length: hi(self.min_length, o.min_length),
            expire_warning: hi(self.expire_warning, o.expire_warning),
            grace_limit: self.grace_limit.min(o.grace_limit).or(self.grace_limit).or(o.grace_limit),
            lockout: yes(self.lockout, o.lockout),
            lockout_duration: hi(self.lockout_duration, o.lockout_duration),
            max_failure: lo(self.max_failure, o.max_failure),
            failure_interval: hi(self.failure_interval, o.failure_interval),
            must_change: yes(self.must_change, o.must_change),
            allow_user_change: no(self.allow_user_change, o.allow_user_change),
            safe_modify: yes(self.safe_modify, o.safe_modify),
            min_alpha: hi(self.min_alpha, o.min_alpha),
            min_other: hi(self.min_other, o.min_other),
            max_repeated: lo(self.max_repeated, o.max_repeated),
            min_diff: hi(self.min_diff, o.min_diff),
        }
    }
}

/// The policy in force for one user, every setting resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Whether it is in force at all.
    pub enabled: bool,
    /// The settings (none left `None` that matter; absent ones are 0 or
    /// the draft's defaults).
    pub p: Partial,
}

impl Policy {
    /// The effective policy: the global one, overlaid by the strictest of
    /// the user's group policies, overlaid by the individual one (group
    /// and individual policies only when the global entry enables them).
    #[must_use]
    pub fn effective(global: &Partial, groups: &[Partial], individual: Option<&Partial>) -> Self {
        let mut p = global.clone();
        if let Some(g) = groups.iter().cloned().reduce(|a, b| a.strictest(&b)) {
            p = p.overlay(&g);
        }
        if let Some(i) = individual {
            p = p.overlay(i);
        }
        Self {
            enabled: p.enabled.unwrap_or(false),
            p,
        }
    }

    fn n(v: Option<u64>) -> u64 {
        v.unwrap_or(0)
    }
    fn secs(v: Option<u64>) -> u64 {
        v.unwrap_or(0).saturating_mul(SEC)
    }
}

/// What a user's entry says of its password's history.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    /// `pwdChangedTime`, Unix microseconds.
    pub changed: Option<u64>,
    /// `pwdFailureTime` values.
    pub failures: Vec<u64>,
    /// `pwdAccountLockedTime`.
    pub locked: Option<u64>,
    /// `ibm-pwdAccountLocked`: locked by an administrator.
    pub admin_locked: bool,
    /// `pwdGraceUseTime` values.
    pub grace: Vec<u64>,
    /// `pwdReset`.
    pub reset: bool,
    /// `pwdHistory` values as stored.
    pub history: Vec<Vec<u8>>,
}

fn times(attrs: &[Attribute], name: &str) -> Vec<u64> {
    attrs
        .iter()
        .filter(|a| a.ty.eq_ignore_ascii_case(name))
        .flat_map(|a| a.vals.iter().filter_map(|v| parse_gentime(v)))
        .collect()
}

impl State {
    /// The state an entry's operational attributes hold.
    #[must_use]
    pub fn from_attrs(a: &[Attribute]) -> Self {
        Self {
            changed: times(a, "pwdChangedTime").first().copied(),
            failures: times(a, "pwdFailureTime"),
            locked: times(a, "pwdAccountLockedTime").first().copied(),
            admin_locked: flag(a, "ibm-pwdAccountLocked").unwrap_or(false),
            grace: times(a, "pwdGraceUseTime"),
            reset: flag(a, "pwdReset").unwrap_or(false),
            history: a
                .iter()
                .filter(|x| x.ty.eq_ignore_ascii_case("pwdHistory"))
                .flat_map(|x| x.vals.clone())
                .collect(),
        }
    }
}

fn replace(ty: &str, vals: Vec<Vec<u8>>) -> Modification {
    Modification {
        op: ModOp::Replace,
        attr: Attribute { ty: ty.into(), vals },
    }
}

fn clear(ty: &str) -> Modification {
    replace(ty, Vec::new())
}

/// What a bind attempt comes to under the policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindVerdict {
    /// The bind succeeds.
    pub allowed: bool,
    /// The response control's warning.
    pub warning: Option<PolicyWarning>,
    /// The response control's error.
    pub error: Option<PolicyError>,
    /// Changes the server makes to the user's entry (committed even when
    /// the bind fails).
    pub mods: Vec<Modification>,
    /// The session may only change its password (`pwdReset` with
    /// `pwdMustChange`).
    pub must_change: bool,
}

/// A bind as the user: `password_ok` is whether the password matched.
#[must_use]
pub fn on_bind(pol: &Policy, st: &State, now: u64, password_ok: bool) -> BindVerdict {
    let p = &pol.p;
    let mut v = BindVerdict::default();
    if st.admin_locked {
        v.error = Some(PolicyError::AccountLocked);
        return v;
    }
    if let Some(at) = st.locked {
        let dur = Policy::secs(p.lockout_duration);
        if dur == 0 || now < at.saturating_add(dur) {
            v.error = Some(PolicyError::AccountLocked);
            return v;
        }
        v.mods.push(clear("pwdAccountLockedTime"));
        v.mods.push(clear("pwdFailureTime"));
    }
    if !password_ok {
        let window = Policy::secs(p.failure_interval);
        let mut fails: Vec<u64> = st
            .failures
            .iter()
            .copied()
            .filter(|&f| window == 0 || now < f.saturating_add(window))
            .filter(|_| st.locked.is_none())
            .collect();
        // Each failure its own time, though two come in one microsecond.
        let at = fails.iter().max().map_or(now, |l| now.max(l + 1));
        fails.push(at);
        let max = Policy::n(p.max_failure);
        v.mods.retain(|m| !m.attr.ty.eq_ignore_ascii_case("pwdFailureTime"));
        v.mods.push(replace(
            "pwdFailureTime",
            fails.iter().map(|&f| gentime(f).into_bytes()).collect(),
        ));
        if p.lockout == Some(true) && max > 0 && fails.len() as u64 >= max {
            v.mods.retain(|m| !m.attr.ty.eq_ignore_ascii_case("pwdAccountLockedTime"));
            v.mods
                .push(replace("pwdAccountLockedTime", vec![gentime(now).into_bytes()]));
        }
        return v;
    }
    if !st.failures.is_empty() && st.locked.is_none() {
        v.mods.push(clear("pwdFailureTime"));
    }
    let max_age = Policy::secs(p.max_age);
    if max_age > 0
        && let Some(changed) = st.changed
    {
        let age = now.saturating_sub(changed);
        if age > max_age {
            let limit = Policy::n(p.grace_limit);
            let used = st.grace.len() as u64;
            if used < limit {
                let mut g: Vec<Vec<u8>> = st.grace.iter().map(|&t| gentime(t).into_bytes()).collect();
                g.push(gentime(now).into_bytes());
                v.mods.push(replace("pwdGraceUseTime", g));
                v.warning = Some(PolicyWarning::GraceAuthNsRemaining(
                    i32::try_from(limit - used - 1).unwrap_or(i32::MAX),
                ));
            } else {
                v.error = Some(PolicyError::PasswordExpired);
                return v;
            }
        } else {
            let left = max_age - age;
            if left <= Policy::secs(p.expire_warning) {
                v.warning = Some(PolicyWarning::TimeBeforeExpiration(
                    i32::try_from(left / SEC).unwrap_or(i32::MAX),
                ));
            }
        }
    }
    if st.reset && p.must_change == Some(true) {
        v.error = Some(PolicyError::ChangeAfterReset);
        v.must_change = true;
    }
    v.allowed = true;
    v
}

/// Who changes the password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Changer {
    /// The user, bound as itself.
    Self_,
    /// Another user with the right to.
    Other,
    /// The directory administrator (quality rules waived).
    Admin,
}

/// Whether a clear password meets the quality rules.
fn quality(p: &Partial, new: &[u8], old_clear: Option<&[u8]>) -> Result<(), PolicyError> {
    let s = String::from_utf8_lossy(new);
    let chars: Vec<char> = s.chars().collect();
    if (chars.len() as u64) < Policy::n(p.min_length) {
        return Err(PolicyError::PasswordTooShort);
    }
    let alpha = chars.iter().filter(|c| c.is_alphabetic()).count() as u64;
    let other = chars.len() as u64 - alpha;
    if alpha < Policy::n(p.min_alpha) || other < Policy::n(p.min_other) {
        return Err(PolicyError::InsufficientPasswordQuality);
    }
    let max_rep = Policy::n(p.max_repeated);
    if max_rep > 0 {
        let mut run = 0u64;
        let mut last = None;
        for c in &chars {
            run = if Some(c) == last { run + 1 } else { 1 };
            last = Some(c);
            if run > max_rep {
                return Err(PolicyError::InsufficientPasswordQuality);
            }
        }
    }
    if let Some(old) = old_clear {
        let old = String::from_utf8_lossy(old);
        let diff = chars.iter().filter(|c| !old.contains(**c)).count() as u64;
        if diff < Policy::n(p.min_diff) {
            return Err(PolicyError::InsufficientPasswordQuality);
        }
    }
    Ok(())
}

/// What a password change must satisfy and the changes it brings.
#[derive(Debug, Clone)]
pub struct Change<'a> {
    /// The new value as the client sent it (clear, or pre-hashed).
    pub new: &'a [u8],
    /// The old password in clear when the client proved it.
    pub old_clear: Option<&'a [u8]>,
    /// The stored values being replaced.
    pub current: &'a [Vec<u8>],
    /// Who changes it.
    pub by: Changer,
}

/// A password change under the policy: the error the response control
/// names, or the changes to the entry's operational attributes.
///
/// # Errors
/// The draft's error for a change the policy refuses.
pub fn on_change(
    pol: &Policy,
    st: &State,
    now: u64,
    c: &Change<'_>,
) -> Result<Vec<Modification>, PolicyError> {
    let p = &pol.p;
    if c.by == Changer::Self_ {
        if p.allow_user_change == Some(false) {
            return Err(PolicyError::PasswordModNotAllowed);
        }
        if p.safe_modify == Some(true) && c.old_clear.is_none() {
            return Err(PolicyError::MustSupplyOldPassword);
        }
        let min_age = Policy::secs(p.min_age);
        if min_age > 0
            && let Some(ch) = st.changed
            && now < ch.saturating_add(min_age)
        {
            return Err(PolicyError::PasswordTooYoung);
        }
    }
    if c.by != Changer::Admin {
        let check = Policy::n(p.check_syntax);
        if check > 0 {
            if scheme::is_hashed(c.new) {
                if check == 2 {
                    return Err(PolicyError::InsufficientPasswordQuality);
                }
            } else {
                quality(p, c.new, c.old_clear)?;
            }
        }
    }
    let keep = Policy::n(p.in_history);
    if keep > 0 && !scheme::is_hashed(c.new) {
        let old_values = st.history.iter().filter_map(|h| history_value(h));
        if c.current.iter().map(Vec::as_slice).chain(old_values).any(|h| scheme::verify(h, c.new)) {
            return Err(PolicyError::PasswordInHistory);
        }
    }
    let mut mods = vec![replace("pwdChangedTime", vec![gentime(now).into_bytes()])];
    mods.push(clear("pwdFailureTime"));
    mods.push(clear("pwdGraceUseTime"));
    mods.push(clear("pwdExpirationWarned"));
    if keep > 0 {
        let mut hist = st.history.clone();
        for v in c.current {
            hist.push(history_entry(now, v));
        }
        let skip = hist.len().saturating_sub(usize::try_from(keep).unwrap_or(usize::MAX));
        mods.push(replace("pwdHistory", hist.split_off(skip)));
    }
    if c.by == Changer::Self_ || p.must_change != Some(true) {
        mods.push(clear("pwdReset"));
    } else {
        mods.push(replace("pwdReset", vec![b"TRUE".to_vec()]));
    }
    Ok(mods)
}

/// A `pwdHistory` value: time, the syntax OID, the length and the value
/// (the draft's form).
#[must_use]
pub fn history_entry(now: u64, value: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "{}#1.3.6.1.4.1.1466.115.121.1.40#{}#",
        gentime(now),
        value.len()
    )
    .into_bytes();
    out.extend_from_slice(value);
    out
}

/// The stored password a `pwdHistory` value holds.
#[must_use]
pub fn history_value(h: &[u8]) -> Option<&[u8]> {
    let mut at = 0;
    for _ in 0..3 {
        at += h[at..].iter().position(|&b| b == b'#')? + 1;
    }
    Some(&h[at..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pol(attrs: &[(&str, &str)]) -> Policy {
        let a: Vec<Attribute> = attrs.iter().map(|(t, v)| Attribute::new(t, &[v])).collect();
        Policy::effective(&Partial::from_attrs(&a), &[], None)
    }

    #[test]
    fn lockout_after_failures() {
        let p = pol(&[
            ("ibm-pwdPolicy", "TRUE"),
            ("pwdLockout", "TRUE"),
            ("pwdMaxFailure", "2"),
            ("pwdLockoutDuration", "60"),
        ]);
        let mut st = State::default();
        let v = on_bind(&p, &st, SEC, false);
        assert!(!v.allowed);
        st.failures = vec![SEC];
        let v = on_bind(&p, &st, 2 * SEC, false);
        assert!(v.mods.iter().any(|m| m.attr.ty == "pwdAccountLockedTime"));
        st.locked = Some(2 * SEC);
        let v = on_bind(&p, &st, 3 * SEC, true);
        assert_eq!(v.error, Some(PolicyError::AccountLocked));
        let v = on_bind(&p, &st, 70 * SEC, true);
        assert!(v.allowed);
    }

    #[test]
    fn expiry_grace_and_warning() {
        let p = pol(&[
            ("ibm-pwdPolicy", "TRUE"),
            ("pwdMaxAge", "100"),
            ("pwdExpireWarning", "50"),
            ("pwdGraceLoginLimit", "1"),
        ]);
        let mut st = State {
            changed: Some(0),
            ..State::default()
        };
        let v = on_bind(&p, &st, 60 * SEC, true);
        assert_eq!(v.warning, Some(PolicyWarning::TimeBeforeExpiration(40)));
        let v = on_bind(&p, &st, 200 * SEC, true);
        assert!(v.allowed);
        assert_eq!(v.warning, Some(PolicyWarning::GraceAuthNsRemaining(0)));
        st.grace = vec![200 * SEC];
        let v = on_bind(&p, &st, 201 * SEC, true);
        assert_eq!(v.error, Some(PolicyError::PasswordExpired));
    }

    #[test]
    fn changes_checked() {
        let p = pol(&[
            ("ibm-pwdPolicy", "TRUE"),
            ("pwdCheckSyntax", "2"),
            ("pwdMinLength", "6"),
            ("pwdInHistory", "2"),
            ("pwdSafeModify", "TRUE"),
            ("pwdMustChange", "TRUE"),
        ]);
        let cur = vec![scheme::Scheme::Ssha.hash(b"oldpass", &[1; 8])];
        let st = State::default();
        let ch = |new: &'static [u8], by, old: Option<&'static [u8]>| Change {
            new,
            old_clear: old,
            current: &cur,
            by,
        };
        assert_eq!(
            on_change(&p, &st, 0, &ch(b"abc", Changer::Self_, Some(b"oldpass"))),
            Err(PolicyError::PasswordTooShort)
        );
        assert_eq!(
            on_change(&p, &st, 0, &ch(b"newpass1", Changer::Self_, None)),
            Err(PolicyError::MustSupplyOldPassword)
        );
        assert_eq!(
            on_change(&p, &st, 0, &ch(b"oldpass", Changer::Other, None)),
            Err(PolicyError::PasswordInHistory)
        );
        let mods = on_change(&p, &st, 0, &ch(b"newpass1", Changer::Other, None)).unwrap();
        assert!(mods.iter().any(|m| m.attr.ty == "pwdReset" && !m.attr.vals.is_empty()));
        let h = mods.iter().find(|m| m.attr.ty == "pwdHistory").unwrap();
        assert_eq!(history_value(&h.attr.vals[0]), Some(cur[0].as_slice()));
    }

    #[test]
    fn group_policies_combine() {
        let a = Partial {
            min_length: Some(4),
            max_age: Some(0),
            ..Partial::default()
        };
        let b = Partial {
            min_length: Some(8),
            max_age: Some(30),
            ..Partial::default()
        };
        let s = a.strictest(&b);
        assert_eq!(s.min_length, Some(8));
        assert_eq!(s.max_age, Some(30));
        let ind = Partial {
            min_length: Some(2),
            ..Partial::default()
        };
        let e = Policy::effective(&Partial::default(), &[a, b], Some(&ind));
        assert_eq!(e.p.min_length, Some(2));
    }
}
