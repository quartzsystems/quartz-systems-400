// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The store's [`Normalizer`] over the schema's matching rules: index
//! and RDN keys (docs/DIRSRV.md §5.2).

use ldap::{Rdn, ResultCode};

use super::check::split_description;
use super::prep::{self, Case};
use super::registry::Schema;
use super::rule::Imp;
use crate::store::normalize::{NormError, Normalizer, Rule};

impl Normalizer for Schema {
    /// The attribute's primary name, lower-cased (`cn` for `commonName`,
    /// `2.5.4.3` and `CN;binary`).
    fn attr_id(&self, description: &str) -> Option<String> {
        let (ty, _) = split_description(description);
        self.attribute(ty.trim())
            .map(|a| a.name().to_ascii_lowercase())
    }

    fn rdn_key(&self, rdn: &Rdn) -> Result<Vec<u8>, NormError> {
        if rdn.0.is_empty() {
            return Err(NormError::new(
                ResultCode::InvalidDnSyntax,
                "the RDN is empty",
            ));
        }
        for ava in &rdn.0 {
            let at = self.attribute(&ava.ty).ok_or_else(|| {
                NormError::new(
                    ResultCode::InvalidDnSyntax,
                    format!("RDN type {} is not defined", ava.ty),
                )
            })?;
            if !self.value_valid(at, &ava.value) {
                return Err(NormError::new(
                    ResultCode::InvalidDnSyntax,
                    format!("the RDN's {} value is not valid", at.name()),
                ));
            }
            if let Some(r) = at.equality {
                r.normalize(self, &ava.value)
                    .map_err(|e| NormError::new(ResultCode::InvalidDnSyntax, e.to_string()))?;
            }
        }
        Ok(self.normalize_rdn(rdn))
    }

    /// Equality keys are the equality rule's normalized value. Substring
    /// keys are the value prepared by the substring rule without spaces
    /// at its ends, so the grams of a filter piece prepared the same way
    /// are always among the value's when the piece matches.
    fn value_key(
        &self,
        attr: &str,
        value: &[u8],
        rule: Rule,
    ) -> Result<Option<Vec<u8>>, NormError> {
        let at = self.attribute(attr).ok_or_else(|| {
            NormError::new(
                ResultCode::UndefinedAttributeType,
                format!("attribute type {attr} is not defined"),
            )
        })?;
        let bad = |e: String| NormError::new(ResultCode::InvalidAttributeSyntax, e);
        match rule {
            Rule::Equality => match at.equality {
                Some(r) => r
                    .normalize(self, value)
                    .map(Some)
                    .map_err(|e| bad(e.to_string())),
                None => Ok(None),
            },
            Rule::Substring => {
                let Some(r) = at.substr else { return Ok(None) };
                let case = match r.imp {
                    Imp::CaseExact | Imp::Numeric => Case::Exact,
                    _ => Case::Fold,
                };
                let lines: Vec<u8> = if r.imp == Imp::CaseIgnoreList {
                    value
                        .iter()
                        .map(|&c| if c == b'$' { b' ' } else { c })
                        .collect()
                } else {
                    value.to_vec()
                };
                let t = prep::prepare(&lines, case).map_err(|e| bad(e.to_string()))?;
                let t: String = match r.imp {
                    Imp::Numeric => t.chars().filter(|c| *c != ' ').collect(),
                    Imp::Telephone => t.chars().filter(|c| *c != ' ' && *c != '-').collect(),
                    _ => t,
                };
                Ok(Some(t.into_bytes()))
            }
        }
    }
}
