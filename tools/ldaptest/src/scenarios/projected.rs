// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The projected backend (docs/DIRSRV.md §9): `os400-sys=SYSTEM`, its
//! `cn=accounts` and one `os400-usrprf` entry per profile; anonymous
//! users see only the root; profile binds; disabled profiles; read-only.

use std::collections::HashSet;

use ldap3::{Mod, Scope};

use super::read::{sorted_dns, summary};
use super::{Ctx, Lines, Scenario, folded};
use crate::client::{expect_rc, rc_lines, values};
use crate::error::{check, skip};
use crate::target::Suite;
use crate::Error;

/// This suite.
pub const LIST: [Scenario; 5] = [
    Scenario { name: "projected-anon", suite: Suite::Projected, oracle: true, run: projected_anon },
    w("projected-bind", projected_bind),
    w("projected-disabled", projected_disabled),
    w("projected-read-only", projected_read_only),
    w("projected-admin-view", projected_admin_view),
];

const fn w(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario { name, suite: Suite::Projected, oracle: false, run }
}

fn root(c: &Ctx) -> Result<String, Error> {
    match c.t.system.clone().or_else(|| c.dse.system()) {
        Some(s) => Ok(format!("os400-sys={s}")),
        None => skip("no ibm-osregistrycontext and no --system"),
    }
}

/// `os400-profile=NAME,cn=accounts,ROOT`.
fn profile_dn(root: &str, name: &str) -> String {
    format!("os400-profile={name},cn=accounts,{root}")
}

fn projected_anon(c: &Ctx) -> Lines {
    let root = root(c)?;
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let f = a.find(&root, Scope::Base, "(objectClass=*)", &["*"])?;
    expect_rc("root", &f.res, 0)?;
    let e = f.entries.first().ok_or_else(|| Error::Check("no root entry".into()))?;
    let oc = folded(&values(e, "objectClass").iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
    check(oc.contains(&"os400-root".to_owned()), || format!("root objectClass {oc:?}"))?;
    out.push(format!("root: objectclass {}; attributes {}", oc.join(" "), crate::client::names(&e.attrs, &e.bin_attrs).join(" ")));
    for (label, base, scope, flt) in [
        ("one", root.clone(), Scope::OneLevel, "(objectClass=*)"),
        ("sub", root.clone(), Scope::Subtree, "(objectClass=*)"),
        ("sub profiles", root.clone(), Scope::Subtree, "(os400-profile=*)"),
        ("accounts base", format!("cn=accounts,{root}"), Scope::Base, "(objectClass=*)"),
        ("accounts one", format!("cn=accounts,{root}"), Scope::OneLevel, "(objectClass=*)"),
        ("QSECOFR", profile_dn(&root, "QSECOFR"), Scope::Base, "(objectClass=*)"),
    ] {
        let f = a.find(&base, scope, flt, &["1.1"])?;
        out.push(format!("{label}: {} [{}]", summary(&f), sorted_dns(&f).join(" | ")));
        if !f.res.text.is_empty() {
            out.push(format!("{label} diag: {}", f.res.text.trim()));
        }
        if label.starts_with("sub") || label == "accounts one" {
            check(f.entries.iter().all(|e| e.dn.eq_ignore_ascii_case(&root)), || format!("{label}: anonymous sees {:?}", f.dns()))?;
        }
    }
    let r = a.compare(&root, "objectClass", "os400-root")?;
    rc_lines("compare root objectClass", &r, &mut out);
    Ok(out)
}

fn profile(c: &Ctx) -> Result<(String, String), Error> {
    match &c.t.profile {
        Some((n, Some(pw))) => Ok((n.clone(), pw.clone())),
        Some((_, None)) => skip("no LDAPTEST_PROFILE_PW"),
        None => skip("no --profile"),
    }
}

fn projected_bind(c: &Ctx) -> Lines {
    let root = root(c)?;
    let (name, pw) = profile(c)?;
    let dn = profile_dn(&root, &name);
    let mut u = c.bound(&dn, &pw)?;
    let mut out = vec![format!("bound as {dn}")];
    let f = u.find(&dn, Scope::Base, "(objectClass=*)", &["*"])?;
    expect_rc("own entry", &f.res, 0)?;
    let e = f.entries.first().ok_or_else(|| Error::Check("own entry not visible".into()))?;
    let oc = folded(&values(e, "objectClass").iter().map(|s| (*s).to_owned()).collect::<Vec<_>>());
    check(oc.contains(&"os400-usrprf".to_owned()), || format!("objectClass {oc:?}"))?;
    out.push(format!("own entry: {}", crate::client::names(&e.attrs, &e.bin_attrs).join(" ")));
    let prof: Vec<&str> = values(e, "os400-profile");
    check(prof.iter().any(|p| p.eq_ignore_ascii_case(&name)), || format!("os400-profile {prof:?}"))?;
    if c.dse.extension(super::ext::WHOAMI) {
        let (x, r) = u.extended(ldap3::exop::WhoAmI.into(), vec![])?;
        expect_rc("whoami", &r, 0)?;
        let id = <ldap3::exop::WhoAmIResp as ldap3::exop::ExopParser>::parse(x.val.as_deref().unwrap_or(&[])).authzid;
        out.push(format!("whoami: {id}"));
    }
    let f = u.find(&format!("cn=accounts,{root}"), Scope::OneLevel, "(objectClass=*)", &["1.1"])?;
    out.push(format!("accounts one: {}", summary(&f)));
    Ok(out)
}

fn projected_disabled(c: &Ctx) -> Lines {
    let root = root(c)?;
    let (name, pw) = match &c.t.disabled {
        Some((n, Some(pw))) => (n.clone(), pw.clone()),
        Some((_, None)) => return skip("no LDAPTEST_DISABLED_PW"),
        None => return skip("no --disabled-profile"),
    };
    let dn = profile_dn(&root, &name);
    let mut a = c.anon()?;
    let r = a.bind(&dn, &pw)?;
    let mut out = Vec::new();
    rc_lines(&format!("bind {dn}"), &r, &mut out);
    expect_rc("disabled profile", &r, 49)?;
    Ok(out)
}

fn projected_read_only(c: &Ctx) -> Lines {
    let root = root(c)?;
    let (name, pw) = profile(c)?;
    let dn = profile_dn(&root, &name);
    let mut u = c.bound(&dn, &pw)?;
    let mut out = Vec::new();
    let r = u.modify(&dn, vec![Mod::Replace("description".to_owned(), HashSet::from(["x".to_owned()]))], vec![])?;
    rc_lines("modify own entry", &r, &mut out);
    expect_rc("modify own entry", &r, 53)?;
    let r = u.add(&profile_dn(&root, "LDAPTESTX"), &[("objectClass", &["top", "os400-usrprf"]), ("os400-profile", &["LDAPTESTX"])], vec![])?;
    rc_lines("add a profile entry", &r, &mut out);
    check(r.rc == 53 || r.rc == 50, || format!("add a profile entry: {}", r.rc))?;
    let r = u.delete(&dn, vec![])?;
    rc_lines("delete own entry", &r, &mut out);
    check(r.rc == 53 || r.rc == 50, || format!("delete own entry: {}", r.rc))?;
    Ok(out)
}

fn projected_admin_view(c: &Ctx) -> Lines {
    let root = root(c)?;
    let mut a = c.admin()?;
    let f = a.find(&format!("cn=accounts,{root}"), Scope::OneLevel, "(objectClass=*)", &["objectClass", "os400-profile"])?;
    expect_rc("accounts", &f.res, 0)?;
    check(!f.entries.is_empty(), || "the administrator sees no profiles".into())?;
    let bad: Vec<&str> = f
        .entries
        .iter()
        .filter(|e| !values(e, "objectClass").iter().any(|o| o.eq_ignore_ascii_case("os400-usrprf")))
        .map(|e| e.dn.as_str())
        .collect();
    check(bad.is_empty(), || format!("not os400-usrprf: {bad:?}"))?;
    let has_secofr = f.entries.iter().any(|e| values(e, "os400-profile").iter().any(|p| p.eq_ignore_ascii_case("QSECOFR")));
    check(has_secofr, || "no QSECOFR entry".into())?;
    Ok(vec![format!("the administrator sees {} profile entries", f.entries.len())])
}
