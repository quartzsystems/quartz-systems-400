// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The profile environment the IFS authority search asks for, answered
//! from the store: profile names and their `UID`/`GID` numbers, which
//! profiles are groups, authorization lists as the search reads them,
//! `QCRTOBJAUD`, and the principal of a profile a call is made as. The
//! profiles are read once per environment (one request), or once per
//! change of the profiles when the access keeps an [`EnvCache`] (the
//! storage service does).

use std::cell::RefCell;
use std::sync::{Arc, Mutex, PoisonError};

use ifs::Errno;
use ifs::auth::{AuthEnv, AutlInfo};
use ifs::client::caller::PrincipalCache;
use ifs::meta::{IdMap, ObjAudit};
use sec::auth::autl;
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::objects;
use store::obj::header::text_of;

use crate::access::StoreAccess;
use crate::ids::{IdCache, IdIndex, Row};

/// What an access keeps across requests: the profile index and the
/// principals built, both valid until a profile changes.
#[derive(Debug, Default)]
pub struct EnvCache {
    /// The profiles by name, UID and GID.
    pub ids: IdCache,
    /// Principals by name.
    pub principals: Mutex<PrincipalCache>,
}

/// What changes with any profile, group membership or authorization
/// list: the security epoch and the count of profile writes.
#[must_use]
pub fn stamp() -> (u64, u64) {
    (sec::auth::cache::epoch(), objects::writes())
}

/// The environment over a store.
pub struct StoreEnv<'a, S> {
    access: &'a S,
    rows: RefCell<Option<Arc<IdIndex>>>,
}

impl<'a, S: StoreAccess> StoreEnv<'a, S> {
    /// Over `access`.
    pub fn new(access: &'a S) -> Self {
        Self {
            access,
            rows: RefCell::new(None),
        }
    }

    fn load(&self) -> Vec<Row> {
        self.access
            .with_main(&mut |txn, qsys| {
                let mut v = Vec::new();
                for n in objects::list(txn, qsys).map_err(|_| Errno::EIO)? {
                    if let Ok((_, p)) = objects::read(txn, qsys, &n) {
                        v.push(Row {
                            name: p.name.clone(),
                            uid: p.uid,
                            gid: p.gid,
                            groups: p.groups(),
                        });
                    }
                }
                Ok(v)
            })
            .unwrap_or_default()
    }

    fn with_rows<T>(&self, f: impl FnOnce(&IdIndex) -> T) -> T {
        let mut rows = self.rows.borrow_mut();
        if rows.is_none() {
            *rows = Some(match self.access.env_cache() {
                Some(c) => c.ids.get(stamp(), || self.load()),
                None => Arc::new(IdIndex::new(self.load())),
            });
        }
        f(rows.as_deref().expect("loaded"))
    }

    fn build_principal(&self, name: &str) -> Result<Principal, Errno> {
        let qsecurity = self
            .access
            .sysval("QSECURITY")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(40);
        self.access.with_main(&mut |txn, qsys| {
            Principal::load(txn, qsys, name, qsecurity).map_err(|_| Errno::ENOTENROLL)
        })
    }
}

impl<S: StoreAccess> IdMap for StoreEnv<'_, S> {
    fn profile_of_uid(&self, uid: u32) -> Option<String> {
        self.with_rows(|r| r.by_uid(uid).map(|p| p.name.clone()))
    }
    fn profile_of_gid(&self, gid: u32) -> Option<String> {
        self.with_rows(|r| r.by_gid(gid).map(|p| p.name.clone()))
    }
    fn uid_of(&self, profile: &str) -> Option<u32> {
        self.with_rows(|r| r.by_name(profile).map(|p| p.uid))
    }
    fn gid_of(&self, profile: &str) -> Option<u32> {
        self.with_rows(|r| r.by_name(profile).filter(|p| p.gid != 0).map(|p| p.gid))
    }
}

impl<S: StoreAccess> AuthEnv for StoreEnv<'_, S> {
    fn autl(&self, name: &str) -> Option<AutlInfo> {
        self.access
            .with_main(&mut |txn, qsys| {
                let id = autl::find(txn, qsys, name).map_err(|_| Errno::ENOENT)?;
                let (_, h) = store::obj::auth::load(txn, id).map_err(|_| Errno::EIO)?;
                let entries = store::obj::auth::private_list(txn, &h)
                    .map_err(|_| Errno::EIO)?
                    .into_iter()
                    .map(|p| (text_of(&p.profile), Auth::of(p.bits)))
                    .collect();
                Ok(AutlInfo {
                    owner: text_of(&h.owner),
                    owner_auth: Auth::of(h.owner_authority),
                    public: Auth(Auth::of(h.public_authority).0 & Auth::ALL.0),
                    entries,
                })
            })
            .ok()
    }

    fn profile_exists(&self, name: &str) -> bool {
        self.with_rows(|r| r.by_name(name).is_some())
    }

    fn is_group(&self, name: &str) -> bool {
        self.with_rows(|r| r.is_group(name))
    }

    fn crtobjaud(&self) -> ObjAudit {
        self.access
            .sysval("QCRTOBJAUD")
            .and_then(|v| ObjAudit::parse(v.trim()))
            .unwrap_or(ObjAudit::None)
    }

    fn principal_of(&self, profile: &str) -> Option<Principal> {
        if !self.profile_exists(profile) {
            return None;
        }
        match self.access.env_cache() {
            Some(c) => c
                .principals
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(stamp(), profile, |n| self.build_principal(n))
                .ok(),
            None => self.build_principal(profile).ok(),
        }
    }
}
