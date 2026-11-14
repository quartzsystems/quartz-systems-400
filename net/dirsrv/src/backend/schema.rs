// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `schema` backend (docs/DIRSRV.md §8): `cn=schema`, the
//! subschema entry, read from the instance's schema and changed through
//! it.

use ldap::ResultCode;
use ldap::dn::Dn;
use ldap::msg::{Attribute, Modification};
use ldap::schema::{IbmAttributeType, MatchingRuleUseDesc};

use crate::schema::Schema;
use crate::schema::rule::{self, Kind};

/// The subschema entry's DN.
pub const DN: &str = "cn=schema";

/// Whether `dn` names the subschema entry.
#[must_use]
pub fn is_schema_dn(schema: &Schema, dn: &Dn) -> bool {
    Dn::parse(DN).is_ok_and(|d| schema.dn_eq(&d, dn))
}

fn text_attr(ty: &str, vals: impl IntoIterator<Item = String>) -> Attribute {
    Attribute {
        ty: ty.into(),
        vals: vals.into_iter().map(String::into_bytes).collect(),
    }
}

/// Each matching rule's use: the attribute types it applies to, those
/// that name it or whose syntax is its assertion syntax.
#[must_use]
pub fn matching_rule_uses(schema: &Schema) -> Vec<MatchingRuleUseDesc> {
    schema
        .matching_rules()
        .iter()
        .filter_map(|d| {
            let r = rule::find(&d.oid)?;
            let applies: Vec<String> = schema
                .attributes()
                .filter(|a| {
                    let named = [a.equality, a.ordering, a.substr]
                        .iter()
                        .flatten()
                        .any(|x| x.oid == r.oid);
                    named || (r.kind != Kind::Substring && a.syntax.as_deref() == Some(r.syntax))
                })
                .map(|a| a.name().to_string())
                .collect();
            (!applies.is_empty()).then(|| MatchingRuleUseDesc {
                oid: d.oid.clone(),
                names: d.names.clone(),
                applies,
                ..MatchingRuleUseDesc::default()
            })
        })
        .collect()
}

/// The subschema entry's attributes: what a base search of `cn=schema`
/// returns (operational ones included; the caller selects).
#[must_use]
pub fn entry(schema: &Schema) -> Vec<Attribute> {
    let ibm = schema.attributes().map(|a| {
        let mut i = a.ibm.clone().unwrap_or_else(|| IbmAttributeType {
            oid: a.oid().to_string(),
            ..IbmAttributeType::default()
        });
        i.access_class = Some(a.access.keyword().to_string());
        if i.length.is_none() {
            i.length = a.syntax_len;
        }
        i.to_string()
    });
    vec![
        text_attr(
            "objectClass",
            ["top", "subschema", "ibmSubSchema"].map(String::from),
        ),
        text_attr("cn", ["schema".to_string()]),
        text_attr(
            "attributeTypes",
            schema.attributes().map(|a| a.def.to_string()),
        ),
        text_attr("objectClasses", schema.classes().map(|c| c.def.to_string())),
        text_attr(
            "ldapSyntaxes",
            schema.syntaxes().iter().map(ToString::to_string),
        ),
        text_attr(
            "matchingRules",
            schema.matching_rules().iter().map(ToString::to_string),
        ),
        text_attr(
            "matchingRuleUse",
            matching_rule_uses(schema).iter().map(ToString::to_string),
        ),
        text_attr("IBMAttributeTypes", ibm),
    ]
}

/// A modify of `cn=schema`: applied to `schema` as a whole or not at
/// all. On success the caller writes [`Schema::modified_text`] to the
/// instance's `etc/V3.modifiedschema`.
///
/// # Errors
/// The result code and Quartz's diagnostic text.
pub fn modify(schema: &mut Schema, mods: &[Modification]) -> Result<(), (ResultCode, String)> {
    schema.modify(mods).map_err(|e| (e.code(), e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ldap::msg::ModOp;
    use ldap::schema::SchemaValue;

    #[test]
    fn subschema_entry_parses_back() {
        let s = Schema::shipped().unwrap();
        let e = entry(&s);
        let get = |n: &str| e.iter().find(|a| a.ty == n).unwrap();
        assert_eq!(get("attributeTypes").vals.len(), s.attributes().count());
        for a in &e[2..] {
            for v in &a.vals {
                let t = std::str::from_utf8(v).unwrap();
                SchemaValue::parse(&a.ty, t).unwrap_or_else(|err| panic!("{}: {t}: {err}", a.ty));
            }
        }
        let ibm = get("IBMAttributeTypes");
        assert!(
            ibm.vals
                .iter()
                .any(|v| v.starts_with(b"( 2.5.4.35 ") && v.windows(8).any(|w| w == b"critical"))
        );
        let uses = get("matchingRuleUse");
        assert!(uses.vals.iter().any(|v| v.starts_with(b"( 2.5.13.2 ")));
        assert!(is_schema_dn(&s, &Dn::parse("CN=Schema").unwrap()));
    }

    #[test]
    fn modify_reports_codes() {
        let mut s = Schema::shipped().unwrap();
        let bad = Modification {
            op: ModOp::Add,
            attr: Attribute {
                ty: "attributeTypes".into(),
                vals: vec![b"( 1.2.3 NAME".to_vec()],
            },
        };
        assert_eq!(
            modify(&mut s, &[bad]).unwrap_err().0,
            ResultCode::InvalidAttributeSyntax
        );
    }
}
