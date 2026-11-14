// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `projected` backend (docs/DIRSRV.md §8, §9): the system's user
//! profiles under `os400-sys=NAME`, read-only.
//!
//! - `os400-sys=NAME` (`os400-root`), seen by everyone;
//! - `cn=accounts,os400-sys=NAME` (`container`);
//! - `os400-profile=PRF,cn=accounts,os400-sys=NAME` (`os400-usrprf`) per
//!   profile, a group profile with its members.
//!
//! Who sees what: an administrator session and a profile with `*ALLOBJ`
//! or `*SECADM` every entry; a session bound as a profile the container
//! and the profiles it has `*READ` to; anyone else (anonymous, a DN of
//! the entry store) the root only. A hidden entry resolves but has no
//! entry, so a search of it succeeds with none, as the primary answers
//! anonymous users; for a session that does not see every profile a
//! profile that does not exist is hidden the same way, so names cannot
//! be found out by asking.
//!
//! The profiles come through [`Profiles`], which the service implements
//! over `sec`.

use ldap::ResultCode;
use ldap::dn::{Ava, Dn, Rdn};
use ldap::filter::Filter;
use ldap::msg::{Attribute, Scope};
use store::txn::Txn;

use super::{Backend, DirEntry, Resolution, Scan};
use crate::ops::error::{OpError, OpResult};
use crate::ops::session::Session;
use crate::schema::Schema;

/// The container's RDN.
pub const ACCOUNTS: &str = "cn=accounts";

/// The root's ID.
pub const ROOT: u64 = 1;
/// The container's ID.
pub const CONTAINER: u64 = 2;
/// The ID of an entry hidden from the session asking (it may not exist).
pub const HIDDEN: u64 = 3;
/// Set in every profile's ID; the name is packed below it.
const PROFILE: u64 = 1 << 62;

/// A profile as the backend shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileInfo {
    /// The name, upper case.
    pub name: String,
    /// A group profile.
    pub group: bool,
    /// A group's members, by name.
    pub members: Vec<String>,
    /// The `os400-*` attributes and their values (`os400-profile`,
    /// `os400-groupmember` and `objectClass` are the backend's).
    pub attrs: Vec<(String, Vec<String>)>,
}

/// The profiles, as the service reads them from `sec`. Every call runs
/// in the request's store transaction.
pub trait Profiles: Send {
    /// Every profile's name, upper case.
    ///
    /// # Errors
    /// The store's.
    fn names(&self, txn: &mut Txn<'_>) -> OpResult<Vec<String>>;

    /// Profile `name`, `None` when there is none.
    ///
    /// # Errors
    /// The store's.
    fn profile(&self, txn: &mut Txn<'_>, name: &str) -> OpResult<Option<ProfileInfo>>;

    /// Whether profile `viewer` sees every profile (`*ALLOBJ`, `*SECADM`).
    fn sees_all(&self, txn: &mut Txn<'_>, viewer: &str) -> bool;

    /// Whether profile `viewer` has `*READ` to profile `name`.
    fn may_read(&self, txn: &mut Txn<'_>, viewer: &str, name: &str) -> bool;
}

/// The characters of a profile name, each packed in six bits (0 is no
/// character).
const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789$#@_";

/// A profile's entry ID, `None` for a string that is no profile name.
#[must_use]
pub fn profile_id(name: &str) -> Option<u64> {
    let b = name.as_bytes();
    if b.is_empty() || b.len() > 10 {
        return None;
    }
    let mut id = 0u64;
    for &c in b {
        let at = CHARS.iter().position(|&x| x == c.to_ascii_uppercase())?;
        id = (id << 6) | (at as u64 + 1);
    }
    Some(PROFILE | id)
}

/// The profile name of entry ID `id`.
#[must_use]
pub fn profile_name(id: u64) -> Option<String> {
    if id & PROFILE == 0 {
        return None;
    }
    let mut v = id & !PROFILE;
    let mut out = Vec::new();
    while v != 0 {
        let c = usize::try_from(v & 0x3f).ok()?;
        out.push(*CHARS.get(c.checked_sub(1)?)?);
        v >>= 6;
    }
    out.reverse();
    String::from_utf8(out).ok()
}

/// What a DN under the suffix names.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Named {
    Root,
    Container,
    Profile(String),
    /// Below a profile, or a container child that is no profile DN.
    UnderContainer,
    /// Anything else under the root.
    UnderRoot,
}

/// Who a session is to the backend.
enum Viewer {
    /// Everything.
    All,
    /// A profile.
    Profile(String),
    /// The root only.
    Outsider,
}

/// The projected backend over `P`.
pub struct ProjectedBackend<P: Profiles> {
    suffix: Dn,
    container: Dn,
    /// The profiles.
    pub profiles: P,
}

impl<P: Profiles> ProjectedBackend<P> {
    /// The backend under `suffix` (`os400-sys=NAME`).
    ///
    /// # Errors
    /// 34 when `suffix` is not a DN.
    pub fn new(suffix: &str, profiles: P) -> OpResult<Self> {
        let suffix = Dn::parse(suffix)
            .map_err(|_| OpError::new(ResultCode::InvalidDnSyntax, format!("suffix {suffix}")))?;
        let container = suffix.child(Rdn(vec![Ava {
            ty: "cn".into(),
            value: b"accounts".to_vec(),
        }]));
        Ok(Self {
            suffix,
            container,
            profiles,
        })
    }

    /// The DN of profile `name`.
    #[must_use]
    pub fn profile_dn(&self, name: &str) -> String {
        format!("os400-profile={name},{}", self.container)
    }

    /// The profile `dn` names, when it is a profile's DN under this
    /// suffix (any case; the name returned upper case).
    #[must_use]
    pub fn profile_of(&self, schema: &Schema, dn: &Dn) -> Option<String> {
        match self.named(schema, dn)? {
            Named::Profile(n) => Some(n),
            _ => None,
        }
    }

    fn named(&self, schema: &Schema, dn: &Dn) -> Option<Named> {
        if !schema.dn_within(dn, &self.suffix) {
            return None;
        }
        if dn.len() == self.suffix.len() {
            return Some(Named::Root);
        }
        if !schema.dn_within(dn, &self.container) {
            return Some(Named::UnderRoot);
        }
        if dn.len() == self.container.len() {
            return Some(Named::Container);
        }
        if dn.len() > self.container.len() + 1 {
            return Some(Named::UnderContainer);
        }
        let rdn = &dn.0[0];
        let [ava] = rdn.0.as_slice() else {
            return Some(Named::UnderContainer);
        };
        let ok = schema
            .attribute(&ava.ty)
            .is_some_and(|a| a.name().eq_ignore_ascii_case("os400-profile"));
        let name = String::from_utf8_lossy(&ava.value)
            .trim()
            .to_ascii_uppercase();
        if !ok || profile_id(&name).is_none() {
            return Some(Named::UnderContainer);
        }
        Some(Named::Profile(name))
    }

    fn viewer(&self, txn: &mut Txn<'_>, schema: &Schema, session: &Session) -> Viewer {
        if session.admin {
            return Viewer::All;
        }
        let Some(dn) = session.dn.as_deref().and_then(|d| Dn::parse(d).ok()) else {
            return Viewer::Outsider;
        };
        match self.profile_of(schema, &dn) {
            Some(p) if self.profiles.sees_all(txn, &p) => Viewer::All,
            Some(p) => Viewer::Profile(p),
            None => Viewer::Outsider,
        }
    }

    fn sees(&self, txn: &mut Txn<'_>, viewer: &Viewer, name: &str) -> bool {
        match viewer {
            Viewer::All => true,
            Viewer::Profile(me) => me == name || self.profiles.may_read(txn, me, name),
            Viewer::Outsider => false,
        }
    }

    fn root_entry() -> Vec<Attribute> {
        // What the primary shows of it.
        vec![Attribute::new("objectClass", &["top", "os400-root"])]
    }

    fn container_entry() -> Vec<Attribute> {
        vec![
            Attribute::new("objectClass", &["top", "container"]),
            Attribute::new("cn", &["accounts"]),
        ]
    }

    fn profile_entry(p: &ProfileInfo) -> Vec<Attribute> {
        let mut out = vec![
            Attribute::new("objectClass", &["top", "os400-usrprf"]),
            Attribute::new("os400-profile", &[p.name.as_str()]),
        ];
        for (ty, vals) in &p.attrs {
            if vals.is_empty()
                || ["objectclass", "os400-profile", "os400-groupmember"]
                    .iter()
                    .any(|x| ty.eq_ignore_ascii_case(x))
            {
                continue;
            }
            let v: Vec<&str> = vals.iter().map(String::as_str).collect();
            out.push(Attribute::new(ty, &v));
        }
        if p.group && !p.members.is_empty() {
            let v: Vec<&str> = p.members.iter().map(String::as_str).collect();
            out.push(Attribute::new("os400-groupmember", &v));
        }
        out
    }

    /// Profile names after `after`, at most `max`, that `viewer` sees.
    fn profiles_after(
        &self,
        txn: &mut Txn<'_>,
        viewer: &Viewer,
        after: Option<&str>,
        max: usize,
    ) -> OpResult<(Vec<u64>, Option<String>)> {
        let mut names = self.profiles.names(txn)?;
        names.sort();
        let mut out = Vec::new();
        let mut last = None;
        for n in names
            .into_iter()
            .filter(|n| after.is_none_or(|a| n.as_str() > a))
        {
            if out.len() >= max.max(1) {
                return Ok((out, last));
            }
            last = Some(n.clone());
            if self.sees(txn, viewer, &n)
                && let Some(id) = profile_id(&n)
            {
                out.push(id);
            }
        }
        Ok((out, None))
    }
}

/// Where a listing of profiles stopped: after this name.
struct After(String);

impl<P: Profiles> Backend for ProjectedBackend<P> {
    fn name(&self) -> &'static str {
        "projected"
    }

    fn suffixes(&self) -> Vec<Dn> {
        vec![self.suffix.clone()]
    }

    fn read_only(&self) -> bool {
        true
    }

    fn resolve(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        session: &Session,
        dn: &Dn,
    ) -> OpResult<Resolution> {
        let Some(named) = self.named(schema, dn) else {
            return Ok(Resolution::default());
        };
        let all = matches!(self.viewer(txn, schema, session), Viewer::All);
        let missing = |matched: u64| {
            if all {
                Resolution {
                    entry: None,
                    matched: Some(matched),
                }
            } else {
                Resolution {
                    entry: Some(HIDDEN),
                    matched: None,
                }
            }
        };
        Ok(match named {
            Named::Root => Resolution {
                entry: Some(ROOT),
                matched: None,
            },
            Named::Container => Resolution {
                entry: Some(CONTAINER),
                matched: None,
            },
            Named::UnderRoot => Resolution {
                entry: None,
                matched: Some(ROOT),
            },
            Named::UnderContainer => missing(CONTAINER),
            Named::Profile(n) => {
                if self.profiles.profile(txn, &n)?.is_some() {
                    Resolution {
                        entry: profile_id(&n),
                        matched: None,
                    }
                } else {
                    missing(CONTAINER)
                }
            }
        })
    }

    fn entry(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        session: &Session,
        id: u64,
    ) -> OpResult<Option<DirEntry>> {
        let viewer = self.viewer(txn, schema, session);
        let (dn, attrs) = match id {
            ROOT => (self.suffix.to_string(), Self::root_entry()),
            CONTAINER => {
                if matches!(viewer, Viewer::Outsider) {
                    return Ok(None);
                }
                (self.container.to_string(), Self::container_entry())
            }
            _ => {
                let Some(name) = profile_name(id) else {
                    return Ok(None);
                };
                if !self.sees(txn, &viewer, &name) {
                    return Ok(None);
                }
                let Some(p) = self.profiles.profile(txn, &name)? else {
                    return Ok(None);
                };
                (self.profile_dn(&p.name), Self::profile_entry(&p))
            }
        };
        Ok(Some(DirEntry { id, dn, attrs }))
    }

    fn parent(&self, _txn: &mut Txn<'_>, id: u64) -> OpResult<Option<u64>> {
        Ok(match id {
            ROOT => None,
            CONTAINER => Some(ROOT),
            _ => Some(CONTAINER),
        })
    }

    fn children(&self, txn: &mut Txn<'_>, id: u64, cap: usize) -> OpResult<u64> {
        Ok(match id {
            ROOT => 1,
            CONTAINER => self.profiles.names(txn)?.len().min(cap) as u64,
            _ => 0,
        })
    }

    fn scan(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        session: &Session,
        base: u64,
        scope: Scope,
        _filter: &Filter,
        state: &mut Scan,
        max: usize,
    ) -> OpResult<Vec<u64>> {
        let viewer = self.viewer(txn, schema, session);
        let outsider = matches!(viewer, Viewer::Outsider);
        let after = match std::mem::take(state) {
            Scan::Done => {
                *state = Scan::Done;
                return Ok(Vec::new());
            }
            Scan::Start => None,
            Scan::Going(b) => Some(
                b.downcast::<After>()
                    .map_or_else(|_| String::new(), |a| a.0),
            ),
        };
        let mut out = Vec::new();
        let profiles = match (base, scope) {
            (ROOT | CONTAINER, Scope::Base) => {
                out.push(base);
                false
            }
            (ROOT, Scope::One) => {
                if !outsider {
                    out.push(CONTAINER);
                }
                false
            }
            (ROOT, Scope::Sub) => {
                if after.is_none() {
                    out.push(ROOT);
                    if !outsider {
                        out.push(CONTAINER);
                    }
                }
                !outsider
            }
            (CONTAINER, scope) => {
                if scope == Scope::Sub && after.is_none() {
                    out.push(CONTAINER);
                }
                !outsider
            }
            (HIDDEN, _) | (_, Scope::One) => false,
            (id, _) => {
                if profile_name(id).is_some_and(|n| self.sees(txn, &viewer, &n)) {
                    out.push(id);
                }
                false
            }
        };
        if profiles {
            let (ids, last) = self.profiles_after(txn, &viewer, after.as_deref(), max)?;
            out.extend(ids);
            if let Some(l) = last {
                *state = Scan::Going(Box::new(After(l)));
                return Ok(out);
            }
        }
        *state = Scan::Done;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for n in ["QSECOFR", "A", "Z9$#@_ABCD", "QDIRSRV"] {
            let id = profile_id(n).unwrap();
            assert!(id > HIDDEN);
            assert_eq!(profile_name(id).as_deref(), Some(n));
        }
        assert_eq!(profile_id("qsecofr"), profile_id("QSECOFR"));
        assert_eq!(profile_id(""), None);
        assert_eq!(profile_id("ELEVENCHARS"), None);
        assert_eq!(profile_id("A-B"), None);
        assert_eq!(profile_name(ROOT), None);
    }
}
