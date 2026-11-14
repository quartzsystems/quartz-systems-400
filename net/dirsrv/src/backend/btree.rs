// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `btree` backend (docs/DIRSRV.md §8): the user suffixes,
//! `cn=localhost` and `cn=ibmpolicies` over the entry store (§5).
//!
//! Candidates (§5): a subtree or one-level search uses the indexes when
//! the filter allows (an indexed equality, presence or substring
//! assertion; the smallest list of an `&`; the union of an `|` whose
//! terms are all indexed) and walks the base's subtree through `names`
//! otherwise.

use ldap::dn::{Dn, Rdn};
use ldap::filter::Filter;
use ldap::msg::{Attribute, Scope};
use store::txn::Txn;

use super::{Backend, DirEntry, Resolution, Scan};
use crate::ops::error::OpResult;
use crate::ops::session::Session;
use crate::schema::Schema;
use crate::store::{Attr, ChangeRec, ChildCursor, DirDb, Entry, Normalizer, WalkCursor};

/// The entry store as a backend.
#[derive(Debug, Clone)]
pub struct BtreeBackend {
    /// The instance's store.
    pub db: DirDb,
    suffixes: Vec<Dn>,
    /// Changes go to the store's change log (`cn=changelog` on).
    pub log_changes: bool,
}

enum State {
    Children(Option<ChildCursor>),
    Walk(Option<WalkCursor>),
    Cands(Vec<u64>, usize),
}

/// The store's attributes as LDAP's.
#[must_use]
pub fn to_ldap(attrs: &[Attr]) -> Vec<Attribute> {
    attrs
        .iter()
        .map(|a| Attribute {
            ty: a.name.clone(),
            vals: a.values.clone(),
        })
        .collect()
}

fn to_store(attrs: &[Attribute]) -> Vec<Attr> {
    attrs
        .iter()
        .map(|a| Attr {
            name: a.ty.clone(),
            values: a.vals.clone(),
        })
        .collect()
}

fn union(mut a: Vec<u64>, b: &[u64]) -> Vec<u64> {
    a.extend_from_slice(b);
    a.sort_unstable();
    a.dedup();
    a
}

impl BtreeBackend {
    /// The backend over `db` holding `suffixes`.
    #[must_use]
    pub fn new(db: DirDb, suffixes: Vec<Dn>) -> Self {
        Self {
            db,
            suffixes,
            log_changes: false,
        }
    }

    /// Candidate IDs from the indexes, `None` when the filter needs a
    /// walk.
    fn plan(&self, txn: &mut Txn<'_>, schema: &Schema, f: &Filter) -> OpResult<Option<Vec<u64>>> {
        Ok(match f {
            Filter::Equality(a, v) => {
                let Some(id) = schema.attr_id(a) else {
                    return Ok(None);
                };
                self.db
                    .equal(txn, schema, &id, v, None, usize::MAX)
                    .ok()
                    .flatten()
            }
            Filter::Present(a) => {
                let Some(id) = schema.attr_id(a) else {
                    return Ok(None);
                };
                if id == "objectclass" {
                    return Ok(None);
                }
                self.db.present(txn, &id, None, usize::MAX)?
            }
            Filter::Substrings(a, s) => {
                let Some(id) = schema.attr_id(a) else {
                    return Ok(None);
                };
                let any: Vec<&[u8]> = s.any.iter().map(Vec::as_slice).collect();
                self.db
                    .substring_candidates(
                        txn,
                        schema,
                        &id,
                        s.initial.as_deref(),
                        &any,
                        s.fin.as_deref(),
                    )
                    .ok()
                    .flatten()
            }
            Filter::And(fs) => {
                let mut best: Option<Vec<u64>> = None;
                for g in fs {
                    if let Some(c) = self.plan(txn, schema, g)?
                        && best.as_ref().is_none_or(|b| c.len() < b.len())
                    {
                        best = Some(c);
                    }
                }
                best
            }
            Filter::Or(fs) if !fs.is_empty() => {
                let mut all = Vec::new();
                for g in fs {
                    match self.plan(txn, schema, g)? {
                        Some(c) => all = union(all, &c),
                        None => return Ok(None),
                    }
                }
                Some(all)
            }
            _ => None,
        })
    }

    /// Whether `id` is `base` or below it (one level only when `one`).
    fn in_scope(&self, txn: &mut Txn<'_>, id: u64, base: u64, one: bool) -> OpResult<bool> {
        if one {
            return Ok(self.db.get(txn, id)?.is_some_and(|e| e.parent == base));
        }
        let mut cur = id;
        for _ in 0..10_000 {
            if cur == base {
                return Ok(true);
            }
            match self.db.get(txn, cur)? {
                Some(e) if e.parent != 0 => cur = e.parent,
                _ => return Ok(false),
            }
        }
        Ok(false)
    }
}

impl Backend for BtreeBackend {
    fn name(&self) -> &'static str {
        "btree"
    }

    fn suffixes(&self) -> Vec<Dn> {
        self.suffixes.clone()
    }

    fn resolve(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        _session: &Session,
        dn: &Dn,
    ) -> OpResult<Resolution> {
        let r = self.db.resolve(txn, schema, dn)?;
        Ok(Resolution {
            entry: r.entry,
            matched: r.matched.map(|(id, _)| id),
        })
    }

    fn entry(
        &self,
        txn: &mut Txn<'_>,
        _schema: &Schema,
        _session: &Session,
        id: u64,
    ) -> OpResult<Option<DirEntry>> {
        let Some(e) = self.db.get(txn, id)? else {
            return Ok(None);
        };
        let dn = if e.suffix {
            e.rdn.clone()
        } else {
            self.db.dn_of(txn, id)?
        };
        Ok(Some(DirEntry {
            id,
            dn,
            attrs: to_ldap(&e.attrs),
        }))
    }

    fn parent(&self, txn: &mut Txn<'_>, id: u64) -> OpResult<Option<u64>> {
        Ok(self
            .db
            .get(txn, id)?
            .and_then(|e| (e.parent != 0).then_some(e.parent)))
    }

    fn children(&self, txn: &mut Txn<'_>, id: u64, cap: usize) -> OpResult<u64> {
        Ok(self.db.children(txn, id, None, cap)?.ids.len() as u64)
    }

    fn scan(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        _session: &Session,
        base: u64,
        scope: Scope,
        filter: &Filter,
        state: &mut Scan,
        max: usize,
    ) -> OpResult<Vec<u64>> {
        let mut st = match std::mem::take(state) {
            Scan::Done => return Ok(Vec::new()),
            Scan::Start => match scope {
                Scope::Base => {
                    *state = Scan::Done;
                    return Ok(vec![base]);
                }
                Scope::One | Scope::Sub => match self.plan(txn, schema, filter)? {
                    Some(c) => State::Cands(c, 0),
                    None if scope == Scope::One => State::Children(None),
                    None => State::Walk(None),
                },
            },
            Scan::Going(b) => *b.downcast::<State>().expect("a btree scan state"),
        };
        let mut out = Vec::new();
        let done = match &mut st {
            State::Children(cur) => {
                let p = self.db.children(txn, base, cur.as_ref(), max)?;
                out = p.ids;
                *cur = p.next;
                cur.is_none()
            }
            State::Walk(cur) => {
                let p = self.db.subtree(txn, base, cur.as_ref(), max)?;
                out = p.ids;
                *cur = p.next;
                cur.is_none()
            }
            State::Cands(ids, at) => {
                let one = scope == Scope::One;
                while out.len() < max && *at < ids.len() {
                    let id = ids[*at];
                    *at += 1;
                    if self.in_scope(txn, id, base, one)? {
                        out.push(id);
                    }
                }
                *at >= ids.len()
            }
        };
        *state = if done {
            Scan::Done
        } else {
            Scan::Going(Box::new(st))
        };
        Ok(out)
    }

    fn add(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        parent: Option<u64>,
        dn: &Dn,
        attrs: &[Attribute],
    ) -> OpResult<u64> {
        let rdn = dn.rdn().map(ToString::to_string).unwrap_or_default();
        match parent {
            None => {
                let e = Entry::new(&dn.to_string(), to_store(attrs));
                Ok(self.db.add_suffix(txn, schema, dn, &e)?)
            }
            Some(p) => {
                let e = Entry::new(&rdn, to_store(attrs));
                let r = dn.rdn().expect("a child has an RDN");
                Ok(self.db.add(txn, schema, p, r, &e)?)
            }
        }
    }

    fn replace(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        id: u64,
        attrs: &[Attribute],
    ) -> OpResult<()> {
        let e = Entry::new("", to_store(attrs));
        Ok(self.db.put(txn, schema, id, &e)?)
    }

    fn rename(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        id: u64,
        rdn: &Rdn,
        new_parent: Option<u64>,
    ) -> OpResult<()> {
        Ok(self
            .db
            .rename(txn, schema, id, rdn, &rdn.to_string(), new_parent)?)
    }

    fn delete(&self, txn: &mut Txn<'_>, schema: &Schema, id: u64) -> OpResult<()> {
        Ok(self.db.delete(txn, schema, id)?)
    }

    fn log(&self, txn: &mut Txn<'_>, rec: &ChangeRec) -> OpResult<Option<u64>> {
        if !self.log_changes {
            return Ok(None);
        }
        Ok(Some(self.db.log_append(txn, rec)?))
    }
}
