// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DN normalization (distinguishedNameMatch): every AVA's type as its
//! OID and its value by the type's equality rule, the AVAs of an RDN in
//! byte order. Equal DNs have equal normalized bytes.
//!
//! The form is `oid=value` per AVA, AVAs joined with `+` and RDNs with
//! `,`; a value is escaped as RFC 4514 §2.4 asks, so the form parses
//! back as a DN.

use ldap::dn::{Ava, Dn, Rdn, escape_value};

use super::prep::{self, Case};
use super::registry::Schema;
use super::rule::Context;

impl Schema {
    /// An AVA's normalized form: `oid=value`. A type the schema does not
    /// know keeps its name, lower-cased, and its value is compared as
    /// caseIgnoreMatch; a type without an equality rule compares octet
    /// by octet.
    #[must_use]
    pub fn normalize_ava(&self, ava: &Ava) -> Vec<u8> {
        let (ty, value) = if let Some(at) = self.attribute(&ava.ty) {
            let v = match at.equality {
                Some(r) => r
                    .normalize(self, &ava.value)
                    .unwrap_or_else(|_| ava.value.clone()),
                None => ava.value.clone(),
            };
            (at.oid().to_string(), v)
        } else {
            let v = prep::prepare(&ava.value, Case::Fold)
                .map_or_else(|_| ava.value.clone(), String::into_bytes);
            (ava.ty.to_ascii_lowercase(), v)
        };
        let mut out = ty.into_bytes();
        out.push(b'=');
        out.extend_from_slice(escape_value(&value).as_bytes());
        out
    }

    /// An RDN's normalized form: its AVAs normalized, in byte order,
    /// joined with `+`. The `names` tree's key (§5).
    #[must_use]
    pub fn normalize_rdn(&self, rdn: &Rdn) -> Vec<u8> {
        let mut avas: Vec<Vec<u8>> = rdn.0.iter().map(|a| self.normalize_ava(a)).collect();
        avas.sort();
        avas.dedup();
        avas.join(&b'+')
    }

    /// A DN's normalized form.
    #[must_use]
    pub fn normalize_parsed_dn(&self, dn: &Dn) -> Vec<u8> {
        let rdns: Vec<Vec<u8>> = dn.0.iter().map(|r| self.normalize_rdn(r)).collect();
        rdns.join(&b',')
    }

    /// Whether two DNs are equal under distinguishedNameMatch.
    #[must_use]
    pub fn dn_eq(&self, a: &Dn, b: &Dn) -> bool {
        a.0.len() == b.0.len()
            && a.0
                .iter()
                .zip(&b.0)
                .all(|(x, y)| self.normalize_rdn(x) == self.normalize_rdn(y))
    }

    /// Whether `dn` is `base` or below it under distinguishedNameMatch.
    #[must_use]
    pub fn dn_within(&self, dn: &Dn, base: &Dn) -> bool {
        let Some(skip) = dn.0.len().checked_sub(base.0.len()) else {
            return false;
        };
        dn.0[skip..]
            .iter()
            .zip(&base.0)
            .all(|(x, y)| self.normalize_rdn(x) == self.normalize_rdn(y))
    }
}

impl Context for Schema {
    fn oid_of(&self, descr: &str) -> Option<String> {
        Schema::oid_of(self, descr)
    }

    fn normalize_dn(&self, dn: &str) -> Option<Vec<u8>> {
        Dn::parse(dn).ok().map(|d| self.normalize_parsed_dn(&d))
    }
}
