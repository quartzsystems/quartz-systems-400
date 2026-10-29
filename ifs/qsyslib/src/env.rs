// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The profile environment the IFS authority search asks for, answered
//! from the store: profile names and their `UID`/`GID` numbers, which
//! profiles are groups, authorization lists as the search reads them,
//! and `QCRTOBJAUD`. The profiles are read once per environment (one
//! request), so a `stat` that maps owner and group costs one read.

use std::cell::RefCell;

use ifs::Errno;
use ifs::auth::{AuthEnv, AutlInfo};
use ifs::meta::{IdMap, ObjAudit};
use sec::auth::autl;
use sec::auth::bits::Auth;
use sec::profile::objects;
use store::obj::header::text_of;

use crate::access::StoreAccess;

/// One profile as the environment needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    name: String,
    uid: u32,
    gid: u32,
    groups: Vec<String>,
}

/// The environment over a store.
pub struct StoreEnv<'a, S> {
    access: &'a S,
    rows: RefCell<Option<Vec<Row>>>,
}

impl<'a, S: StoreAccess> StoreEnv<'a, S> {
    /// Over `access`.
    pub fn new(access: &'a S) -> Self {
        Self {
            access,
            rows: RefCell::new(None),
        }
    }

    fn with_rows<T>(&self, f: impl FnOnce(&[Row]) -> T) -> T {
        let mut rows = self.rows.borrow_mut();
        if rows.is_none() {
            let loaded = self
                .access
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
                .unwrap_or_default();
            *rows = Some(loaded);
        }
        f(rows.as_deref().unwrap_or(&[]))
    }
}

impl<S: StoreAccess> IdMap for StoreEnv<'_, S> {
    fn profile_of_uid(&self, uid: u32) -> Option<String> {
        self.with_rows(|r| r.iter().find(|p| p.uid == uid).map(|p| p.name.clone()))
    }
    fn profile_of_gid(&self, gid: u32) -> Option<String> {
        if gid == 0 {
            return None;
        }
        self.with_rows(|r| r.iter().find(|p| p.gid == gid).map(|p| p.name.clone()))
    }
    fn uid_of(&self, profile: &str) -> Option<u32> {
        self.with_rows(|r| r.iter().find(|p| p.name == profile).map(|p| p.uid))
    }
    fn gid_of(&self, profile: &str) -> Option<u32> {
        self.with_rows(|r| {
            r.iter()
                .find(|p| p.name == profile && p.gid != 0)
                .map(|p| p.gid)
        })
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
        self.with_rows(|r| r.iter().any(|p| p.name == name))
    }

    fn is_group(&self, name: &str) -> bool {
        self.with_rows(|r| {
            r.iter()
                .any(|p| (p.name == name && p.gid != 0) || p.groups.iter().any(|g| g == name))
        })
    }

    fn crtobjaud(&self) -> ObjAudit {
        self.access
            .sysval("QCRTOBJAUD")
            .and_then(|v| ObjAudit::parse(v.trim()))
            .unwrap_or(ObjAudit::None)
    }
}
