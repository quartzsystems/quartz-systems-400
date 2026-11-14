// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped schema against the primary's `cn=schema`, entry checking,
//! DN normalization, schema changes and the store's keys.

use std::collections::HashMap;

use dirsrv::schema::{AccessClass, Level, Schema};
use dirsrv::store::normalize::{Normalizer, Rule};
use ldap::ResultCode;
use ldap::dn::Dn;
use ldap::msg::{Attribute, ModOp, Modification};
use ldap::schema::{AttributeTypeDesc, ObjectClassDesc, SchemaValue};

fn shipped() -> Schema {
    Schema::shipped().expect("the shipped schema loads")
}

fn primary() -> Vec<SchemaValue> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/conformance/oracle/dirsrv/schema-primary.txt"
    );
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (a, v) = l.split_once(": ").unwrap();
            SchemaValue::parse(a, v).unwrap_or_else(|e| panic!("{l}: {e}"))
        })
        .collect()
}

#[test]
fn includes_every_primary_definition() {
    let s = shipped();
    let defs = primary();
    let ats: Vec<&AttributeTypeDesc> = defs
        .iter()
        .filter_map(|d| {
            if let SchemaValue::AttributeType(a) = d {
                Some(a)
            } else {
                None
            }
        })
        .collect();
    let ocs: Vec<&ObjectClassDesc> = defs
        .iter()
        .filter_map(|d| {
            if let SchemaValue::ObjectClass(c) = d {
                Some(c)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(ats.len(), 1627);
    assert_eq!(ocs.len(), 428);
    let by_oid: HashMap<&str, &AttributeTypeDesc> =
        ats.iter().map(|a| (a.oid.as_str(), *a)).collect();
    let by_name: HashMap<String, &AttributeTypeDesc> = ats
        .iter()
        .flat_map(|a| a.names.iter().map(move |n| (n.to_ascii_lowercase(), *a)))
        .collect();
    // The primary's syntax, own or by its supertype.
    let syntax_of = |a: &AttributeTypeDesc| {
        let mut cur = a;
        for _ in 0..16 {
            if let Some(s) = &cur.syntax {
                return Some(s.clone());
            }
            let sup = cur.sup.as_deref()?;
            cur = by_oid
                .get(sup)
                .copied()
                .or_else(|| by_name.get(&sup.to_ascii_lowercase()).copied())?;
        }
        None
    };
    for a in &ats {
        let ours = s
            .attribute(&a.oid)
            .unwrap_or_else(|| panic!("attribute {} missing", a.oid));
        for n in &a.names {
            let named = s
                .attribute(n)
                .unwrap_or_else(|| panic!("attribute name {n} missing"));
            assert_eq!(named.oid(), a.oid, "{n}");
        }
        assert_eq!(ours.syntax, syntax_of(a), "syntax of {}", a.oid);
        assert_eq!(
            ours.single_value(),
            a.single_value,
            "single value of {}",
            a.oid
        );
    }
    for c in &ocs {
        let ours = s
            .class(&c.oid)
            .unwrap_or_else(|| panic!("class {} missing", c.oid));
        assert_eq!(ours.kind(), c.kind, "{}", c.oid);
        for n in &c.names {
            assert_eq!(
                s.class(n).map(dirsrv::schema::ObjectClass::oid),
                Some(c.oid.as_str()),
                "{n}"
            );
        }
    }
    // Every syntax and matching rule the primary lists is known.
    for d in &defs {
        match d {
            SchemaValue::Syntax(x) => assert!(
                s.syntaxes().iter().any(|y| y.oid == x.oid),
                "syntax {}",
                x.oid
            ),
            SchemaValue::MatchingRule(x) => {
                assert!(
                    s.matching_rules().iter().any(|y| y.oid == x.oid),
                    "rule {}",
                    x.oid
                );
            }
            _ => {}
        }
    }
}

#[test]
fn rfc_additions_and_access_classes() {
    let s = shipped();
    for n in [
        "entryUUID",
        "structuralObjectClass",
        "governingStructureRule",
        "uidNumber",
        "memberUid",
        "carLicense",
    ] {
        assert!(s.attribute(n).is_some(), "{n}");
    }
    for n in [
        "uidObject",
        "posixAccount",
        "inetOrgPerson",
        "os400-usrprf",
        "os400-root",
        "pwdPolicy",
    ] {
        assert!(s.class(n).is_some(), "{n}");
    }
    assert_eq!(
        s.attribute("userPassword").unwrap().access,
        AccessClass::Critical
    );
    assert_eq!(
        s.attribute("aclEntry").unwrap().access,
        AccessClass::Restricted
    );
    assert_eq!(
        s.attribute("creatorsName").unwrap().access,
        AccessClass::System
    );
    assert_eq!(s.attribute("cn").unwrap().access, AccessClass::Normal);
    // Inheritance: cn takes name's rules.
    let cn = s.attribute("commonName").unwrap();
    assert_eq!(cn.equality.map(|r| r.name), Some("caseIgnoreMatch"));
    assert_eq!(cn.syntax.as_deref(), Some("1.3.6.1.4.1.1466.115.121.1.15"));
    let person = s.class("inetOrgPerson").unwrap();
    assert!(person.supers.iter().any(|o| o == "2.5.6.6"));
    assert!(person.must.iter().any(|o| o == "2.5.4.4"));
    assert!(s.attribute("createTimestamp").unwrap().is_operational());
    assert!(
        s.attribute("createTimestamp")
            .unwrap()
            .no_user_modification()
    );
}

fn dn(s: &str) -> Dn {
    Dn::parse(s).unwrap()
}

#[test]
fn dn_normalization() {
    let s = shipped();
    let a = s.normalize_parsed_dn(&dn("cn=Foo  Bar,DC=Example, dc=COM"));
    let b = s.normalize_parsed_dn(&dn("cn=foo bar,dc=example,dc=com"));
    assert_eq!(a, b);
    assert_eq!(
        String::from_utf8(a).unwrap(),
        "2.5.4.3=foo bar,0.9.2342.19200300.100.1.25=example,0.9.2342.19200300.100.1.25=com"
    );
    assert_eq!(
        s.normalize_parsed_dn(&dn("commonName=A+uid=B,o=X")),
        s.normalize_parsed_dn(&dn("UID=b+2.5.4.3=a,O=x"))
    );
    assert_ne!(
        s.normalize_parsed_dn(&dn("cn=a,o=x")),
        s.normalize_parsed_dn(&dn("cn=b,o=x"))
    );
    // caseExact types keep their case.
    assert_ne!(
        s.normalize_parsed_dn(&dn("dnsRecord=A")),
        s.normalize_parsed_dn(&dn("dnsRecord=a"))
    );
    assert!(s.dn_within(&dn("cn=a,OU=People,dc=x"), &dn("ou=people, DC=X")));
    assert!(!s.dn_within(&dn("ou=people,dc=x"), &dn("cn=a,ou=people,dc=x")));
    // distinguishedNameMatch on a DN-valued attribute.
    let member = s.attribute("member").unwrap();
    let r = member.equality.unwrap();
    assert_eq!(
        r.equals(&s, b"CN=Bob, O=Acme", b"cn=bob,o=acme"),
        Some(true)
    );
    let um = s.attribute("uniqueMember").unwrap().equality.unwrap();
    assert_eq!(
        um.equals(&s, b"CN=Bob, O=Acme#'0101'B", b"cn=bob,o=acme#'0101'B"),
        Some(true)
    );
}

fn attr(ty: &str, vals: &[&str]) -> Attribute {
    Attribute {
        ty: ty.into(),
        vals: vals.iter().map(|v| v.as_bytes().to_vec()).collect(),
    }
}

fn person(extra: &[Attribute], classes: &[&str]) -> Vec<Attribute> {
    let mut v = vec![
        attr("objectClass", classes),
        attr("cn", &["Bob Smith"]),
        attr("sn", &["Smith"]),
    ];
    v.extend_from_slice(extra);
    v
}

#[test]
#[allow(clippy::too_many_lines)]
fn entry_checking() {
    let s = shipped();
    let d = dn("cn=Bob Smith,o=Acme");
    let full = ["top", "person", "organizationalPerson", "inetOrgPerson"];
    let ok = s
        .check_entry(
            &d,
            &person(&[attr("mail", &["bob@acme.example"])], &full),
            Level::V3,
        )
        .unwrap();
    assert_eq!(ok.structural.as_deref(), Some("inetOrgPerson"));
    // Lenient implies the superclasses; strict wants them listed.
    let short = person(&[], &["inetOrgPerson"]);
    let c = s.check_entry(&d, &short, Level::V3Lenient).unwrap();
    assert!(c.object_classes.iter().any(|n| n == "person"));
    assert_eq!(
        s.check_entry(&d, &short, Level::V3).unwrap_err().code,
        ResultCode::ObjectClassViolation
    );
    // top alone is implied at both levels.
    assert!(
        s.check_entry(&d, &person(&[], &["person"]), Level::V3)
            .is_ok()
    );
    // Required attribute missing.
    let no_sn = vec![attr("objectClass", &["person"]), attr("cn", &["Bob Smith"])];
    assert_eq!(
        s.check_entry(&d, &no_sn, Level::V3Lenient)
            .unwrap_err()
            .code,
        ResultCode::ObjectClassViolation
    );
    // Not allowed, unless extensibleObject.
    let extra = person(&[attr("uidNumber", &["5"])], &["person"]);
    assert_eq!(
        s.check_entry(&d, &extra, Level::V3Lenient)
            .unwrap_err()
            .code,
        ResultCode::ObjectClassViolation
    );
    let ext = person(
        &[attr("uidNumber", &["5"])],
        &["person", "extensibleObject"],
    );
    assert!(s.check_entry(&d, &ext, Level::V3Lenient).is_ok());
    // Operational attributes are always allowed.
    let op = person(
        &[attr("aclEntry", &["group:cn=anybody:normal:rsc"])],
        &["person"],
    );
    assert!(s.check_entry(&d, &op, Level::V3).is_ok());
    // Unknown attribute type and class.
    let unk = person(&[attr("noSuchAttr", &["x"])], &["person"]);
    assert_eq!(
        s.check_entry(&d, &unk, Level::V3Lenient).unwrap_err().code,
        ResultCode::UndefinedAttributeType
    );
    assert_eq!(
        s.check_entry(
            &d,
            &person(&[], &["person", "noSuchClass"]),
            Level::V3Lenient
        )
        .unwrap_err()
        .code,
        ResultCode::ObjectClassViolation
    );
    // Syntax, single value, duplicates.
    let bad = person(&[attr("uidNumber", &["five"])], &["person", "posixAccount"]);
    assert_eq!(
        s.check_entry(&d, &bad, Level::V3Lenient).unwrap_err().code,
        ResultCode::InvalidAttributeSyntax
    );
    let two = person(
        &[
            attr("displayName", &["a", "b"]),
            attr("preferredLanguage", &["en", "fr"]),
        ],
        &["inetOrgPerson"],
    );
    assert_eq!(
        s.check_entry(&d, &two, Level::V3Lenient).unwrap_err().code,
        ResultCode::ConstraintViolation
    );
    let dup = person(&[attr("description", &["Same", "  same "])], &["person"]);
    assert_eq!(
        s.check_entry(&d, &dup, Level::V3Lenient).unwrap_err().code,
        ResultCode::AttributeOrValueExists
    );
    // Two structural chains: strict refuses, lenient takes.
    let two_chains = person(&[attr("ou", &["x"])], &["person", "organizationalUnit"]);
    assert_eq!(
        s.check_entry(&d, &two_chains, Level::V3).unwrap_err().code,
        ResultCode::ObjectClassViolation
    );
    assert!(s.check_entry(&d, &two_chains, Level::V3Lenient).is_ok());
    // The RDN's value must be in the entry.
    let other = dn("cn=Alice,o=Acme");
    assert_eq!(
        s.check_entry(&other, &person(&[], &["person"]), Level::V3)
            .unwrap_err()
            .code,
        ResultCode::NamingViolation
    );
    assert!(
        s.check_entry(
            &dn("CN=bob   smith,o=Acme"),
            &person(&[], &["person"]),
            Level::V3
        )
        .is_ok()
    );
    // Language tags are refused; binary is taken.
    assert_eq!(
        s.attribute_for("cn;lang-en").unwrap_err().code,
        ResultCode::UnwillingToPerform
    );
    assert!(s.attribute_for("userCertificate;binary").is_ok());
    assert_eq!(
        s.check_user_modification(s.attribute("createTimestamp").unwrap())
            .unwrap_err()
            .code,
        ResultCode::ConstraintViolation
    );
    assert_eq!(Level::parse("V2"), None);
}

fn m(op: ModOp, ty: &str, vals: &[&str]) -> Modification {
    Modification {
        op,
        attr: attr(ty, vals),
    }
}

#[test]
fn schema_modification() {
    let mut s = shipped();
    let at = "( 1.3.6.1.4.1.99999.1.1 NAME 'qsTestColour' DESC 'A colour' EQUALITY caseIgnoreMatch SYNTAX 1.3.6.1.4.1.1466.115.121.1.15 SINGLE-VALUE )";
    let oc = "( 1.3.6.1.4.1.99999.2.1 NAME 'qsTestThing' SUP top AUXILIARY MAY qsTestColour )";
    s.modify(&[
        m(ModOp::Add, "attributeTypes", &[at]),
        m(ModOp::Add, "objectClasses", &[oc]),
    ])
    .unwrap();
    assert!(s.attribute("qstestcolour").unwrap().single_value());
    assert!(s.class("qsTestThing").is_some());
    // Adding again is 20; a class naming an unknown attribute is 19 and
    // leaves the schema as it was.
    assert_eq!(
        s.modify(&[m(ModOp::Add, "attributeTypes", &[at])])
            .unwrap_err()
            .code(),
        ResultCode::AttributeOrValueExists
    );
    let bad = "( 1.3.6.1.4.1.99999.2.2 NAME 'qsTestBad' SUP top AUXILIARY MAY noSuchAttr )";
    assert_eq!(
        s.modify(&[m(ModOp::Add, "objectClasses", &[bad])])
            .unwrap_err()
            .code(),
        ResultCode::ConstraintViolation
    );
    assert!(s.class("qsTestBad").is_none());
    // In use: the class must go first.
    assert_eq!(
        s.modify(&[m(
            ModOp::Delete,
            "attributeTypes",
            &["( 1.3.6.1.4.1.99999.1.1 )"]
        )])
        .unwrap_err()
        .code(),
        ResultCode::ConstraintViolation
    );
    // System definitions and syntaxes are not changed.
    assert_eq!(
        s.modify(&[m(ModOp::Delete, "attributeTypes", &["( 2.5.18.1 )"])])
            .unwrap_err()
            .code(),
        ResultCode::UnwillingToPerform
    );
    assert_eq!(
        s.modify(&[m(ModOp::Add, "ldapSyntaxes", &["( 1.2.3 DESC 'x' )"])])
            .unwrap_err()
            .code(),
        ResultCode::UnwillingToPerform
    );
    // Replace a user definition; delete a shipped one.
    let drink = "( 0.9.2342.19200300.100.1.5 NAME ( 'drink' 'favouriteDrink' ) EQUALITY caseIgnoreMatch SYNTAX 1.3.6.1.4.1.1466.115.121.1.15 SINGLE-VALUE )";
    s.modify(&[m(ModOp::Replace, "attributeTypes", &[drink])])
        .unwrap();
    assert!(s.attribute("drink").unwrap().single_value());
    s.modify(&[m(
        ModOp::Add,
        "IBMAttributeTypes",
        &["( 1.3.6.1.4.1.99999.1.1 ACCESS-CLASS sensitive )"],
    )])
    .unwrap();
    assert_eq!(
        s.attribute("qsTestColour").unwrap().access,
        AccessClass::Sensitive
    );
    let text = s.modified_text();
    assert!(
        text.contains("qsTestColour") && text.contains("qsTestThing") && text.contains("'drink'"),
        "{text}"
    );
    assert!(text.contains("ACCESS-CLASS sensitive"), "{text}");
    // Delete the class and then the attribute in one modify.
    s.modify(&[
        m(
            ModOp::Delete,
            "objectClasses",
            &["( 1.3.6.1.4.1.99999.2.1 NAME 'qsTestThing' )"],
        ),
        m(
            ModOp::Delete,
            "attributeTypes",
            &["( 1.3.6.1.4.1.99999.1.1 )"],
        ),
    ])
    .unwrap();
    // A shipped class deleted shows as a deletion.
    s.modify(&[m(
        ModOp::Delete,
        "objectClasses",
        &["( 0.9.2342.19200300.100.4.7 )"],
    )])
    .unwrap();
    let text = s.modified_text();
    assert!(!text.contains("qsTestColour"));
    assert!(
        text.contains("deletedObjectClasses: 0.9.2342.19200300.100.4.7"),
        "{text}"
    );
    // The text read back over a fresh schema gives the same schema.
    let mut again = shipped();
    again.load_modified(&text).unwrap();
    assert!(again.class("room").is_none());
    assert!(again.attribute("drink").unwrap().single_value());
    assert_eq!(again.modified_text(), text);
}

#[test]
fn store_keys() {
    let s = shipped();
    assert_eq!(s.attr_id("commonName;binary").as_deref(), Some("cn"));
    assert_eq!(s.attr_id("2.5.4.0").as_deref(), Some("objectclass"));
    assert_eq!(s.attr_id("entryUUID").as_deref(), Some("entryuuid"));
    assert_eq!(s.attr_id("nope"), None);
    let r = |t: &str| Dn::parse(t).unwrap().0.remove(0);
    assert_eq!(
        s.rdn_key(&r("CN=Foo  Bar")).unwrap(),
        s.rdn_key(&r("cn=foo bar")).unwrap()
    );
    assert_eq!(
        s.rdn_key(&r("nope=x")).unwrap_err().code,
        ResultCode::InvalidDnSyntax
    );
    assert_eq!(
        s.rdn_key(&r("uidNumber=x")).unwrap_err().code,
        ResultCode::InvalidDnSyntax
    );
    assert_eq!(
        s.value_key("cn", b"  Foo   BAR ", Rule::Equality)
            .unwrap()
            .unwrap(),
        b"foo bar"
    );
    assert_eq!(
        s.value_key("cn", b"  Foo   BAR ", Rule::Substring)
            .unwrap()
            .unwrap(),
        b"foo bar"
    );
    assert_eq!(
        s.value_key("telephoneNumber", b"+1 507-555", Rule::Substring)
            .unwrap()
            .unwrap(),
        b"+1507555"
    );
    assert_eq!(
        s.value_key("uidNumber", b"007", Rule::Equality)
            .unwrap()
            .unwrap(),
        b"7"
    );
    assert_eq!(
        s.value_key("uidNumber", b"7", Rule::Substring).unwrap(),
        None
    );
    assert_eq!(
        s.value_key("cn", b"a\xFF", Rule::Equality)
            .unwrap_err()
            .code,
        ResultCode::InvalidAttributeSyntax
    );
}
