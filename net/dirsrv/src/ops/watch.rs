// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Persistent search (draft-ietf-ldapext-psearch, `dir_watch` and
//! `dir_changes`): each watch keeps its base, scope, filter and change
//! types; [`Directory::publish`] matches every committed change against
//! every watch, as the watch's session may see it, and queues the entry
//! (with an entry change notification when asked); the job's watcher
//! thread takes them with [`Directory::changes`].

use std::collections::{HashMap, VecDeque};

use ldap::control::{ControlValue, EntryChange, PersistentSearch, change_type};
use ldap::dn::Dn;
use ldap::filter::Filter;
use ldap::msg::{ProtocolOp, Scope};
use store::txn::Txn;

use super::attrs::Wanted;
use super::{AclCache, Change, Directory, Reply, Session};
use crate::backend::DirEntry;
use crate::store::ChangeKind;

/// A watch queues at most this many changes; past it the oldest go.
pub const QUEUE_MAX: usize = 10_000;

/// One persistent search.
pub(crate) struct Watch {
    session: Session,
    base: Dn,
    scope: Scope,
    filter: Filter,
    want: Wanted,
    types_only: bool,
    ps: PersistentSearch,
    queue: VecDeque<Reply>,
}

impl Watch {
    pub(crate) fn new(
        session: Session,
        base: Dn,
        scope: Scope,
        filter: Filter,
        want: Wanted,
        types_only: bool,
        ps: PersistentSearch,
    ) -> Self {
        Self {
            session,
            base,
            scope,
            filter,
            want,
            types_only,
            ps,
            queue: VecDeque::new(),
        }
    }
}

/// The instance's watches.
#[derive(Default)]
pub struct Watches {
    next: u64,
    map: HashMap<u64, Watch>,
}

impl Watches {
    pub(crate) fn add(&mut self, w: Watch) -> u64 {
        self.next += 1;
        self.map.insert(self.next, w);
        self.next
    }

    /// Open watches.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether none is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

fn bit(k: ChangeKind) -> i32 {
    match k {
        ChangeKind::Add => change_type::ADD,
        ChangeKind::Delete => change_type::DELETE,
        ChangeKind::Modify => change_type::MODIFY,
        ChangeKind::ModDn => change_type::MODDN,
    }
}

impl Directory {
    fn in_scope(&self, dn: &Dn, base: &Dn, scope: Scope) -> bool {
        match scope {
            Scope::Base => self.schema.dn_eq(dn, base),
            Scope::One => dn.len() == base.len() + 1 && self.schema.dn_within(dn, base),
            Scope::Sub => self.schema.dn_within(dn, base),
        }
    }

    /// Deliver committed changes to the watches (call after the commit,
    /// in any transaction that sees it).
    pub fn publish(&mut self, txn: &mut Txn<'_>, changes: &[Change]) {
        if self.watches.map.is_empty() {
            return;
        }
        let mut watches = std::mem::take(&mut self.watches);
        for ch in changes {
            let Ok(dn) = Dn::parse(&ch.dn) else { continue };
            let e = DirEntry {
                id: ch.id,
                dn: ch.dn.clone(),
                attrs: ch.attrs.clone(),
            };
            for w in watches.map.values_mut() {
                if w.ps.change_types & bit(ch.kind) == 0 || !self.in_scope(&dn, &w.base, w.scope) {
                    continue;
                }
                let mut cache = AclCache::default();
                let Ok(eff) = self.effective(txn, &mut cache, ch.backend, &e.dn, &e.attrs, ch.parent)
                else {
                    continue;
                };
                let key = self.entry_key(&e.dn);
                let can = |at: &crate::schema::AttributeType| {
                    self.attr_rights(&eff, &w.session, &key, at) & crate::acl::S != 0
                };
                let t = super::filter::Target {
                    dn: &dn,
                    attrs: &e.attrs,
                    can_search: &can,
                };
                if super::filter::eval(&self.schema, &w.filter, &t) != Some(true) {
                    continue;
                }
                let readable = |at: &crate::schema::AttributeType| {
                    self.attr_rights(&eff, &w.session, &key, at) & crate::acl::R != 0
                };
                let attrs = self.select(e.attrs.clone(), &w.want, w.types_only, &readable, false);
                let mut controls = Vec::new();
                if w.ps.return_ecs {
                    controls.push(
                        EntryChange {
                            change_type: bit(ch.kind),
                            previous_dn: ch.previous_dn.clone(),
                            change_number: ch.number.and_then(|n| i64::try_from(n).ok()),
                        }
                        .to_control(false),
                    );
                }
                if w.queue.len() >= QUEUE_MAX {
                    w.queue.pop_front();
                }
                w.queue.push_back(Reply {
                    op: ProtocolOp::SearchResultEntry {
                        dn: e.dn.clone(),
                        attrs,
                    },
                    controls,
                });
                self.counters.psearch_processed += 1;
            }
        }
        self.watches = watches;
    }

    /// `dir_changes`: up to `max` queued entries of watch `id`; `None`
    /// when there is no such watch.
    pub fn changes(&mut self, id: u64, max: usize) -> Option<Vec<Reply>> {
        let w = self.watches.map.get_mut(&id)?;
        let n = max.min(w.queue.len());
        Some(w.queue.drain(..n).collect())
    }

    /// End watch `id` (abandon, the connection's end).
    pub fn close_watch(&mut self, id: u64) {
        self.watches.map.remove(&id);
        self.counters.current_psearches = self.watches.len() as u64;
    }
}
