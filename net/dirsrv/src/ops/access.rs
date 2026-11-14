// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ACL and owners in force on an entry (docs/DIRSRV.md §12): its own,
//! or those of the nearest ancestor whose values propagate, or the
//! defaults (`cn=anybody` reads, searches and compares the normal,
//! system and restricted classes; the administrator owns).

use ldap::msg::Attribute;
use store::txn::Txn;

use super::{AclCache, Directory, OpResult};
use crate::acl::{self, AclEntry, Effective, Subject};

/// What an entry's children inherit: the values and the entry they come
/// from; `None` for the defaults.
#[derive(Debug, Clone, Default)]
pub(crate) struct Inherit {
    acl: Option<(Vec<AclEntry>, String)>,
    owners: Option<(Vec<Subject>, String)>,
}

fn get<'a>(attrs: &'a [Attribute], name: &str) -> Option<&'a Attribute> {
    attrs
        .iter()
        .find(|a| a.ty.eq_ignore_ascii_case(name) && !a.vals.is_empty())
}

fn flag(attrs: &[Attribute], name: &str) -> bool {
    get(attrs, name)
        .and_then(|a| a.vals.first())
        .is_none_or(|v| !v.eq_ignore_ascii_case(b"FALSE"))
}

impl Directory {
    /// What the children of entry `id` in backend `b` inherit.
    pub(crate) fn inherit_of(
        &self,
        txn: &mut Txn<'_>,
        cache: &mut AclCache,
        b: usize,
        id: u64,
    ) -> OpResult<Inherit> {
        if let Some(i) = cache.inherit.get(&(b, id)) {
            return Ok(i.clone());
        }
        let backend = &self.backends[b];
        let Some(e) = backend.entry(txn, &self.schema, &self.sys, id)? else {
            return Ok(Inherit::default());
        };
        let own_acl = get(&e.attrs, "aclEntry")
            .filter(|_| flag(&e.attrs, "aclPropagate"))
            .map(|a| (self.parse_acls(Some(a)), e.dn.clone()));
        let own_owners = get(&e.attrs, "entryOwner")
            .filter(|_| flag(&e.attrs, "ownerPropagate"))
            .map(|a| (self.parse_owners(Some(a)), e.dn.clone()));
        let up = if own_acl.is_some() && own_owners.is_some() {
            Inherit::default()
        } else {
            match backend.parent(txn, id)? {
                Some(p) => self.inherit_of(txn, cache, b, p)?,
                None => Inherit::default(),
            }
        };
        let i = Inherit {
            acl: own_acl.or(up.acl),
            owners: own_owners.or(up.owners),
        };
        cache.inherit.insert((b, id), i.clone());
        Ok(i)
    }

    /// The ACL and owners in force on an entry with `attrs` (its stored
    /// attributes) under `parent` in backend `b`.
    pub(crate) fn effective(
        &self,
        txn: &mut Txn<'_>,
        cache: &mut AclCache,
        b: usize,
        dn: &str,
        attrs: &[Attribute],
        parent: Option<u64>,
    ) -> OpResult<Effective> {
        let own_acl = get(attrs, "aclEntry");
        let own_owner = get(attrs, "entryOwner");
        let up = if own_acl.is_some() && own_owner.is_some() {
            Inherit::default()
        } else if let Some(p) = parent {
            self.inherit_of(txn, cache, b, p)?
        } else {
            Inherit::default()
        };
        let (acl, acl_source, acl_propagate) = match own_acl {
            Some(a) => (self.parse_acls(Some(a)), Some(dn.to_string()), flag(attrs, "aclPropagate")),
            None => match up.acl {
                Some((v, src)) => (v, Some(src), true),
                None => (self.default_acl(), None, true),
            },
        };
        let (owners, owner_source, owner_propagate) = match own_owner {
            Some(a) => (
                self.parse_owners(Some(a)),
                Some(dn.to_string()),
                flag(attrs, "ownerPropagate"),
            ),
            None => match up.owners {
                Some((v, src)) => (v, Some(src), true),
                None => (self.default_owners(), None, true),
            },
        };
        Ok(Effective {
            acl,
            acl_source,
            acl_propagate,
            owners,
            owner_source,
            owner_propagate,
        })
    }

    /// The effective ACL of an entry found by an operation.
    pub(crate) fn effective_of(
        &self,
        txn: &mut Txn<'_>,
        cache: &mut AclCache,
        b: usize,
        e: &crate::backend::DirEntry,
    ) -> OpResult<Effective> {
        let parent = self.backends[b].parent(txn, e.id)?;
        self.effective(txn, cache, b, &e.dn, &e.attrs, parent)
    }

    pub(crate) fn default_acl(&self) -> Vec<AclEntry> {
        vec![acl::model::parse_acl(&self.schema, acl::DEFAULT_ACL).expect("the default ACL")]
    }

    pub(crate) fn default_owners(&self) -> Vec<Subject> {
        acl::model::parse_owner(&self.schema, &format!("access-id:{}", self.settings.admin_dn))
            .into_iter()
            .collect()
    }
}
