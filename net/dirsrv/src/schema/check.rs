// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Entry checking against the schema (`ibm-slapdSchemaCheck`): `V3`
//! strict, `V3_lenient` as IBM ships it. What lenient relaxes is in
//! docs/DIRSRV.md §6.

use ldap::ResultCode;
use ldap::dn::Dn;
use ldap::msg::Attribute;

use super::model::{AttributeType, ObjectClass};
use super::registry::Schema;
use super::syntax;

/// The checking level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    /// `V3`: every superclass listed, exactly one structural chain.
    V3,
    /// `V3_lenient`: superclasses implied, structural rules relaxed.
    #[default]
    V3Lenient,
}

impl Level {
    /// The level an `ibm-slapdSchemaCheck` value names; `V2` and
    /// anything else are refused (`None`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case("V3") {
            Some(Self::V3)
        } else if s.eq_ignore_ascii_case("V3_lenient") {
            Some(Self::V3Lenient)
        } else {
            None
        }
    }
}

/// Why an entry fails the check: the result code and what was wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{what}")]
pub struct CheckError {
    /// The LDAP result code.
    pub code: ResultCode,
    /// Quartz's text for the diagnostic message.
    pub what: String,
}

fn fail(code: ResultCode, what: impl Into<String>) -> CheckError {
    CheckError {
        code,
        what: what.into(),
    }
}

/// What a passing entry resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// Every object class of the entry, listed and implied, by name:
    /// what `objectClass` holds once stored.
    pub object_classes: Vec<String>,
    /// The structural class's name, if there is one
    /// (`structuralObjectClass`).
    pub structural: Option<String>,
}

/// An attribute description split: the type and its options
/// (`cn;binary` is `cn` with `binary`).
#[must_use]
pub fn split_description(d: &str) -> (&str, Vec<&str>) {
    let mut parts = d.split(';');
    let ty = parts.next().unwrap_or("");
    (ty, parts.filter(|p| !p.is_empty()).collect())
}

impl Schema {
    /// The attribute type an attribute description names. Options other
    /// than `binary` are refused (language tags are not built, §14).
    ///
    /// # Errors
    /// 17 undefinedAttributeType for an unknown type; 53
    /// unwillingToPerform for an option.
    pub fn attribute_for(&self, description: &str) -> Result<&AttributeType, CheckError> {
        let (ty, options) = split_description(description);
        let at = self.attribute(ty).ok_or_else(|| {
            fail(
                ResultCode::UndefinedAttributeType,
                format!("attribute type {ty} is not defined"),
            )
        })?;
        if let Some(o) = options.iter().find(|o| !o.eq_ignore_ascii_case("binary")) {
            return Err(fail(
                ResultCode::UnwillingToPerform,
                format!("attribute option {o} is not supported"),
            ));
        }
        Ok(at)
    }

    /// Whether `v` is a valid value of the attribute's syntax and length.
    #[must_use]
    pub fn value_valid(&self, at: &AttributeType, v: &[u8]) -> bool {
        if at.max_length().is_some_and(|m| v.len() > m as usize) {
            return false;
        }
        match at.syntax.as_deref().and_then(syntax::find) {
            Some(s) => s.check.valid(v, &|t| Dn::parse(t).is_ok()),
            None => true,
        }
    }

    /// The values' normalized forms under the attribute's equality rule
    /// (the bytes themselves without one), for duplicate checks and
    /// index keys.
    #[must_use]
    pub fn equality_key(&self, at: &AttributeType, v: &[u8]) -> Vec<u8> {
        match at.equality {
            Some(r) => r.normalize(self, v).unwrap_or_else(|_| v.to_vec()),
            None => v.to_vec(),
        }
    }

    /// Whether a user (not the server) may change this attribute.
    ///
    /// # Errors
    /// 19 constraintViolation for a NO-USER-MODIFICATION attribute.
    pub fn check_user_modification(&self, at: &AttributeType) -> Result<(), CheckError> {
        if at.no_user_modification() {
            Err(fail(
                ResultCode::ConstraintViolation,
                format!("{} cannot be changed by users", at.name()),
            ))
        } else {
            Ok(())
        }
    }

    /// Checks an entry: its classes, required and allowed attributes,
    /// values, single values, duplicates and its RDN.
    ///
    /// # Errors
    /// [`CheckError`] with the result code RFC 4511 gives the failure.
    pub fn check_entry(
        &self,
        dn: &Dn,
        attrs: &[Attribute],
        level: Level,
    ) -> Result<Checked, CheckError> {
        // Every attribute known, values valid, no duplicates.
        let mut present: Vec<(&AttributeType, Vec<Vec<u8>>)> = Vec::new();
        for a in attrs {
            let at = self.attribute_for(&a.ty)?;
            let slot = if let Some(i) = present.iter().position(|(p, _)| p.oid() == at.oid()) {
                i
            } else {
                present.push((at, Vec::new()));
                present.len() - 1
            };
            for v in &a.vals {
                if !self.value_valid(at, v) {
                    return Err(fail(
                        ResultCode::InvalidAttributeSyntax,
                        format!("a value of {} is not valid for its syntax", at.name()),
                    ));
                }
                let k = self.equality_key(at, v);
                if present[slot].1.contains(&k) {
                    return Err(fail(
                        ResultCode::AttributeOrValueExists,
                        format!("{} has the same value twice", at.name()),
                    ));
                }
                present[slot].1.push(k);
            }
        }
        for (at, vals) in &present {
            if vals.is_empty() {
                return Err(fail(
                    ResultCode::ProtocolError,
                    format!("{} has no values", at.name()),
                ));
            }
            if at.single_value() && vals.len() > 1 {
                return Err(fail(
                    ResultCode::ConstraintViolation,
                    format!("{} is single-valued", at.name()),
                ));
            }
        }
        let checked = self.check_classes(attrs, level)?;
        let classes: Vec<&ObjectClass> = checked
            .object_classes
            .iter()
            .filter_map(|c| self.class(c))
            .collect();
        // Required attributes.
        for c in &classes {
            for m in &c.must {
                if !present.iter().any(|(p, _)| p.oid() == m) {
                    let name = self.attribute(m).map_or(m.as_str(), AttributeType::name);
                    return Err(fail(
                        ResultCode::ObjectClassViolation,
                        format!("{} requires {name}", c.name()),
                    ));
                }
            }
        }
        // Allowed attributes.
        let extensible = classes
            .iter()
            .any(|c| c.oid() == "1.3.6.1.4.1.1466.101.120.111");
        if !extensible {
            for (at, _) in &present {
                if at.is_operational() {
                    continue;
                }
                let allowed = classes.iter().any(|c| {
                    c.must
                        .iter()
                        .chain(&c.may)
                        .any(|o| self.is_subtype_of(at, o))
                });
                if !allowed {
                    return Err(fail(
                        ResultCode::ObjectClassViolation,
                        format!("{} is not allowed by the entry's object classes", at.name()),
                    ));
                }
            }
        }
        // The RDN's values are in the entry.
        if let Some(rdn) = dn.rdn() {
            for ava in &rdn.0 {
                let at = self.attribute(&ava.ty).ok_or_else(|| {
                    fail(
                        ResultCode::NamingViolation,
                        format!("RDN type {} is not defined", ava.ty),
                    )
                })?;
                let k = self.equality_key(at, &ava.value);
                if !present
                    .iter()
                    .any(|(p, vals)| p.oid() == at.oid() && vals.contains(&k))
                {
                    return Err(fail(
                        ResultCode::NamingViolation,
                        format!("the RDN's {} value is not in the entry", at.name()),
                    ));
                }
            }
        }
        Ok(checked)
    }

    /// Whether `at` is the attribute `oid` or one of its subtypes.
    #[must_use]
    pub fn is_subtype_of(&self, at: &AttributeType, oid: &str) -> bool {
        let mut cur = Some(at);
        let mut depth = 0;
        while let Some(a) = cur {
            if a.oid() == oid {
                return true;
            }
            depth += 1;
            if depth > 32 {
                return false;
            }
            cur = a.sup.as_deref().and_then(|s| self.attribute(s));
        }
        false
    }

    /// The object class checks alone: every class known, superclasses
    /// listed (V3) or implied (lenient), the structural chain.
    ///
    /// # Errors
    /// 65 objectClassViolation.
    pub fn check_classes(&self, attrs: &[Attribute], level: Level) -> Result<Checked, CheckError> {
        let oc_vals: Vec<&[u8]> = attrs
            .iter()
            .filter(|a| {
                self.attribute(split_description(&a.ty).0)
                    .is_some_and(|t| t.oid() == "2.5.4.0")
            })
            .flat_map(|a| a.vals.iter().map(Vec::as_slice))
            .collect();
        if oc_vals.is_empty() {
            return Err(fail(
                ResultCode::ObjectClassViolation,
                "the entry has no objectClass",
            ));
        }
        let mut listed: Vec<&ObjectClass> = Vec::new();
        for v in oc_vals {
            let name = String::from_utf8_lossy(v);
            let c = self.class(name.trim()).ok_or_else(|| {
                fail(
                    ResultCode::ObjectClassViolation,
                    format!("object class {name} is not defined"),
                )
            })?;
            if !listed.iter().any(|l| l.oid() == c.oid()) {
                listed.push(c);
            }
        }
        let mut all = listed.clone();
        for c in &listed {
            for s in &c.supers {
                if all.iter().any(|a| a.oid() == s) {
                    continue;
                }
                let sc = self.class(s).ok_or_else(|| {
                    fail(
                        ResultCode::ObjectClassViolation,
                        format!("superclass {s} is not defined"),
                    )
                })?;
                // `top` is implied at both levels.
                if level == Level::V3 && sc.oid() != "2.5.6.0" {
                    return Err(fail(
                        ResultCode::ObjectClassViolation,
                        format!("{} needs its superclass {} listed", c.name(), sc.name()),
                    ));
                }
                all.push(sc);
            }
        }
        let tips = self.most_specific_structural(&all);
        if level == Level::V3 {
            if tips.is_empty() {
                return Err(fail(
                    ResultCode::ObjectClassViolation,
                    "the entry has no structural object class",
                ));
            }
            if tips.len() > 1 {
                return Err(fail(
                    ResultCode::ObjectClassViolation,
                    "the entry has more than one structural object class chain",
                ));
            }
        }
        if all.iter().any(|c| c.def.obsolete) && level == Level::V3 {
            let c = all.iter().find(|c| c.def.obsolete).map_or("", |c| c.name());
            return Err(fail(
                ResultCode::ObjectClassViolation,
                format!("object class {c} is obsolete"),
            ));
        }
        let structural = (tips.len() == 1).then(|| tips[0].name().to_string());
        Ok(Checked {
            object_classes: all.iter().map(|c| c.name().to_string()).collect(),
            structural,
        })
    }
}
