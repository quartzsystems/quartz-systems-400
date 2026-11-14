// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Compare (RFC 4511 §4.10): compareTrue (6) or compareFalse (5) under
//! the attribute's equality rule, its subtypes and the computed
//! operational attributes included; `c` on the attribute is needed (50).
//! An attribute the entry lacks, or the schema does not define, is
//! noSuchAttribute (16), as IBM's server answers both; one with no
//! equality rule is inappropriateMatching (18).

use ldap::ResultCode;
use store::txn::Txn;

use super::controls::Ctl;
use super::{AclCache, Directory, OpError, OpResult, Place, Session};
use crate::acl;
use crate::backend::DirEntry;
use crate::schema::check::split_description;

impl Directory {
    pub(crate) fn compare(
        &self,
        txn: &mut Txn<'_>,
        s: &Session,
        ctl: &Ctl,
        dn: &str,
        attr: &str,
        value: &[u8],
    ) -> OpResult<ResultCode> {
        let d = Self::parse_dn(dn)?;
        let no_attr = || {
            OpError::new(
                ResultCode::NoSuchAttribute,
                format!("the entry has no {attr}"),
            )
        };
        let at = self
            .schema
            .attribute(split_description(attr).0)
            .ok_or_else(no_attr)?;
        let rule = at
            .equality
            .or_else(|| super::filter::by_syntax(at, crate::schema::rule::Kind::Equality))
            .ok_or_else(|| {
                OpError::new(
                    ResultCode::InappropriateMatching,
                    format!("{} has no equality rule", at.name()),
                )
            })?;
        let (attrs, allowed) = match self.place(&d) {
            Place::RootDse => (self.root_dse(), true),
            Place::Schema => (crate::backend::schema::entry(&self.schema), true),
            Place::Monitor => (
                self.counters.entry(self.now(), &self.settings.release),
                true,
            ),
            Place::UnderMonitor => {
                return Err(OpError::unwilling("cn=monitor has no entries below it"));
            }
            Place::Backend(_) | Place::Outside => {
                let f = self.find_visible(txn, s, &d, ctl.manage)?;
                if let Some(a) = &ctl.assertion {
                    self.assert(txn, s, f.backend, &f.entry, a)?;
                }
                let mut cache = AclCache::default();
                let eff = self.effective_of(txn, &mut cache, f.backend, &f.entry)?;
                let key = self.entry_key(&f.entry.dn);
                let allowed = self.attr_rights(&eff, s, &key, at) & acl::C != 0;
                let name = at.name().to_string();
                let mut attrs = f.entry.attrs.clone();
                let e: &DirEntry = &f.entry;
                attrs.extend(
                    self.computed(txn, f.backend, e, &eff, &|n| n.eq_ignore_ascii_case(&name))?,
                );
                (attrs, allowed)
            }
        };
        if !allowed {
            return Err(OpError::access());
        }
        let vals: Vec<&[u8]> = attrs
            .iter()
            .filter(|a| {
                self.schema
                    .attribute(split_description(&a.ty).0)
                    .is_some_and(|t| self.schema.is_subtype_of(t, at.oid()))
            })
            .flat_map(|a| a.vals.iter().map(Vec::as_slice))
            .collect();
        if vals.is_empty() {
            return Err(no_attr());
        }
        let hit = vals
            .iter()
            .any(|v| rule.equals(&self.schema, v, value) == Some(true));
        Ok(if hit {
            ResultCode::CompareTrue
        } else {
            ResultCode::CompareFalse
        })
    }
}
