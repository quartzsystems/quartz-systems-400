// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IFS authority: the data and object authorities ([`data`]), what
//! each operation needs ([`need`]), and the search over an object's
//! [`Meta`] in IBM's order, the same steps as `sec::auth::search` uses
//! for library objects: `*ALLOBJ`, owner, private, authorization list,
//! group `*ALLOBJ`, groups (primary group, owner, private, list),
//! `*PUBLIC`, then adopted owners.

pub mod data;
pub mod need;

use sec::auth::bits::Auth;
use sec::auth::principal::{Adopted, Principal};
use sec::auth::required::Need;
use sec::auth::search::{Decision, Found, Step, satisfies};
use sec::profile::spcaut::SpecialAuth;

use crate::meta::{IdMap, Meta};

/// An authorization list as the search needs it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutlInfo {
    /// The list's owner.
    pub owner: String,
    /// The owner's authority.
    pub owner_auth: Auth,
    /// `*PUBLIC` of the list.
    pub public: Auth,
    /// Entries: profile and authority.
    pub entries: Vec<(String, Auth)>,
}

/// What the IFS needs from the rest of the system: profile and UID/GID
/// mapping (through [`IdMap`]), authorization lists, profiles that
/// exist, and `QCRTOBJAUD`. The storage service implements it over the
/// store; tests use [`MemEnv`].
pub trait AuthEnv: IdMap {
    /// The authorization list `name`.
    fn autl(&self, name: &str) -> Option<AutlInfo>;
    /// Whether a user or group profile exists.
    fn profile_exists(&self, name: &str) -> bool;
    /// Whether `name` is a group profile.
    fn is_group(&self, name: &str) -> bool;
    /// `QCRTOBJAUD`.
    fn crtobjaud(&self) -> crate::meta::ObjAudit {
        crate::meta::ObjAudit::None
    }
}

/// An in-memory [`AuthEnv`].
#[derive(Debug, Clone, Default)]
pub struct MemEnv {
    /// Profiles: name, UID, GID (group profiles have a GID).
    pub profiles: Vec<(String, u32, Option<u32>)>,
    /// Authorization lists.
    pub autls: Vec<(String, AutlInfo)>,
    /// `QCRTOBJAUD`.
    pub qcrtobjaud: crate::meta::ObjAudit,
}

impl MemEnv {
    /// Add a user profile.
    #[must_use]
    pub fn user(mut self, name: &str, uid: u32) -> Self {
        self.profiles.push((name.into(), uid, None));
        self
    }

    /// Add a group profile.
    #[must_use]
    pub fn group(mut self, name: &str, uid: u32, gid: u32) -> Self {
        self.profiles.push((name.into(), uid, Some(gid)));
        self
    }

    /// Add an authorization list.
    #[must_use]
    pub fn autl(mut self, name: &str, info: AutlInfo) -> Self {
        self.autls.push((name.into(), info));
        self
    }
}

impl IdMap for MemEnv {
    fn profile_of_uid(&self, uid: u32) -> Option<String> {
        self.profiles
            .iter()
            .find(|p| p.1 == uid)
            .map(|p| p.0.clone())
    }
    fn profile_of_gid(&self, gid: u32) -> Option<String> {
        self.profiles
            .iter()
            .find(|p| p.2 == Some(gid))
            .map(|p| p.0.clone())
    }
    fn uid_of(&self, profile: &str) -> Option<u32> {
        self.profiles.iter().find(|p| p.0 == profile).map(|p| p.1)
    }
    fn gid_of(&self, profile: &str) -> Option<u32> {
        self.profiles
            .iter()
            .find(|p| p.0 == profile)
            .and_then(|p| p.2)
    }
}

impl AuthEnv for MemEnv {
    fn autl(&self, name: &str) -> Option<AutlInfo> {
        self.autls
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, i)| i.clone())
    }
    fn profile_exists(&self, name: &str) -> bool {
        self.profiles.iter().any(|p| p.0 == name)
    }
    fn is_group(&self, name: &str) -> bool {
        self.profiles.iter().any(|p| p.0 == name && p.2.is_some())
    }
    fn crtobjaud(&self) -> crate::meta::ObjAudit {
        self.qcrtobjaud
    }
}

/// `who`'s entry on the list securing `m`.
fn autl_entry(env: &dyn AuthEnv, m: &Meta, who: &str) -> Option<Auth> {
    let list = env.autl(m.autl.as_deref()?)?;
    if list.owner == who {
        return Some(list.owner_auth);
    }
    list.entries
        .iter()
        .find(|(p, _)| p == who)
        .map(|(_, a)| Auth(a.0 & Auth::ALL.0))
}

/// One profile's own entry: owner, private, list.
fn own_entry(env: &dyn AuthEnv, m: &Meta, who: &str) -> Option<(Auth, Step)> {
    if m.owner == who {
        return Some((m.owner_auth, Step::Owner));
    }
    if let Some(a) = m.private_of(who) {
        return Some((a, Step::Private));
    }
    autl_entry(env, m, who).map(|a| (a, Step::Autl))
}

/// Steps 1 to 5 for the user and its groups.
#[must_use]
pub fn find(p: &Principal, m: &Meta, env: &dyn AuthEnv) -> Found {
    let all = Auth(Auth::MASK);
    if SpecialAuth(p.spcaut).has(SpecialAuth::ALLOBJ) || (p.qsecurity != 0 && p.qsecurity <= 20) {
        return Found {
            have: all,
            step: Step::AllObj,
            all: true,
        };
    }
    if let Some((have, step)) = own_entry(env, m, &p.user) {
        return Found {
            have,
            step,
            all: false,
        };
    }
    if SpecialAuth(p.group_spcaut).has(SpecialAuth::ALLOBJ) {
        return Found {
            have: all,
            step: Step::GroupAllObj,
            all: true,
        };
    }
    let mut group: Option<(Auth, Step)> = None;
    for g in &p.groups {
        let entry = if m.group.as_deref() == Some(g.as_str()) {
            Some((m.group_auth, Step::PrimaryGroup))
        } else {
            own_entry(env, m, g).map(|(a, s)| {
                let s = match s {
                    Step::Owner => Step::GroupOwner,
                    Step::Private => Step::GroupPrivate,
                    _ => Step::GroupAutl,
                };
                (a, s)
            })
        };
        if let Some((a, s)) = entry {
            group = Some(match group {
                Some((acc, first)) => (acc.union(a), first),
                None => (a, s),
            });
        }
    }
    if let Some((have, step)) = group {
        return Found {
            have,
            step,
            all: false,
        };
    }
    if m.public_from_autl {
        let have = m
            .autl
            .as_deref()
            .and_then(|l| env.autl(l))
            .map_or(Auth::EXCLUDE, |l| Auth(l.public.0 & Auth::ALL.0));
        return Found {
            have,
            step: Step::PublicAutl,
            all: false,
        };
    }
    Found {
        have: m.public,
        step: Step::Public,
        all: false,
    }
}

/// What one adopted owner has: its `*ALLOBJ`, or its own entry.
fn adopted(a: &Adopted, m: &Meta, env: &dyn AuthEnv) -> Auth {
    if SpecialAuth(a.spcaut).has(SpecialAuth::ALLOBJ) {
        return Auth(Auth::MASK);
    }
    own_entry(env, m, &a.owner).map_or(Auth::EXCLUDE, |(b, _)| b)
}

/// The whole search for `need`.
#[must_use]
pub fn check(p: &Principal, m: &Meta, env: &dyn AuthEnv, need: Need) -> Decision {
    let found = find(p, m, env);
    if satisfies(found.have, need, found.all) {
        return Decision {
            allowed: true,
            step: found.step,
            have: found.have,
        };
    }
    let mut have = found.have;
    for a in &p.adopted {
        have = have.union(adopted(a, m, env));
        if satisfies(have, need, false) {
            return Decision {
                allowed: true,
                step: Step::Adopted,
                have,
            };
        }
    }
    Decision {
        allowed: false,
        step: found.step,
        have,
    }
}

/// The authorities the user has (adoption included), for `access()`
/// and displays.
#[must_use]
pub fn have(p: &Principal, m: &Meta, env: &dyn AuthEnv) -> Auth {
    let found = find(p, m, env);
    if found.all {
        return Auth(Auth::MASK);
    }
    p.adopted
        .iter()
        .fold(found.have, |h, a| h.union(adopted(a, m, env)))
}

/// Whether the user owns `m` or has `*ALLOBJ` (directly, through a
/// group, or adopted).
#[must_use]
pub fn owner_or_allobj(p: &Principal, m: &Meta) -> bool {
    p.user == m.owner || p.has(SpecialAuth::ALLOBJ)
}

#[cfg(test)]
mod tests {
    use super::data::{R, RWX, RX, W, X};
    use super::*;

    fn user(name: &str, groups: &[&str]) -> Principal {
        Principal {
            user: name.into(),
            groups: groups.iter().map(|g| (*g).to_string()).collect(),
            qsecurity: 40,
            ..Principal::default()
        }
    }

    fn obj() -> Meta {
        let mut m = Meta::new("OWNER", 1208, 0);
        m.owner_auth = RWX.union(Auth(data::OBJECT));
        m.group = Some("DEV".into());
        m.group_auth = RX;
        m.public = R;
        m
    }

    #[test]
    fn steps_in_order() {
        let env = MemEnv::default();
        let m = obj();
        let need = |a| Need::Bits(a);
        // Owner.
        let d = check(&user("OWNER", &[]), &m, &env, need(RWX));
        assert_eq!((d.allowed, d.step), (true, Step::Owner));
        // Primary group.
        let d = check(&user("ANN", &["DEV"]), &m, &env, need(X));
        assert_eq!((d.allowed, d.step), (true, Step::PrimaryGroup));
        assert!(!check(&user("ANN", &["DEV"]), &m, &env, need(W)).allowed);
        // *PUBLIC.
        let d = check(&user("ZED", &[]), &m, &env, need(R));
        assert_eq!((d.allowed, d.step), (true, Step::Public));
        // A private *EXCLUDE decides even though *PUBLIC has *R.
        let mut m2 = m.clone();
        m2.set_private("ZED", Some(Auth::EXCLUDE));
        let d = check(&user("ZED", &[]), &m2, &env, need(R));
        assert_eq!((d.allowed, d.step), (false, Step::Private));
        // *ALLOBJ.
        let mut a = user("SEC", &[]);
        a.spcaut = SpecialAuth::ALLOBJ.0;
        assert_eq!(check(&a, &m2, &env, need(RWX)).step, Step::AllObj);
        // Level 20: everyone.
        let mut l20 = user("ZED", &[]);
        l20.qsecurity = 20;
        assert!(check(&l20, &m2, &env, need(RWX)).allowed);
    }

    #[test]
    fn lists_and_adoption() {
        let env = MemEnv::default().autl(
            "PAY",
            AutlInfo {
                owner: "QSECOFR".into(),
                owner_auth: Auth::ALL,
                public: RX,
                entries: vec![("ANN".into(), RWX)],
            },
        );
        let mut m = obj();
        m.autl = Some("PAY".into());
        m.public_from_autl = true;
        let d = check(&user("ANN", &[]), &m, &env, Need::Bits(W));
        assert_eq!((d.allowed, d.step), (true, Step::Autl));
        let d = check(&user("ZED", &[]), &m, &env, Need::Bits(X));
        assert_eq!((d.allowed, d.step), (true, Step::PublicAutl));
        assert!(!check(&user("ZED", &[]), &m, &env, Need::Bits(W)).allowed);
        let mut z = user("ZED", &[]);
        z.adopted.push(Adopted {
            program: "APP/PGM".into(),
            owner: "OWNER".into(),
            groups: Vec::new(),
            spcaut: 0,
        });
        let d = check(&z, &m, &env, Need::Bits(W));
        assert_eq!((d.allowed, d.step), (true, Step::Adopted));
        assert!(have(&z, &m, &env).has(RWX));
        assert!(owner_or_allobj(&user("OWNER", &[]), &m));
        assert!(!owner_or_allobj(&z, &m));
    }
}
