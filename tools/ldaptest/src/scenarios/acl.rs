// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Access control as docs/DIRSRV.md §12 describes IBM's model: the default
//! ACL, `aclEntry` grants and denials, propagation, `entryOwner`, the
//! computed sources; anonymous against a bound user against the
//! administrator; simple bind errors.

use std::collections::HashSet;

use ldap3::{Mod, Scope};

use super::{Ctx, Lines, Sandbox, Scenario};
use crate::client::{Conn, expect_rc, rc_lines, values};
use crate::error::check;
use crate::target::Suite;
use crate::Error;

/// This suite.
pub const LIST: [Scenario; 7] = [
    w("acl-default", acl_default),
    w("acl-grant", acl_grant),
    w("acl-deny", acl_deny),
    w("acl-propagate", acl_propagate),
    w("acl-owner", acl_owner),
    w("acl-sources", acl_sources),
    w("bind-errors", bind_errors),
];

const fn w(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario { name, suite: Suite::Acl, oracle: false, run }
}

const PW: &str = "Acl-pass-1";

fn set(v: &[&str]) -> HashSet<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

/// The attribute names `c` sees of `dn`.
fn sees(c: &mut Conn, dn: &str) -> Result<Vec<String>, Error> {
    let f = c.find(dn, Scope::Base, "(objectClass=*)", &["*"])?;
    Ok(f.entries.first().map(|e| crate::client::names(&e.attrs, &e.bin_attrs)).unwrap_or_default())
}

fn set_acl(sb: &mut Sandbox, dn: &str, acl: &[&str], propagate: Option<bool>) -> Result<(), Error> {
    let mut mods = vec![Mod::Replace("aclEntry".to_owned(), set(acl))];
    if let Some(p) = propagate {
        mods.push(Mod::Replace("aclPropagate".to_owned(), set(&[if p { "TRUE" } else { "FALSE" }])));
    }
    let r = sb.admin.modify(dn, mods, vec![])?;
    expect_rc(&format!("aclEntry on {dn}"), &r, 0)
}

fn acl_default(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("acldef")?;
    let user = sb.user("reader", PW)?;
    let mut out = Vec::new();
    let mut anon = c.anon()?;
    let a = sees(&mut anon, &user)?;
    out.push(format!("anonymous sees: {}", a.join(" ")));
    check(a.contains(&"cn".to_owned()) && !a.contains(&"userpassword".to_owned()), || format!("anonymous sees {a:?}"))?;
    let mut u = c.bound(&user, PW)?;
    let s = sees(&mut u, &user)?;
    out.push(format!("the user sees of itself: {}", s.join(" ")));
    let adm = sees(&mut sb.admin, &user)?;
    out.push(format!("the administrator sees: {}", adm.join(" ")));
    check(adm.contains(&"userpassword".to_owned()), || "the administrator does not see userPassword".into())?;
    // Searching on a critical attribute finds nothing for anonymous users.
    let f = anon.find(&sb.dn, Scope::Subtree, "(userPassword=*)", &["1.1"])?;
    out.push(format!("anonymous (userPassword=*): {} entries", f.entries.len()));
    check(f.entries.is_empty(), || "anonymous users search on userPassword".into())?;
    let r = anon.compare(&user, "userPassword", PW)?;
    rc_lines("anonymous compare of userPassword", &r, &mut out);
    check(r.rc != 6, || "anonymous compare of userPassword answered true".into())?;
    Ok(out)
}

fn acl_grant(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("aclgrant")?;
    let user = sb.user("writer", PW)?;
    let g = sb.ou("granted", None)?;
    set_acl(&mut sb, &g, &["group:cn=anybody:normal:rsc", &format!("access-id:{user}:object:ad:normal:rwsc")], None)?;
    let mut out = Vec::new();
    let mut u = c.bound(&user, PW)?;
    let child = format!("ou=bywriter,{g}");
    let r = u.add(&child, &[("objectClass", &["top", "organizationalUnit"]), ("ou", &["bywriter"])], vec![])?;
    rc_lines("the user adds under the grant", &r, &mut out);
    expect_rc("the user adds under the grant", &r, 0)?;
    let r = u.modify(&child, vec![Mod::Replace("description".into(), set(&["mine"]))], vec![])?;
    expect_rc("the user modifies it", &r, 0)?;
    let r = u.add(&sb.child("ou=outside"), &[("objectClass", &["top", "organizationalUnit"]), ("ou", &["outside"])], vec![])?;
    rc_lines("the user adds outside the grant", &r, &mut out);
    expect_rc("the user adds outside the grant", &r, 50)?;
    let mut anon = c.anon()?;
    let r = anon.add(&format!("ou=byanon,{g}"), &[("objectClass", &["top", "organizationalUnit"]), ("ou", &["byanon"])], vec![])?;
    rc_lines("anonymous add under the grant", &r, &mut out);
    expect_rc("anonymous add under the grant", &r, 50)?;
    let r = u.delete(&child, vec![])?;
    expect_rc("the user deletes it", &r, 0)?;
    Ok(out)
}

fn acl_deny(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("acldeny")?;
    let user = sb.user("denied", PW)?;
    let d = sb.ou("denying", None)?;
    set_acl(
        &mut sb,
        &d,
        &["group:cn=anybody:normal:rsc", &format!("access-id:{user}:normal:rsc:at.description:deny:rsc")],
        None,
    )?;
    let target = format!("cn=t,{d}");
    sb.add_dn(target.clone(), &[("objectClass", &["top", "person"]), ("cn", &["t"]), ("sn", &["t"]), ("description", &["secret"])])?;
    let mut out = Vec::new();
    let mut u = c.bound(&user, PW)?;
    let s = sees(&mut u, &target)?;
    out.push(format!("the user sees: {}", s.join(" ")));
    check(!s.contains(&"description".to_owned()) && s.contains(&"sn".to_owned()), || format!("the user sees {s:?}"))?;
    let f = u.find(&d, Scope::Subtree, "(description=secret)", &["1.1"])?;
    check(f.entries.is_empty(), || "the user searches on a denied attribute".into())?;
    let mut anon = c.anon()?;
    let a = sees(&mut anon, &target)?;
    out.push(format!("anonymous sees: {}", a.join(" ")));
    check(a.contains(&"description".to_owned()), || "the denial reached anonymous users".into())?;
    Ok(out)
}

fn acl_propagate(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("aclprop")?;
    let user = sb.user("prop", PW)?;
    let p = sb.ou("here-only", None)?;
    set_acl(&mut sb, &p, &["group:cn=anybody:normal:rsc", &format!("access-id:{user}:normal:rwsc")], Some(false))?;
    let child = sb.ou("child", Some(&p))?;
    let mut out = Vec::new();
    let mut u = c.bound(&user, PW)?;
    let r = u.modify(&p, vec![Mod::Replace("description".into(), set(&["x"]))], vec![])?;
    rc_lines("modify the entry with the ACL", &r, &mut out);
    expect_rc("modify the entry with the ACL", &r, 0)?;
    let r = u.modify(&child, vec![Mod::Replace("description".into(), set(&["x"]))], vec![])?;
    rc_lines("modify its child (not propagated)", &r, &mut out);
    expect_rc("modify its child", &r, 50)?;
    Ok(out)
}

fn acl_owner(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("aclown")?;
    let user = sb.user("owner", PW)?;
    let o = sb.ou("owned", None)?;
    let r = sb.admin.modify(&o, vec![Mod::Replace("entryOwner".into(), set(&[&format!("access-id:{user}")]))], vec![])?;
    expect_rc("entryOwner", &r, 0)?;
    let child = format!("cn=k,{o}");
    sb.add_dn(child.clone(), &[("objectClass", &["top", "person"]), ("cn", &["k"]), ("sn", &["k"])])?;
    let other = sb.ou("other", None)?;
    let mut out = Vec::new();
    let mut u = c.bound(&user, PW)?;
    let r = u.modify(&child, vec![Mod::Replace("sn".into(), set(&["changed"]))], vec![])?;
    expect_rc("the owner modifies below", &r, 0)?;
    let r = u.modify(&o, vec![Mod::Replace("aclEntry".into(), set(&["group:cn=anybody:normal:rsc"]))], vec![])?;
    rc_lines("the owner sets an ACL", &r, &mut out);
    expect_rc("the owner sets an ACL", &r, 0)?;
    let r = u.modify(&other, vec![Mod::Replace("description".into(), set(&["x"]))], vec![])?;
    rc_lines("the owner modifies elsewhere", &r, &mut out);
    expect_rc("the owner modifies elsewhere", &r, 50)?;
    let r = u.delete(&child, vec![])?;
    expect_rc("the owner deletes below", &r, 0)?;
    out.push("the owner has every right below the owned entry".into());
    Ok(out)
}

fn acl_sources(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("aclsrc")?;
    let user = sb.user("src", PW)?;
    let p = sb.ou("src", None)?;
    set_acl(&mut sb, &p, &["group:cn=anybody:normal:rsc", &format!("access-id:{user}:normal:rwsc")], None)?;
    let r = sb.admin.modify(&p, vec![Mod::Replace("entryOwner".into(), set(&[&format!("access-id:{user}")]))], vec![])?;
    expect_rc("entryOwner", &r, 0)?;
    let child = sb.ou("kid", Some(&p))?;
    let f = sb.admin.find(&child, Scope::Base, "(objectClass=*)", &["aclSource", "ownerSource", "ibm-effectiveAcl", "aclEntry", "entryOwner"])?;
    let e = f.entries.first().ok_or_else(|| Error::Check("no entry".into()))?;
    let acl_src: Vec<&str> = values(e, "aclSource");
    let own_src: Vec<&str> = values(e, "ownerSource");
    check(acl_src.iter().any(|s| s.eq_ignore_ascii_case(&p)), || format!("aclSource {acl_src:?}"))?;
    check(own_src.iter().any(|s| s.eq_ignore_ascii_case(&p)), || format!("ownerSource {own_src:?}"))?;
    let mut eff = values(e, "ibm-effectiveAcl");
    eff.sort_unstable();
    Ok(vec![
        format!("aclSource: {}", acl_src.join(" | ")),
        format!("ownerSource: {}", own_src.join(" | ")),
        format!("ibm-effectiveAcl: {} values", eff.len()),
        format!("inherited aclEntry: {}", values(e, "aclEntry").len()),
    ])
}

fn bind_errors(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("bind")?;
    let user = sb.user("binder", PW)?;
    let mut out = Vec::new();
    for (label, dn, pw) in [
        ("wrong password", user.clone(), "wrong"),
        ("no such entry", sb.child("cn=nobody"), "x"),
        ("an entry with no password", sb.dn.clone(), "x"),
    ] {
        let mut a = c.anon()?;
        let r = a.bind(&dn, pw)?;
        rc_lines(label, &r, &mut out);
        expect_rc(label, &r, 49)?;
    }
    let mut a = c.anon()?;
    let r = a.bind(&user.to_ascii_uppercase(), PW)?;
    rc_lines("the DN in upper case", &r, &mut out);
    expect_rc("the DN in upper case", &r, 0)?;
    let mut a = c.anon()?;
    let r = a.bind("", PW)?;
    rc_lines("no name with a password", &r, &mut out);
    check(r.rc != 0, || "a password with no name bound".into())?;
    Ok(out)
}
