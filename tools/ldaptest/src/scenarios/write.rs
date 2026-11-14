// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writes under the test suffix (docs/DIRSRV.md §7): add, modify (add,
//! delete, replace, increment), modify DN (rename, subtree move), delete,
//! and their errors.

use std::collections::HashSet;

use ldap3::{Mod, Scope};

use super::{Ctx, Lines, Scenario};
use crate::Error;
use crate::client::{expect_rc, rc_lines, values};
use crate::error::check;
use crate::target::Suite;

/// This suite.
pub const LIST: [Scenario; 9] = [
    w("add-entry", add_entry),
    w("add-errors", add_errors),
    w("modify-ops", modify_ops),
    w("modify-errors", modify_errors),
    w("moddn-rename", moddn_rename),
    w("moddn-subtree-move", moddn_subtree_move),
    w("delete-entry", delete_entry),
    w("anon-write-denied", anon_write_denied),
    w("operational-attrs", operational_attrs),
];

const fn w(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        suite: Suite::Write,
        oracle: false,
        run,
    }
}

fn set(v: &[&str]) -> HashSet<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn read_attr(c: &mut crate::client::Conn, dn: &str, attr: &str) -> Result<Vec<String>, Error> {
    let f = c.find(dn, Scope::Base, "(objectClass=*)", &[attr])?;
    expect_rc(&format!("read {dn}"), &f.res, 0)?;
    let mut v: Vec<String> = f
        .entries
        .first()
        .map(|e| values(e, attr).into_iter().map(str::to_owned).collect())
        .unwrap_or_default();
    v.sort();
    Ok(v)
}

fn add_entry(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("add")?;
    let dn = sb.add(
        "cn=Ann Example",
        &[
            (
                "objectClass",
                &["top", "person", "organizationalPerson", "inetOrgPerson"],
            ),
            ("cn", &["Ann Example", "Ann"]),
            ("sn", &["Example"]),
            ("mail", &["ann@example.com"]),
            ("description", &["caf\u{e9} \u{65e5}\u{672c}"]),
        ],
    )?;
    let mut a = c.anon()?;
    let f = a.find(&dn, Scope::Base, "(objectClass=*)", &["*"])?;
    expect_rc("read back", &f.res, 0)?;
    let e = f
        .entries
        .first()
        .ok_or_else(|| Error::Check("no entry".into()))?;
    let mut cn: Vec<&str> = values(e, "cn");
    cn.sort_unstable();
    check(cn == ["Ann", "Ann Example"], || format!("cn {cn:?}"))?;
    check(
        values(e, "description") == ["caf\u{e9} \u{65e5}\u{672c}"],
        || "UTF-8 value changed".into(),
    )?;
    // Found by each of its values.
    for flt in ["(cn=ann)", "(mail=ANN@example.com)", "(sn=exam*)"] {
        let f = a.find(&sb.dn, Scope::Subtree, flt, &["1.1"])?;
        check(f.entries.len() == 1, || {
            format!("{flt}: {} entries", f.entries.len())
        })?;
    }
    Ok(vec![format!("added and read back {dn}")])
}

fn add_errors(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("adderr")?;
    let mut out = Vec::new();
    let person: &[(&str, &[&str])] = &[
        ("objectClass", &["top", "person"]),
        ("cn", &["dup"]),
        ("sn", &["x"]),
    ];
    let dn = sb.add("cn=dup", person)?;
    let cases: Vec<(&str, String, Vec<(&str, &[&str])>, u32)> = vec![
        ("exists", dn.clone(), person.to_vec(), 68),
        (
            "no parent",
            sb.child("cn=a,ou=missing"),
            vec![
                ("objectClass", &["top", "person"]),
                ("cn", &["a"]),
                ("sn", &["x"]),
            ],
            32,
        ),
        (
            "missing MUST",
            sb.child("cn=nosn"),
            vec![("objectClass", &["top", "person"]), ("cn", &["nosn"])],
            65,
        ),
        (
            "unknown attribute",
            sb.child("cn=ua"),
            vec![
                ("objectClass", &["top", "person"]),
                ("cn", &["ua"]),
                ("sn", &["x"]),
                ("ldaptestNoSuchAttr", &["x"]),
            ],
            17,
        ),
        (
            "unknown class",
            sb.child("cn=uc"),
            vec![
                ("objectClass", &["top", "ldaptestNoSuchClass"]),
                ("cn", &["uc"]),
            ],
            65,
        ),
        (
            "no objectClass",
            sb.child("cn=noc"),
            vec![("cn", &["noc"]), ("sn", &["x"])],
            65,
        ),
        (
            "RDN value not in entry",
            sb.child("cn=rdn"),
            vec![
                ("objectClass", &["top", "person"]),
                ("cn", &["other"]),
                ("sn", &["x"]),
            ],
            64,
        ),
        (
            "bad syntax",
            sb.child("cn=syn"),
            vec![
                ("objectClass", &["top", "person"]),
                ("cn", &["syn"]),
                ("sn", &["x"]),
                ("seeAlso", &["not a dn"]),
            ],
            21,
        ),
        ("bad DN", "cn=,x".into(), person.to_vec(), 34),
        (
            "outside every suffix",
            "cn=a,o=ldaptest-nowhere".into(),
            person.to_vec(),
            32,
        ),
    ];
    let mut wrong = Vec::new();
    for (label, dn, attrs, want) in cases {
        let r = sb.admin.add(&dn, &attrs, vec![])?;
        rc_lines(label, &r, &mut out);
        // Some servers answer 10 (a default referral) outside every suffix.
        if r.rc != want && !(label == "outside every suffix" && r.rc == 10) {
            wrong.push(format!("{label}: {}, wanted {want}", r.rc));
        }
    }
    check(wrong.is_empty(), || wrong.join("; "))?;
    Ok(out)
}

fn modify_ops(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("mod")?;
    let dn = sb.add(
        "cn=m",
        &[
            (
                "objectClass",
                &[
                    "top",
                    "person",
                    "organizationalPerson",
                    "inetOrgPerson",
                    "posixAccount",
                ],
            ),
            ("cn", &["m"]),
            ("sn", &["one"]),
            ("uid", &["m"]),
            ("uidNumber", &["100"]),
            ("gidNumber", &["100"]),
            ("homeDirectory", &["/home/m"]),
        ],
    )?;
    let a = &mut sb.admin;
    let mut out = Vec::new();
    let r = a.modify(
        &dn,
        vec![Mod::Add("description".into(), set(&["d1", "d2"]))],
        vec![],
    )?;
    expect_rc("add values", &r, 0)?;
    check(read_attr(a, &dn, "description")? == ["d1", "d2"], || {
        "add values".into()
    })?;
    let r = a.modify(
        &dn,
        vec![Mod::Delete("description".into(), set(&["d1"]))],
        vec![],
    )?;
    expect_rc("delete a value", &r, 0)?;
    check(read_attr(a, &dn, "description")? == ["d2"], || {
        "delete a value".into()
    })?;
    let r = a.modify(
        &dn,
        vec![Mod::Replace("sn".into(), set(&["two", "three"]))],
        vec![],
    )?;
    expect_rc("replace", &r, 0)?;
    check(read_attr(a, &dn, "sn")? == ["three", "two"], || {
        "replace".into()
    })?;
    let r = a.modify(
        &dn,
        vec![Mod::Delete("description".into(), HashSet::new())],
        vec![],
    )?;
    expect_rc("delete the attribute", &r, 0)?;
    check(read_attr(a, &dn, "description")?.is_empty(), || {
        "delete the attribute".into()
    })?;
    let r = a.modify(
        &dn,
        vec![Mod::Replace("description".into(), HashSet::new())],
        vec![],
    )?;
    expect_rc("replace with nothing (absent)", &r, 0)?;
    // Several changes are one: either all or none.
    let r = a.modify(
        &dn,
        vec![
            Mod::Add("description".into(), set(&["x"])),
            Mod::Delete("sn".into(), set(&["ldaptest-absent"])),
        ],
        vec![],
    )?;
    rc_lines("add then a failing delete", &r, &mut out);
    check(read_attr(a, &dn, "description")?.is_empty(), || {
        "a failed modify left a change".into()
    })?;
    let r = a.modify(
        &dn,
        vec![Mod::Increment("uidNumber".into(), "5".into())],
        vec![],
    )?;
    rc_lines("increment uidNumber by 5", &r, &mut out);
    if r.rc == 0 {
        check(read_attr(a, &dn, "uidNumber")? == ["105"], || {
            "increment".into()
        })?;
    }
    out.push("add, delete value, replace, delete attribute, atomicity".into());
    Ok(out)
}

fn modify_errors(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("moderr")?;
    let dn = sb.add(
        "cn=e",
        &[
            ("objectClass", &["top", "person"]),
            ("cn", &["e"]),
            ("sn", &["x"]),
            ("description", &["d"]),
        ],
    )?;
    let mut out = Vec::new();
    let cases: Vec<(&str, String, Vec<Mod<String>>, u32)> = vec![
        (
            "delete an absent value",
            dn.clone(),
            vec![Mod::Delete("description".into(), set(&["nope"]))],
            16,
        ),
        (
            "add an existing value",
            dn.clone(),
            vec![Mod::Add("description".into(), set(&["d"]))],
            20,
        ),
        (
            "delete the RDN value",
            dn.clone(),
            vec![Mod::Delete("cn".into(), set(&["e"]))],
            67,
        ),
        (
            "delete a MUST",
            dn.clone(),
            vec![Mod::Delete("sn".into(), HashSet::new())],
            65,
        ),
        (
            "unknown attribute",
            dn.clone(),
            vec![Mod::Add("ldaptestNoSuchAttr".into(), set(&["x"]))],
            17,
        ),
        (
            "attribute not allowed",
            dn.clone(),
            vec![Mod::Add("mail".into(), set(&["x@example.com"]))],
            65,
        ),
        (
            "no such entry",
            sb.child("cn=missing"),
            vec![Mod::Add("description".into(), set(&["x"]))],
            32,
        ),
        (
            "NO-USER-MODIFICATION",
            dn.clone(),
            vec![Mod::Replace(
                "createTimestamp".into(),
                set(&["20200101000000Z"]),
            )],
            19,
        ),
        (
            "structural class change",
            dn.clone(),
            vec![Mod::Replace(
                "objectClass".into(),
                set(&["top", "organizationalUnit"]),
            )],
            69,
        ),
    ];
    let mut wrong = Vec::new();
    for (label, dn, mods, want) in cases {
        let r = sb.admin.modify(&dn, mods, vec![])?;
        rc_lines(label, &r, &mut out);
        // 53 is also what servers answer for operational attributes.
        if r.rc != want
            && !(label == "NO-USER-MODIFICATION" && r.rc == 53)
            && !(label == "structural class change" && r.rc == 65)
        {
            wrong.push(format!("{label}: {}, wanted {want}", r.rc));
        }
    }
    check(wrong.is_empty(), || wrong.join("; "))?;
    Ok(out)
}

fn moddn_rename(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("ren")?;
    let mut out = Vec::new();
    let person: &[(&str, &[&str])] = &[
        ("objectClass", &["top", "person"]),
        ("cn", &["a"]),
        ("sn", &["x"]),
    ];
    let dn = sb.add("cn=a", person)?;
    let r = sb.admin.moddn(&dn, "cn=b", false, None)?;
    expect_rc("rename keeping the old value", &r, 0)?;
    let b = sb.child("cn=b");
    check(read_attr(&mut sb.admin, &b, "cn")? == ["a", "b"], || {
        "old RDN value not kept".into()
    })?;
    let r = sb.admin.moddn(&b, "cn=c", true, None)?;
    expect_rc("rename deleting the old value", &r, 0)?;
    let cdn = sb.child("cn=c");
    check(read_attr(&mut sb.admin, &cdn, "cn")? == ["a", "c"], || {
        "cn after delete-old".into()
    })?;
    let f = sb
        .admin
        .find(&b, Scope::Base, "(objectClass=*)", &["1.1"])?;
    expect_rc("old name", &f.res, 32)?;
    sb.add(
        "cn=d",
        &[
            ("objectClass", &["top", "person"]),
            ("cn", &["d"]),
            ("sn", &["x"]),
        ],
    )?;
    let r = sb.admin.moddn(&cdn, "cn=d", true, None)?;
    rc_lines("rename onto an existing name", &r, &mut out);
    expect_rc("rename onto an existing name", &r, 68)?;
    let r = sb
        .admin
        .moddn(&sb.child("cn=missing"), "cn=e", true, None)?;
    expect_rc("rename a missing entry", &r, 32)?;
    let r = sb.admin.moddn(&cdn, "cn=C", true, None)?;
    rc_lines("rename to a case change", &r, &mut out);
    out.push("renamed with and without deleteOldRDN".into());
    Ok(out)
}

fn moddn_subtree_move(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("move")?;
    let from = sb.ou("from", None)?;
    let to = sb.ou("to", None)?;
    let tree = sb.ou("tree", Some(&from))?;
    let mid = sb.ou("mid", Some(&tree))?;
    for i in 0..5 {
        sb.add_dn(
            format!("cn=p{i},{mid}"),
            &[
                ("objectClass", &["top", "person"]),
                ("cn", &[&format!("p{i}")]),
                ("sn", &["x"]),
            ],
        )?;
    }
    let r = sb.admin.moddn(&tree, "ou=tree", true, Some(&to))?;
    expect_rc("move", &r, 0)?;
    let moved = format!("ou=tree,{to}");
    let f = sb
        .admin
        .find(&moved, Scope::Subtree, "(objectClass=*)", &["1.1"])?;
    check(f.entries.len() == 7, || {
        format!("{} entries under the new place", f.entries.len())
    })?;
    let f = sb
        .admin
        .find(&from, Scope::OneLevel, "(objectClass=*)", &["1.1"])?;
    check(f.entries.is_empty(), || {
        "entries left under the old parent".into()
    })?;
    let f = sb.admin.find(&moved, Scope::Subtree, "(cn=p3)", &["1.1"])?;
    check(
        f.dns().iter().any(|d| {
            d.to_ascii_lowercase()
                .starts_with("cn=p3,ou=mid,ou=tree,ou=to,")
        }),
        || format!("{:?}", f.dns()),
    )?;
    let mut out = vec!["moved a subtree of 7 entries".into()];
    let r = sb
        .admin
        .moddn(&moved, "ou=tree", true, Some(&format!("ou=mid,{moved}")))?;
    rc_lines("move under itself", &r, &mut out);
    check(r.rc != 0, || "moved under itself".into())?;
    let r = sb
        .admin
        .moddn(&moved, "ou=tree", true, Some(&sb.child("ou=missing")))?;
    rc_lines("move under a missing parent", &r, &mut out);
    expect_rc("move under a missing parent", &r, 32)?;
    Ok(out)
}

fn delete_entry(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("del")?;
    let mut out = Vec::new();
    let parent = sb.ou("p", None)?;
    let child = sb.ou("c", Some(&parent))?;
    let r = sb.admin.delete(&parent, vec![])?;
    rc_lines("delete a parent", &r, &mut out);
    expect_rc("delete a parent", &r, 66)?;
    let r = sb.admin.delete(&child, vec![])?;
    expect_rc("delete a leaf", &r, 0)?;
    let r = sb.admin.delete(&child, vec![])?;
    expect_rc("delete it again", &r, 32)?;
    let r = sb.admin.delete(&parent, vec![])?;
    expect_rc("delete the parent now", &r, 0)?;
    Ok(out)
}

fn anon_write_denied(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("anon")?;
    let dn = sb.add(
        "cn=x",
        &[
            ("objectClass", &["top", "person"]),
            ("cn", &["x"]),
            ("sn", &["x"]),
        ],
    )?;
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let r = a.add(
        &sb.child("cn=y"),
        &[
            ("objectClass", &["top", "person"]),
            ("cn", &["y"]),
            ("sn", &["y"]),
        ],
        vec![],
    )?;
    rc_lines("anonymous add", &r, &mut out);
    expect_rc("anonymous add", &r, 50)?;
    let r = a.modify(&dn, vec![Mod::Replace("sn".into(), set(&["z"]))], vec![])?;
    rc_lines("anonymous modify", &r, &mut out);
    expect_rc("anonymous modify", &r, 50)?;
    let r = a.moddn(&dn, "cn=z", true, None)?;
    rc_lines("anonymous modify DN", &r, &mut out);
    expect_rc("anonymous modify DN", &r, 50)?;
    let r = a.delete(&dn, vec![])?;
    rc_lines("anonymous delete", &r, &mut out);
    expect_rc("anonymous delete", &r, 50)?;
    Ok(out)
}

fn operational_attrs(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("op")?;
    let dn = sb.add(
        "cn=o",
        &[
            ("objectClass", &["top", "person"]),
            ("cn", &["o"]),
            ("sn", &["x"]),
        ],
    )?;
    let f = sb.admin.find(&dn, Scope::Base, "(objectClass=*)", &["+"])?;
    let e = f
        .entries
        .first()
        .ok_or_else(|| Error::Check("no entry".into()))?;
    let mut out = vec![format!(
        "operational: {}",
        crate::client::names(&e.attrs, &e.bin_attrs).join(" ")
    )];
    for a in [
        "createTimestamp",
        "modifyTimestamp",
        "creatorsName",
        "modifiersName",
    ] {
        check(!values(e, a).is_empty(), || format!("no {a}"))?;
    }
    let creator = values(e, "creatorsName")
        .first()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    check(creator == c.t.admin_dn.to_ascii_lowercase(), || {
        format!("creatorsName {creator}")
    })?;
    out.push(format!("creatorsName: {creator}"));
    Ok(out)
}
