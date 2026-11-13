// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `NFSv4` identity mapping (docs/NFS.md section 5.10): `owner` and
//! `owner_group` strings as `profile@domain`, both ways.
//!
//! [`IdMap`] is what the job implements (profile name to UID and GID
//! through the service, the domain from the NFS options or the TCP/IP
//! domain name); [`Registry`] is one cache over it that every
//! `QNFSNFSD` thread and the mounting client share (`QNFSRGYD`'s).
//!
//! As RFC 7530 section 5.9 has it with `AUTH_SYS`: an ID no profile
//! has is sent as its number; a number is taken as the ID; a name in
//! another domain, or one no profile has, is [`IdError::BadOwner`]
//! (`NFS4ERR_BADOWNER`); bytes that are not UTF-8 are
//! [`IdError::BadChar`]. Names compare without case (profile names are
//! upper case), domains likewise.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::server::instance::Clock;

/// Where names and IDs come from (the job's link to the service).
pub trait IdMap: Send + Sync {
    /// The `NFSv4` domain (the NFS options', else the TCP/IP domain name).
    fn domain(&self) -> String;
    /// The profile with UID `uid`.
    fn user_name(&self, uid: u32) -> Option<String>;
    /// The UID of the profile `name` (upper case).
    fn user_id(&self, name: &str) -> Option<u32>;
    /// The group profile with GID `gid`.
    fn group_name(&self, gid: u32) -> Option<String>;
    /// The GID of the group profile `name` (upper case).
    fn group_id(&self, name: &str) -> Option<u32>;
}

/// Why a string is not an ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// Not UTF-8 (`NFS4ERR_BADCHAR`).
    #[error("not UTF-8")]
    BadChar,
    /// No such user or group here, or another domain (`NFS4ERR_BADOWNER`).
    #[error("not a known owner")]
    BadOwner,
}

/// How long an answer is kept by default.
pub const TTL: Duration = Duration::from_secs(600);

/// Entries kept at most per table; the table is cleared past it.
const MAX: usize = 4096;

#[derive(Debug, Default)]
struct Tables {
    users: HashMap<u32, (Option<String>, Instant)>,
    uids: HashMap<String, (Option<u32>, Instant)>,
    groups: HashMap<u32, (Option<String>, Instant)>,
    gids: HashMap<String, (Option<u32>, Instant)>,
    domain: Option<(String, Instant)>,
}

/// The cached mapping.
pub struct Registry {
    map: Arc<dyn IdMap>,
    clock: Arc<dyn Clock>,
    ttl: Duration,
    t: Mutex<Tables>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

fn get<K, V>(
    m: &mut HashMap<K, (V, Instant)>,
    k: &K,
    now: Instant,
    ttl: Duration,
) -> Option<V>
where
    K: std::hash::Hash + Eq,
    V: Clone,
{
    match m.get(k) {
        Some((v, at)) if now.saturating_duration_since(*at) < ttl => Some(v.clone()),
        _ => None,
    }
}

fn put<K: std::hash::Hash + Eq, V>(m: &mut HashMap<K, (V, Instant)>, k: K, v: V, now: Instant) {
    if m.len() >= MAX {
        m.clear();
    }
    m.insert(k, (v, now));
}

/// A string of digits only, as a number.
fn numeric(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 10 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

impl Registry {
    /// A registry over `map`, answers kept `ttl` on `clock`.
    pub fn new(map: Arc<dyn IdMap>, ttl: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            map,
            clock,
            ttl,
            t: Mutex::new(Tables::default()),
        }
    }

    fn tables(&self) -> std::sync::MutexGuard<'_, Tables> {
        self.t.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Forgets every answer (a profile changed).
    pub fn clear(&self) {
        *self.tables() = Tables::default();
    }

    /// The domain.
    pub fn domain(&self) -> String {
        let now = self.clock.now();
        if let Some((d, at)) = &self.tables().domain
            && now.saturating_duration_since(*at) < self.ttl
        {
            return d.clone();
        }
        let d = self.map.domain();
        self.tables().domain = Some((d.clone(), now));
        d
    }

    fn user_name(&self, uid: u32) -> Option<String> {
        let now = self.clock.now();
        if let Some(v) = get(&mut self.tables().users, &uid, now, self.ttl) {
            return v;
        }
        let v = self.map.user_name(uid);
        put(&mut self.tables().users, uid, v.clone(), now);
        v
    }

    fn group_name(&self, gid: u32) -> Option<String> {
        let now = self.clock.now();
        if let Some(v) = get(&mut self.tables().groups, &gid, now, self.ttl) {
            return v;
        }
        let v = self.map.group_name(gid);
        put(&mut self.tables().groups, gid, v.clone(), now);
        v
    }

    fn user_id(&self, name: &str) -> Option<u32> {
        let now = self.clock.now();
        let k = name.to_ascii_uppercase();
        if let Some(v) = get(&mut self.tables().uids, &k, now, self.ttl) {
            return v;
        }
        let v = self.map.user_id(&k);
        put(&mut self.tables().uids, k, v, now);
        v
    }

    fn group_id(&self, name: &str) -> Option<u32> {
        let now = self.clock.now();
        let k = name.to_ascii_uppercase();
        if let Some(v) = get(&mut self.tables().gids, &k, now, self.ttl) {
            return v;
        }
        let v = self.map.group_id(&k);
        put(&mut self.tables().gids, k, v, now);
        v
    }

    fn with_domain(&self, name: &str) -> String {
        format!("{name}@{}", self.domain())
    }

    /// The `owner` string of UID `uid` (`name`: the profile, when the
    /// caller already knows it).
    pub fn owner(&self, uid: u32, name: Option<&str>) -> String {
        match name.filter(|n| !n.is_empty()) {
            Some(n) => self.with_domain(n),
            None => self
                .user_name(uid)
                .map_or_else(|| uid.to_string(), |n| self.with_domain(&n)),
        }
    }

    /// The `owner_group` string of GID `gid`.
    pub fn group(&self, gid: u32, name: Option<&str>) -> String {
        match name.filter(|n| !n.is_empty()) {
            Some(n) => self.with_domain(n),
            None => self
                .group_name(gid)
                .map_or_else(|| gid.to_string(), |n| self.with_domain(&n)),
        }
    }

    fn split<'a>(&self, who: &'a [u8]) -> Result<Result<u32, &'a str>, IdError> {
        let s = std::str::from_utf8(who).map_err(|_| IdError::BadChar)?;
        if let Some(n) = numeric(s) {
            return Ok(Ok(n));
        }
        let Some((name, dom)) = s.rsplit_once('@') else {
            return Err(IdError::BadOwner);
        };
        if name.is_empty() || !dom.eq_ignore_ascii_case(&self.domain()) {
            return Err(IdError::BadOwner);
        }
        Ok(Err(name))
    }

    /// The UID an `owner` string names.
    ///
    /// # Errors
    ///
    /// [`IdError`].
    pub fn uid(&self, who: &[u8]) -> Result<u32, IdError> {
        match self.split(who)? {
            Ok(n) => Ok(n),
            Err(name) => self.user_id(name).ok_or(IdError::BadOwner),
        }
    }

    /// The GID an `owner_group` string names.
    ///
    /// # Errors
    ///
    /// [`IdError`].
    pub fn gid(&self, who: &[u8]) -> Result<u32, IdError> {
        match self.split(who)? {
            Ok(n) => Ok(n),
            Err(name) => self.group_id(name).ok_or(IdError::BadOwner),
        }
    }
}

/// An [`IdMap`] in memory (tests, and a job without the service).
#[derive(Debug, Default, Clone)]
pub struct MemIdMap {
    /// The domain.
    pub domain: String,
    /// Users: name and UID.
    pub users: Vec<(String, u32)>,
    /// Groups: name and GID.
    pub groups: Vec<(String, u32)>,
}

impl IdMap for MemIdMap {
    fn domain(&self) -> String {
        self.domain.clone()
    }
    fn user_name(&self, uid: u32) -> Option<String> {
        self.users.iter().find(|u| u.1 == uid).map(|u| u.0.clone())
    }
    fn user_id(&self, name: &str) -> Option<u32> {
        self.users
            .iter()
            .find(|u| u.0.eq_ignore_ascii_case(name))
            .map(|u| u.1)
    }
    fn group_name(&self, gid: u32) -> Option<String> {
        self.groups.iter().find(|g| g.1 == gid).map(|g| g.0.clone())
    }
    fn group_id(&self, name: &str) -> Option<u32> {
        self.groups
            .iter()
            .find(|g| g.0.eq_ignore_ascii_case(name))
            .map(|g| g.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::instance::ManualClock;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Counting(MemIdMap, AtomicU32);

    impl IdMap for Counting {
        fn domain(&self) -> String {
            self.0.domain()
        }
        fn user_name(&self, uid: u32) -> Option<String> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.user_name(uid)
        }
        fn user_id(&self, name: &str) -> Option<u32> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.user_id(name)
        }
        fn group_name(&self, gid: u32) -> Option<String> {
            self.0.group_name(gid)
        }
        fn group_id(&self, name: &str) -> Option<u32> {
            self.0.group_id(name)
        }
    }

    fn map() -> MemIdMap {
        MemIdMap {
            domain: "example.com".into(),
            users: vec![("BOB".into(), 100)],
            groups: vec![("DEV".into(), 500)],
        }
    }

    #[test]
    fn names_both_ways() {
        let clock = Arc::new(ManualClock::default());
        let r = Registry::new(Arc::new(map()), TTL, clock);
        assert_eq!(r.owner(100, None), "BOB@example.com");
        assert_eq!(r.owner(7, None), "7");
        assert_eq!(r.owner(7, Some("ANN")), "ANN@example.com");
        assert_eq!(r.group(500, None), "DEV@example.com");
        assert_eq!(r.group(9, None), "9");
        assert_eq!(r.uid(b"bob@EXAMPLE.com"), Ok(100));
        assert_eq!(r.uid(b"4242"), Ok(4242));
        assert_eq!(r.uid(b"bob@other.org"), Err(IdError::BadOwner));
        assert_eq!(r.uid(b"nobody@example.com"), Err(IdError::BadOwner));
        assert_eq!(r.uid(b"bob"), Err(IdError::BadOwner));
        assert_eq!(r.uid(&[0xff, 0xfe]), Err(IdError::BadChar));
        assert_eq!(r.gid(b"dev@example.com"), Ok(500));
    }

    #[test]
    fn answers_are_cached_until_they_expire() {
        let clock = Arc::new(ManualClock::default());
        let m = Arc::new(Counting(map(), AtomicU32::new(0)));
        let r = Registry::new(m.clone(), Duration::from_secs(10), clock.clone());
        for _ in 0..5 {
            assert_eq!(r.owner(100, None), "BOB@example.com");
            assert_eq!(r.uid(b"BOB@example.com"), Ok(100));
        }
        assert_eq!(m.1.load(Ordering::Relaxed), 2);
        clock.advance(Duration::from_secs(11));
        r.owner(100, None);
        assert_eq!(m.1.load(Ordering::Relaxed), 3);
        r.clear();
        r.owner(100, None);
        assert_eq!(m.1.load(Ordering::Relaxed), 4);
    }
}
