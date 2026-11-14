// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `changelog` backend (docs/DIRSRV.md §8): `cn=changelog` over the
//! entry store's change log, one `changeLogEntry` per change
//! (draft-good-ldap-changelog), `changeNumber=N,cn=changelog`, read-only.
//! Present only when `ibm-slapdChangeLogMaxEntries` is set.

use ldap::dn::Dn;
use ldap::filter::Filter;
use ldap::msg::{Attribute, Scope};
use store::txn::Txn;

use super::{Backend, DirEntry, Resolution, Scan};
use crate::ops::clock::gentime;
use crate::ops::error::OpResult;
use crate::ops::session::Session;
use crate::schema::Schema;
use crate::store::{ChangeKind, ChangeRec, DirDb};

/// The suffix.
pub const DN: &str = "cn=changelog";

/// The root entry's ID (change numbers are the others').
pub const ROOT: u64 = u64::MAX;

/// The change log as a backend.
#[derive(Debug, Clone, Copy)]
pub struct ChangelogBackend {
    /// The instance's store.
    pub db: DirDb,
}

fn text(ty: &str, v: &str) -> Attribute {
    Attribute::new(ty, &[v])
}

/// A change record as a `changeLogEntry`'s attributes.
#[must_use]
pub fn attrs_of(n: u64, r: &ChangeRec) -> Vec<Attribute> {
    let kind = match r.kind {
        ChangeKind::Add => "add",
        ChangeKind::Delete => "delete",
        ChangeKind::Modify => "modify",
        ChangeKind::ModDn => "modrdn",
    };
    let mut a = vec![
        Attribute::new("objectClass", &["top", "changeLogEntry"]),
        text("changeNumber", &n.to_string()),
        text("targetDN", &r.target_dn),
        text("changeType", kind),
        text("changeTime", &gentime(r.time.saturating_mul(1000))),
    ];
    if !r.changes.is_empty() {
        a.push(Attribute {
            ty: "changes".into(),
            vals: vec![r.changes.clone()],
        });
    }
    if let Some(n) = &r.new_rdn {
        a.push(text("newRdn", n));
        a.push(text(
            "deleteOldRdn",
            if r.delete_old_rdn { "TRUE" } else { "FALSE" },
        ));
    }
    if let Some(s) = &r.new_superior {
        a.push(text("newSuperior", s));
    }
    if !r.modifier.is_empty() {
        a.push(text("ibm-changeInitiatorsName", &r.modifier));
    }
    a
}

impl ChangelogBackend {
    fn number_of(schema: &Schema, dn: &Dn) -> Option<u64> {
        let rdn = dn.rdn()?;
        let [ava] = rdn.0.as_slice() else { return None };
        let at = schema.attribute(&ava.ty)?;
        if !at.is_named("changeNumber") {
            return None;
        }
        ava.value_str()?.trim().parse().ok()
    }
}

impl Backend for ChangelogBackend {
    fn name(&self) -> &'static str {
        "changelog"
    }

    fn suffixes(&self) -> Vec<Dn> {
        vec![Dn::parse(DN).expect("a DN")]
    }

    fn read_only(&self) -> bool {
        true
    }

    fn resolve(
        &self,
        txn: &mut Txn<'_>,
        schema: &Schema,
        _session: &Session,
        dn: &Dn,
    ) -> OpResult<Resolution> {
        let root = Dn::parse(DN).expect("a DN");
        if schema.dn_eq(dn, &root) {
            return Ok(Resolution {
                entry: Some(ROOT),
                matched: Some(ROOT),
            });
        }
        let found = if dn.len() == 2 {
            Self::number_of(schema, dn).filter(|&n| {
                self.db
                    .log_read(txn, n, 1)
                    .is_ok_and(|v| v.first().is_some_and(|(m, _)| *m == n))
            })
        } else {
            None
        };
        Ok(Resolution {
            entry: found,
            matched: Some(ROOT),
        })
    }

    fn entry(
        &self,
        txn: &mut Txn<'_>,
        _schema: &Schema,
        _session: &Session,
        id: u64,
    ) -> OpResult<Option<DirEntry>> {
        if id == ROOT {
            return Ok(Some(DirEntry {
                id,
                dn: DN.into(),
                attrs: vec![
                    Attribute::new("objectClass", &["top", "container"]),
                    text("cn", "changelog"),
                ],
            }));
        }
        Ok(self
            .db
            .log_read(txn, id, 1)?
            .into_iter()
            .find(|(n, _)| *n == id)
            .map(|(n, r)| DirEntry {
                id: n,
                dn: format!("changeNumber={n},{DN}"),
                attrs: attrs_of(n, &r),
            }))
    }

    fn parent(&self, _txn: &mut Txn<'_>, id: u64) -> OpResult<Option<u64>> {
        Ok((id != ROOT).then_some(ROOT))
    }

    fn children(&self, txn: &mut Txn<'_>, id: u64, cap: usize) -> OpResult<u64> {
        if id != ROOT {
            return Ok(0);
        }
        let a = self.db.attr(txn)?;
        Ok(a.log_count.min(cap as u64))
    }

    fn scan(
        &self,
        txn: &mut Txn<'_>,
        _schema: &Schema,
        _session: &Session,
        base: u64,
        scope: Scope,
        _filter: &Filter,
        state: &mut Scan,
        max: usize,
    ) -> OpResult<Vec<u64>> {
        let (mut from, first) = match std::mem::take(state) {
            Scan::Done => return Ok(Vec::new()),
            Scan::Start => (0u64, true),
            Scan::Going(b) => (*b.downcast::<u64>().expect("a change number"), false),
        };
        let mut out = Vec::new();
        if first && (scope == Scope::Base || scope == Scope::Sub) {
            out.push(base);
        }
        if scope == Scope::Base || base != ROOT {
            *state = Scan::Done;
            return Ok(out);
        }
        let room = max.saturating_sub(out.len()).max(1);
        let recs = self.db.log_read(txn, from, room)?;
        let full = recs.len() == room;
        for (n, _) in recs {
            out.push(n);
            from = n + 1;
        }
        *state = if full {
            Scan::Going(Box::new(from))
        } else {
            Scan::Done
        };
        Ok(out)
    }
}
