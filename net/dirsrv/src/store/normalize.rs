// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the store asks of the schema (docs/DIRSRV.md §5.2): names and
//! values as matching rules see them. `schema` implements it; tests use
//! [`AsciiFold`].

use ldap::{Rdn, ResultCode};

/// Which matching rule a value is normalized under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rule {
    /// The attribute's equality rule.
    Equality,
    /// The attribute's substrings rule (also applied to a filter's
    /// initial, any and final pieces).
    Substring,
}

/// A name or value the schema refuses, with the result code the
/// operation answers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{text}")]
pub struct NormError {
    /// `invalidDNSyntax`, `invalidAttributeSyntax`,
    /// `undefinedAttributeType`, ...
    pub code: ResultCode,
    /// Quartz's text for the diagnostic message.
    pub text: String,
}

impl NormError {
    /// A refusal with `code`.
    #[must_use]
    pub fn new(code: ResultCode, text: impl Into<String>) -> Self {
        Self {
            code,
            text: text.into(),
        }
    }
}

/// Normalization by the schema's matching rules. Equal names and values
/// must give equal bytes, unequal ones unequal bytes; the store compares
/// nothing else.
pub trait Normalizer {
    /// The attribute an attribute description names, as the store keeps
    /// it in index definitions: one spelling for every alias and OID of
    /// the type, options dropped (`commonName;lang-de` and `2.5.4.3`
    /// both give `cn`). `None` for a type the schema does not know.
    fn attr_id(&self, description: &str) -> Option<String>;

    /// An RDN's key: its AVAs in a canonical order, each type by
    /// [`attr_id`](Self::attr_id) and each value by its equality rule.
    /// Two RDNs that name the same entry give the same key.
    ///
    /// # Errors
    ///
    /// An unknown type or a value its syntax refuses.
    fn rdn_key(&self, rdn: &Rdn) -> Result<Vec<u8>, NormError>;

    /// `value` of attribute `attr` (an [`attr_id`](Self::attr_id))
    /// normalized under `rule`; `None` when the attribute has no such
    /// rule (it is then not indexed that way).
    ///
    /// # Errors
    ///
    /// A value the attribute's syntax refuses.
    fn value_key(&self, attr: &str, value: &[u8], rule: Rule)
    -> Result<Option<Vec<u8>>, NormError>;
}

/// A normalizer for tests: attribute names and values ASCII
/// case-folded, runs of spaces collapsed and trimmed, every attribute
/// with both rules.
#[derive(Debug, Clone, Copy, Default)]
pub struct AsciiFold;

impl AsciiFold {
    fn fold(v: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(v.len());
        for &b in v {
            if b == b' ' {
                if out.last().is_some_and(|&l| l != b' ') {
                    out.push(b' ');
                }
            } else {
                out.push(b.to_ascii_lowercase());
            }
        }
        if out.last() == Some(&b' ') {
            out.pop();
        }
        out
    }
}

impl Normalizer for AsciiFold {
    fn attr_id(&self, description: &str) -> Option<String> {
        let ty = description.split(';').next().unwrap_or("").trim();
        (!ty.is_empty()).then(|| ty.to_ascii_lowercase())
    }

    fn rdn_key(&self, rdn: &Rdn) -> Result<Vec<u8>, NormError> {
        if rdn.0.is_empty() {
            return Err(NormError::new(ResultCode::InvalidDnSyntax, "empty RDN"));
        }
        let mut avas = Vec::with_capacity(rdn.0.len());
        for a in &rdn.0 {
            let ty = self
                .attr_id(&a.ty)
                .ok_or_else(|| NormError::new(ResultCode::InvalidDnSyntax, "RDN without a type"))?;
            avas.push((ty, Self::fold(&a.value)));
        }
        avas.sort();
        let mut out = Vec::new();
        for (i, (ty, v)) in avas.iter().enumerate() {
            if i > 0 {
                out.push(b'+');
            }
            out.extend_from_slice(ty.as_bytes());
            out.push(b'=');
            out.extend_from_slice(v);
        }
        Ok(out)
    }

    fn value_key(
        &self,
        _attr: &str,
        value: &[u8],
        _rule: Rule,
    ) -> Result<Option<Vec<u8>>, NormError> {
        Ok(Some(Self::fold(value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ldap::Ava;

    fn rdn(pairs: &[(&str, &str)]) -> Rdn {
        Rdn(pairs
            .iter()
            .map(|(t, v)| Ava {
                ty: (*t).into(),
                value: v.as_bytes().to_vec(),
            })
            .collect())
    }

    #[test]
    fn folds_names_and_values() {
        let n = AsciiFold;
        assert_eq!(n.attr_id("CN;lang-de").as_deref(), Some("cn"));
        assert_eq!(
            n.rdn_key(&rdn(&[("CN", "  John   Smith ")])).unwrap(),
            b"cn=john smith"
        );
        assert_eq!(
            n.rdn_key(&rdn(&[("uid", "a"), ("cn", "B")])).unwrap(),
            n.rdn_key(&rdn(&[("CN", "b"), ("UID", "A")])).unwrap()
        );
        assert!(n.rdn_key(&Rdn(Vec::new())).is_err());
        assert_eq!(
            n.value_key("cn", b"ABC", Rule::Substring).unwrap().unwrap(),
            b"abc"
        );
    }
}
