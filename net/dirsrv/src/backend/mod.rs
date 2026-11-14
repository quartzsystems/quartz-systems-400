// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The backends (docs/DIRSRV.md §8) and what the operations ask of them
//! ([`Backend`]). Access control, schema checks, operational attributes
//! and filters are the operations'; a backend keeps entries, finds them
//! and lists candidates.

use std::any::Any;

use ldap::dn::{Dn, Rdn};
use ldap::filter::Filter;
use ldap::msg::{Attribute, Modification, Scope};
use store::txn::Txn;

use crate::ops::error::{OpError, OpResult};
use crate::ops::session::Session;
use crate::schema::Schema;
use crate::store::ChangeRec;

pub mod btree;
pub mod changelog;
pub mod config;
pub mod monitor;
pub mod projected;
pub mod schema;

/// An entry as a backend gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The backend's ID for it.
    pub id: u64,
    /// Its DN as written.
    pub dn: String,
    /// Its stored attributes, operational ones included.
    pub attrs: Vec<Attribute>,
}

impl DirEntry {
    /// The values of the first attribute named `name` (ASCII case
    /// ignored, no options).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Attribute> {
        self.attrs.iter().find(|a| a.ty.eq_ignore_ascii_case(name))
    }
}

/// What a DN resolved to in a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Resolution {
    /// The entry, when it exists.
    pub entry: Option<u64>,
    /// The deepest existing entry above it (the matched DN of a
    /// `noSuchObject`), when the DN is under a stored suffix.
    pub matched: Option<u64>,
}

/// Where a candidate listing stopped.
#[derive(Default)]
pub enum Scan {
    /// Not started.
    #[default]
    Start,
    /// Under way: the backend's own position.
    Going(Box<dyn Any + Send>),
    /// Every candidate given.
    Done,
}

impl std::fmt::Debug for Scan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Start => "Start",
            Self::Going(_) => "Going",
            Self::Done => "Done",
        })
    }
}

fn unwilling() -> OpError {
    OpError::unwilling("this part of the directory is read-only")
}

/// One backend: the entry store's, the projected one (the service's),
/// the change log, the configuration. Every call runs in the caller's
/// store transaction.
///
/// A backend that hides an entry from `session` (the projected one,
/// for anonymous users) resolves it but answers `None` from
/// [`entry`](Self::entry) and leaves it out of [`scan`](Self::scan): a
/// search then succeeds with no entries, as IBM's does.
pub trait Backend: Send {
    /// A short name for logs (`btree`, `projected`, ...).
    fn name(&self) -> &str;

    /// The suffixes it holds.
    fn suffixes(&self) -> Vec<Dn>;

    /// Writes are refused (53).
    fn read_only(&self) -> bool {
        false
    }

    /// Only administrators may use it at all (50 for others).
    fn admin_only(&self) -> bool {
        false
    }

    /// Find `dn`, which is at or below one of [`suffixes`](Self::suffixes).
    ///
    /// # Errors
    /// The store's, or a DN the schema refuses.
    fn resolve(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        session: &Session,
        dn: &Dn,
    ) -> OpResult<Resolution>;

    /// Entry `id`, `None` when it is gone or hidden from `session`.
    ///
    /// # Errors
    /// The store's.
    fn entry(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        session: &Session,
        id: u64,
    ) -> OpResult<Option<DirEntry>>;

    /// The parent of `id`, `None` for a suffix's root.
    ///
    /// # Errors
    /// The store's.
    fn parent(&self, txn: &mut Txn<'_>, id: u64) -> OpResult<Option<u64>>;

    /// How many children `id` has, counting at most `cap`.
    ///
    /// # Errors
    /// The store's.
    fn children(&self, txn: &mut Txn<'_>, id: u64, cap: usize) -> OpResult<u64>;

    /// Up to `max` candidates for a search of `base` with `scope`,
    /// continuing from `state`; [`Scan::Done`] after the last. Candidates
    /// may be a superset of the matches (the operations evaluate the
    /// filter); in `scope` they must be.
    ///
    /// # Errors
    /// The store's.
    #[allow(clippy::too_many_arguments)]
    fn scan(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        session: &Session,
        base: u64,
        scope: Scope,
        filter: &Filter,
        state: &mut Scan,
        max: usize,
    ) -> OpResult<Vec<u64>>;

    /// Add an entry under `parent` (`None`: `dn` is a suffix's root).
    ///
    /// # Errors
    /// 53 by default; the store's.
    fn add(
        &self,
        _txn: &mut Txn<'_>,
        _schema: &Schema,
        _parent: Option<u64>,
        _dn: &Dn,
        _attrs: &[Attribute],
    ) -> OpResult<u64> {
        Err(unwilling())
    }

    /// Replace the attributes of `id`.
    ///
    /// # Errors
    /// 53 by default; the store's.
    fn replace(
        &self,
        _txn: &mut Txn<'_>,
        _schema: &Schema,
        _id: u64,
        _attrs: &[Attribute],
    ) -> OpResult<()> {
        Err(unwilling())
    }

    /// Rename `id` to `rdn`, and move it under `new_parent`.
    ///
    /// # Errors
    /// 53 by default; the store's.
    fn rename(
        &self,
        _txn: &mut Txn<'_>,
        _schema: &Schema,
        _id: u64,
        _rdn: &Rdn,
        _new_parent: Option<u64>,
    ) -> OpResult<()> {
        Err(unwilling())
    }

    /// Delete `id`, a leaf.
    ///
    /// # Errors
    /// 53 by default; the store's.
    fn delete(&self, _txn: &mut Txn<'_>, _schema: &Schema, _id: u64) -> OpResult<()> {
        Err(unwilling())
    }

    /// A modify the backend applies itself (the configuration file), or
    /// `None` for the operations to apply it and [`replace`](Self::replace)
    /// the result.
    fn apply(
        &self,
        _txn: &mut Txn<'_>,
        _id: u64,
        _mods: &[Modification],
    ) -> Option<OpResult<()>> {
        None
    }

    /// Record a change in the change log, if this backend keeps one;
    /// its change number.
    ///
    /// # Errors
    /// The store's.
    fn log(&self, _txn: &mut Txn<'_>, _rec: &ChangeRec) -> OpResult<Option<u64>> {
        Ok(None)
    }
}
