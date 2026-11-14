// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Anonymous reads: the root DSE, the schema, searches with every scope
//! and filter form, limits, attribute selection, compare, the errors.

use std::fmt::Write as _;

use ldap3::Scope;

use super::{Ctx, Lines, Scenario, folded};
use crate::Error;
use crate::client::{Found, Opts, expect_rc, names, rc, rc_lines, values};
use crate::error::check;
use crate::schema;
use crate::target::Suite;

/// This suite.
pub const LIST: [Scenario; 17] = [
    s("rootdse", rootdse),
    s("rootdse-default-attrs", rootdse_default_attrs),
    s("rootdse-one-level", rootdse_one_level),
    s("schema-read", schema_read),
    s("naming-contexts", naming_contexts),
    s("search-scopes", search_scopes),
    s("filters", filters),
    s("size-limit", size_limit),
    s("time-limit", time_limit),
    s("types-only", types_only),
    s("attr-selection", attr_selection),
    s("compare", compare),
    s("no-such-object", no_such_object),
    s("invalid-dn", invalid_dn),
    s("anon-bind", anon_bind),
    s("special-subtrees", special_subtrees),
    Scenario {
        name: "unauth-bind",
        suite: Suite::Read,
        oracle: false,
        run: unauth_bind,
    },
];

const fn s(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        suite: Suite::Read,
        oracle: true,
        run,
    }
}

/// `rc ..., N entries[, M refs]`.
pub fn summary(f: &Found) -> String {
    let mut s = format!("{}, {} entries", rc(&f.res), f.entries.len());
    if !f.refs.is_empty() {
        let _ = write!(s, ", {} refs", f.refs.len());
    }
    s
}

/// The DNs, sorted case-insensitively.
pub fn sorted_dns(f: &Found) -> Vec<String> {
    let mut v = f.dns();
    v.sort_by_key(|d| d.to_ascii_lowercase());
    v
}

/// The bases the scope and filter scenarios search: `cn=localhost` and
/// `cn=ibmpolicies` where present, else every context but the schema.
pub fn bases(c: &Ctx) -> Vec<String> {
    let mut v: Vec<String> = ["cn=localhost", "cn=ibmpolicies"]
        .into_iter()
        .filter(|b| c.dse.has_context(b))
        .map(str::to_owned)
        .collect();
    if v.is_empty() {
        v = c
            .dse
            .get("namingContexts")
            .iter()
            .filter(|n| !n.eq_ignore_ascii_case("cn=schema"))
            .cloned()
            .collect();
    }
    v
}

fn rootdse(c: &Ctx) -> Lines {
    let d = &c.dse;
    let mut out = Vec::new();
    let vers = d.get("supportedLDAPVersion");
    check(vers.iter().any(|v| v == "3"), || {
        format!("supportedLDAPVersion {vers:?} has no 3")
    })?;
    check(!d.get("namingContexts").is_empty(), || {
        "no namingContexts".into()
    })?;
    check(d.one("subschemaSubentry").is_some(), || {
        "no subschemaSubentry".into()
    })?;
    out.push(format!(
        "attributes: {}",
        d.attrs
            .iter()
            .map(|(k, v)| format!("{k}({})", v.len()))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    for name in [
        "supportedLDAPVersion",
        "namingContexts",
        "subschemaSubentry",
        "supportedSASLMechanisms",
        "ibm-supportedacimechanisms",
        "ibmdirectoryversion",
        "ibm-slapdServerBackend",
        "ibm-slapdServerInstanceName",
        "ibm-slapdSizeLimit",
        "ibm-slapdTimeLimit",
        "ibm-slapdDerefAliases",
        "ibm-osregistrycontext",
        "ibm-osregistrytype",
        "ibm-slapdisconfigurationmode",
        "security",
        "port",
        "vendorName",
        "vendorVersion",
    ] {
        out.push(format!(
            "{}: {}",
            name.to_ascii_lowercase(),
            d.get(name).join(" | ")
        ));
    }
    for name in [
        "supportedControl",
        "supportedExtension",
        "supportedFeatures",
    ] {
        let all = d.get(name);
        let mut uniq = all.to_vec();
        uniq.sort();
        uniq.dedup();
        out.push(format!(
            "{} ({} values, {} distinct): {}",
            name.to_ascii_lowercase(),
            all.len(),
            uniq.len(),
            uniq.join(" ")
        ));
    }
    Ok(out)
}

fn rootdse_default_attrs(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let f = a.find("", Scope::Base, "(objectClass=*)", &[])?;
    out.push(format!("no list: {}", summary(&f)));
    if let Some(e) = f.entries.first() {
        out.push(format!(
            "no list names: {}",
            names(&e.attrs, &e.bin_attrs).join(" ")
        ));
    }
    let f = a.find("", Scope::Base, "(objectClass=*)", &["+"])?;
    if let Some(e) = f.entries.first() {
        out.push(format!(
            "+ names: {}",
            names(&e.attrs, &e.bin_attrs).join(" ")
        ));
    }
    let f = a.find("", Scope::Base, "(objectClass=*)", &["namingContexts"])?;
    if let Some(e) = f.entries.first() {
        out.push(format!(
            "namingContexts only: {}",
            names(&e.attrs, &e.bin_attrs).join(" ")
        ));
    }
    let f = a.find("", Scope::Base, "(cn=nothing)", &["*"])?;
    out.push(format!("(cn=nothing): {}", summary(&f)));
    Ok(out)
}

fn rootdse_one_level(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for (label, scope) in [("one", Scope::OneLevel), ("sub", Scope::Subtree)] {
        let f = a.search(
            "",
            scope,
            "(objectClass=*)",
            &["1.1"],
            &Opts {
                size: 50,
                ..Opts::default()
            },
        )?;
        out.push(format!("{label}: {}", summary(&f)));
        out.push(format!("{label} dns: {}", sorted_dns(&f).join(" | ")));
    }
    Ok(out)
}

fn schema_read(c: &Ctx) -> Lines {
    let base = c
        .dse
        .one("subschemaSubentry")
        .unwrap_or("cn=schema")
        .to_owned();
    let mut a = c.anon()?;
    let kinds = [
        "attributeTypes",
        "objectClasses",
        "ldapSyntaxes",
        "matchingRules",
        "matchingRuleUse",
    ];
    let mut want: Vec<&str> = kinds.to_vec();
    want.push("objectClass");
    let f = a.find(&base, Scope::Base, "(objectClass=subschema)", &want)?;
    expect_rc("schema", &f.res, 0)?;
    let e = f
        .entries
        .first()
        .ok_or_else(|| Error::Check("schema: no entry".into()))?;
    let mut out = vec![format!(
        "objectclass: {}",
        folded(
            &values(e, "objectClass")
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>()
        )
        .join(" ")
    )];
    let mut parsed = std::collections::HashMap::new();
    for k in kinds {
        let vals = values(e, k);
        let mut bad = 0;
        let mut descs = Vec::new();
        for v in &vals {
            match schema::parse(v) {
                Ok(d) => descs.push(d),
                Err(_) => bad += 1,
            }
        }
        out.push(format!(
            "{}: {} values, {bad} unparsed",
            k.to_ascii_lowercase(),
            vals.len()
        ));
        parsed.insert(k, descs);
    }
    let at = &parsed["attributeTypes"];
    let oc = &parsed["objectClasses"];
    check(!at.is_empty() && !oc.is_empty(), || {
        "no attribute types or object classes".into()
    })?;
    let has = |list: &Vec<schema::Desc>, oid: &str, name: &str| {
        list.iter().any(|d| d.oid == oid && d.named(name))
    };
    for (oid, name) in [
        ("2.5.4.0", "objectClass"),
        ("2.5.4.3", "cn"),
        ("2.5.4.4", "sn"),
        ("2.5.4.35", "userPassword"),
        ("0.9.2342.19200300.100.1.1", "uid"),
        ("2.5.18.1", "createTimestamp"),
        ("2.5.4.49", "distinguishedName"),
    ] {
        out.push(format!(
            "attributetype {oid} {name}: {}",
            has(at, oid, name)
        ));
    }
    for name in [
        "aclEntry",
        "aclPropagate",
        "entryOwner",
        "ibm-entryuuid",
        "ibm-effectiveAcl",
        "os400-profile",
    ] {
        let found = at.iter().find(|d| d.named(name));
        out.push(format!(
            "attributetype {name}: {}",
            found.map_or("absent".into(), |d| d.oid.clone())
        ));
    }
    for (oid, name) in [
        ("2.5.6.0", "top"),
        ("2.5.6.6", "person"),
        ("2.5.6.5", "organizationalUnit"),
        ("2.16.840.1.113730.3.2.2", "inetOrgPerson"),
        ("2.16.840.1.113730.3.2.6", "referral"),
        ("2.5.6.1", "alias"),
    ] {
        out.push(format!("objectclass {oid} {name}: {}", has(oc, oid, name)));
    }
    for name in ["os400-root", "os400-usrprf", "os400-container"] {
        let found = oc.iter().find(|d| d.named(name));
        out.push(format!(
            "objectclass {name}: {}",
            found.map_or("absent".into(), |d| d.oid.clone())
        ));
    }
    if let Some(p) = oc.iter().find(|d| d.named("person")) {
        out.push(format!(
            "person must: {}",
            folded(p.get("MUST").unwrap_or(&[])).join(" ")
        ));
    }
    let syn = &parsed["ldapSyntaxes"];
    out.push(format!(
        "syntax 1.3.6.1.4.1.1466.115.121.1.15: {}",
        syn.iter().any(|d| d.oid == "1.3.6.1.4.1.1466.115.121.1.15")
    ));
    let mr = &parsed["matchingRules"];
    out.push(format!(
        "matchingrule 2.5.13.2 caseIgnoreMatch: {}",
        has(mr, "2.5.13.2", "caseIgnoreMatch")
    ));
    Ok(out)
}

fn naming_contexts(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for nc in c.dse.get("namingContexts") {
        let f = a.find(nc, Scope::Base, "(objectClass=*)", &["objectClass"])?;
        let oc = f
            .entries
            .first()
            .map(|e| {
                folded(
                    &values(e, "objectClass")
                        .iter()
                        .map(|s| (*s).to_owned())
                        .collect::<Vec<_>>(),
                )
                .join(" ")
            })
            .unwrap_or_default();
        out.push(format!("{nc}: {} objectclass {oc}", summary(&f)));
    }
    Ok(out)
}

fn search_scopes(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        for (label, scope) in [
            ("base", Scope::Base),
            ("one", Scope::OneLevel),
            ("sub", Scope::Subtree),
        ] {
            let f = a.find(&b, scope, "(objectClass=*)", &["1.1"])?;
            expect_rc(&format!("{b} {label}"), &f.res, 0)?;
            out.push(format!("{b} {label}: {}", summary(&f)));
            out.push(format!("{b} {label} dns: {}", sorted_dns(&f).join(" | ")));
        }
    }
    Ok(out)
}

/// Filters of every form (RFC 4515), each run as a subtree search.
pub const FILTERS: &[&str] = &[
    "(objectClass=*)",
    "(objectclass=*)",
    "(cn=*)",
    "(cn=pwdpolicy)",
    "(CN=PWDPOLICY)",
    "(cn=pwd*)",
    "(cn=*policy)",
    "(cn=*dpo*)",
    "(cn=p*d*y)",
    "(cn=*)",
    "(cn>=m)",
    "(cn<=m)",
    "(cn~=pwdpolicy)",
    "(cn~=ibmpolicy)",
    "(objectClass=pwdPolicy)",
    "(objectClass=container)",
    "(!(cn=pwdpolicy))",
    "(&(objectClass=*)(cn=pwdpolicy))",
    "(|(cn=nothing)(cn=pwdpolicy))",
    "(&(|(cn=a*)(cn=p*))(!(cn=a*)))",
    "(cn:caseExactMatch:=pwdpolicy)",
    "(cn:2.5.13.5:=pwdPolicy)",
    "(cn:dn:=ibmpolicies)",
    "(:dn:2.5.13.2:=ibmpolicies)",
    "(:caseIgnoreMatch:=pwdpolicy)",
    "(nosuchattribute=x)",
    "(nosuchattribute=*)",
    "(!(nosuchattribute=x))",
    "(cn=\\2a)",
    "(cn=pwd\\70olicy)",
    "(cn=)",
    "(&)",
    "(|)",
    "(entryOwner=*)",
    "(aclEntry=*)",
    "(createTimestamp>=19700101000000Z)",
    "(createTimestamp<=19700101000000Z)",
];

fn filters(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        for flt in FILTERS {
            let line = match a.find(&b, Scope::Subtree, flt, &["1.1"]) {
                Ok(f) => {
                    let mut l = format!("{b} {flt}: {}", summary(&f));
                    if !f.entries.is_empty() && f.entries.len() <= 5 {
                        let _ = write!(l, " [{}]", sorted_dns(&f).join(" | "));
                    }
                    l
                }
                Err(Error::Ldap(ldap3::LdapError::FilterParsing)) => {
                    format!("{b} {flt}: the client cannot encode it")
                }
                Err(e) => return Err(e),
            };
            out.push(line);
        }
    }
    Ok(out)
}

fn size_limit(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        let all = a.find(&b, Scope::Subtree, "(objectClass=*)", &["1.1"])?;
        let n = all.entries.len();
        for size in [1, 2, 0] {
            let f = a.search(
                &b,
                Scope::Subtree,
                "(objectClass=*)",
                &["1.1"],
                &Opts {
                    size,
                    ..Opts::default()
                },
            )?;
            out.push(format!("{b} size {size}: {}", summary(&f)));
            let size_u = usize::try_from(size).unwrap_or(0);
            if size > 0 && n > size_u {
                expect_rc(&format!("{b} size {size}"), &f.res, 4)?;
                check(f.entries.len() == size_u, || {
                    format!("{b} size {size}: {} entries", f.entries.len())
                })?;
            } else {
                expect_rc(&format!("{b} size {size}"), &f.res, 0)?;
            }
        }
    }
    Ok(out)
}

fn time_limit(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        for time in [1, 0] {
            let f = a.search(
                &b,
                Scope::Subtree,
                "(objectClass=*)",
                &["1.1"],
                &Opts {
                    time,
                    ..Opts::default()
                },
            )?;
            out.push(format!("{b} time {time}: {}", summary(&f)));
        }
    }
    Ok(out)
}

fn types_only(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        let f = a.search(
            &b,
            Scope::Base,
            "(objectClass=*)",
            &["*"],
            &Opts {
                types_only: true,
                ..Opts::default()
            },
        )?;
        expect_rc(&b, &f.res, 0)?;
        let e = f
            .entries
            .first()
            .ok_or_else(|| Error::Check(format!("{b}: no entry")))?;
        let with_values: Vec<&String> = e
            .attrs
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, _)| k)
            .collect();
        check(
            with_values.is_empty() && e.bin_attrs.values().all(Vec::is_empty),
            || format!("{b}: values returned with typesOnly: {with_values:?}"),
        )?;
        out.push(format!(
            "{b} typesonly: {}",
            names(&e.attrs, &e.bin_attrs).join(" ")
        ));
    }
    Ok(out)
}

fn attr_selection(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        for sel in [
            &["*"][..],
            &["+"],
            &["1.1"],
            &["*", "+"],
            &["cn", "objectClass"],
            &["CN"],
            &["1.1", "cn"],
            &["nosuchattribute"],
            &["*", "aclEntry"],
            &["entryOwner", "ownerSource", "aclSource", "aclPropagate"],
            &[
                "createTimestamp",
                "modifyTimestamp",
                "creatorsName",
                "modifiersName",
            ],
            &[
                "ibm-entryuuid",
                "entryDN",
                "hasSubordinates",
                "numSubordinates",
                "subschemaSubentry",
            ],
            &[],
        ] {
            let f = a.find(&b, Scope::Base, "(objectClass=*)", sel)?;
            expect_rc(&b, &f.res, 0)?;
            let e = f
                .entries
                .first()
                .ok_or_else(|| Error::Check(format!("{b}: no entry")))?;
            let got = names(&e.attrs, &e.bin_attrs);
            if sel == ["1.1"] {
                check(got.is_empty(), || format!("{b} 1.1 returned {got:?}"))?;
            }
            out.push(format!("{b} [{}]: {}", sel.join(" "), got.join(" ")));
        }
    }
    Ok(out)
}

fn compare(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        let rdn_val = b.split_once('=').map_or("", |(_, v)| v).to_owned();
        let upper = rdn_val.to_ascii_uppercase();
        for (attr, val) in [
            ("cn", rdn_val.as_str()),
            ("cn", upper.as_str()),
            ("cn", "nothing"),
            ("objectClass", "top"),
            ("objectClass", "person"),
            ("nosuchattribute", "x"),
            ("description", "x"),
        ] {
            let r = a.compare(&b, attr, val)?;
            rc_lines(&format!("{b} {attr}={val}"), &r, &mut out);
        }
        let r = a.compare(&b, "cn", &rdn_val)?;
        expect_rc(&format!("compare {b} cn"), &r, 6)?;
        let r = a.compare(&format!("cn=nothing,{b}"), "cn", "nothing")?;
        rc_lines(&format!("cn=nothing,{b}"), &r, &mut out);
    }
    Ok(out)
}

fn no_such_object(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let mut dns: Vec<String> = bases(c).iter().map(|b| format!("cn=nothing,{b}")).collect();
    dns.extend(bases(c).iter().map(|b| format!("cn=a,cn=nothing,{b}")));
    dns.push("dc=ldaptest-nowhere".into());
    dns.push("o=ldaptest-nowhere,c=us".into());
    for dn in &dns {
        for (label, scope) in [("base", Scope::Base), ("sub", Scope::Subtree)] {
            let f = a.find(dn, scope, "(objectClass=*)", &["1.1"])?;
            rc_lines(&format!("{dn} {label}"), &f.res, &mut out);
        }
    }
    if let Some(b) = bases(c).first() {
        let f = a.find(
            &format!("cn=nothing,{b}"),
            Scope::Base,
            "(objectClass=*)",
            &["1.1"],
        )?;
        expect_rc("missing entry", &f.res, 32)?;
    }
    Ok(out)
}

fn invalid_dn(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for dn in ["not a dn", "cn=", "=x", "cn=a,,cn=b", "cn=a+", "cn=\\zz"] {
        let f = a.find(dn, Scope::Base, "(objectClass=*)", &["1.1"])?;
        rc_lines(&format!("\"{dn}\""), &f.res, &mut out);
    }
    let r = a.compare("not a dn", "cn", "x")?;
    rc_lines("compare \"not a dn\"", &r, &mut out);
    Ok(out)
}

fn anon_bind(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let r = a.bind("", "")?;
    expect_rc("anonymous bind", &r, 0)?;
    let mut out = Vec::new();
    rc_lines("bind \"\" \"\"", &r, &mut out);
    let f = a.find("", Scope::Base, "(objectClass=*)", &["1.1"])?;
    out.push(format!("search after: {}", summary(&f)));
    Ok(out)
}

fn special_subtrees(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for dn in [
        "cn=monitor",
        "cn=configuration",
        "cn=changelog",
        "cn=schema",
        "cn=connections,cn=monitor",
    ] {
        let f = a.find(dn, Scope::Base, "(objectClass=*)", &["*", "+"])?;
        out.push(format!("{dn}: {}", summary(&f)));
        if let Some(e) = f.entries.first() {
            let n = names(&e.attrs, &e.bin_attrs);
            out.push(format!("{dn}: {} attribute names", n.len()));
            if dn != "cn=schema" {
                out.push(format!("{dn} names: {}", n.join(" ")));
            }
        }
        if !f.res.text.is_empty() {
            out.push(format!("{dn} diag: {}", f.res.text.trim()));
        }
        let f = a.find(dn, Scope::OneLevel, "(objectClass=*)", &["1.1"])?;
        out.push(format!("{dn} one: {}", summary(&f)));
    }
    Ok(out)
}

/// A name with no password (RFC 4513 §5.1.2): servers should refuse it.
fn unauth_bind(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let dn = format!(
        "cn=ldaptest-nobody,{}",
        bases(c).first().map_or("cn=localhost", String::as_str)
    );
    let r = a.bind(&dn, "")?;
    let mut out = Vec::new();
    rc_lines(&format!("bind {dn} \"\""), &r, &mut out);
    check(r.rc == 53 || r.rc == 0, || {
        format!("unauthenticated bind: {}", rc(&r))
    })?;
    Ok(out)
}
