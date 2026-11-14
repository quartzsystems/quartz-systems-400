// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Extended operations, TLS and SASL: an unknown operation, whoami,
//! password modify, `StartTLS`, LDAPS, SASL EXTERNAL.

use ldap3::Scope;
use ldap3::exop::{Exop, ExopParser, PasswordModify, PasswordModifyResp, WhoAmI, WhoAmIResp};

use super::read::summary;
use super::{Ctx, Lines, Scenario};
use crate::client::{Conn, expect_rc, rc_lines};
use crate::conn::Tls;
use crate::error::{check, skip};
use crate::target::Suite;
use crate::{Error, ber};

/// `StartTLS`.
pub const STARTTLS: &str = "1.3.6.1.4.1.1466.20037";
/// whoami (RFC 4532).
pub const WHOAMI: &str = "1.3.6.1.4.1.4203.1.11.3";
/// Password modify (RFC 3062).
pub const PASSMOD: &str = "1.3.6.1.4.1.4203.1.11.1";

/// This suite.
pub const LIST: [Scenario; 9] = [
    o("unknown-extop", unknown_extop),
    o("whoami-anon", whoami_anon),
    w("password-modify-anon", password_modify_anon),
    o("starttls", starttls),
    o("ldaps", ldaps),
    w("sasl-external", sasl_external),
    w("sasl-external-no-cert", sasl_external_no_cert),
    w("whoami-bound", whoami_bound),
    w("password-modify", password_modify),
];

const fn o(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario { name, suite: Suite::Ext, oracle: true, run }
}

const fn w(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario { name, suite: Suite::Ext, oracle: false, run }
}

fn unknown_extop(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for (oid, val) in [("1.3.6.1.4.1.99999.9.9", Some(vec![1u8, 2, 3])), ("1.3.6.1.4.1.99999.9.8", None)] {
        let (e, r) = a.extended(Exop { name: Some(oid.into()), val }, vec![])?;
        rc_lines(&format!("extended {oid}"), &r, &mut out);
        out.push(format!("extended {oid} response name: {:?}", e.name));
        check(r.rc != 0, || format!("{oid}: accepted"))?;
    }
    // The connection still serves.
    let f = a.find("", Scope::Base, "(objectClass=*)", &["1.1"])?;
    out.push(format!("search after: {}", summary(&f)));
    Ok(out)
}

fn whoami_id(e: &Exop) -> String {
    <WhoAmIResp as ExopParser>::parse(e.val.as_deref().unwrap_or(&[])).authzid
}

fn whoami_anon(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let (e, r) = a.extended(WhoAmI.into(), vec![])?;
    rc_lines("whoami", &r, &mut out);
    if c.dse.extension(WHOAMI) {
        expect_rc("whoami", &r, 0)?;
        let id = whoami_id(&e);
        check(id.is_empty(), || format!("anonymous whoami: {id:?}"))?;
        out.push(format!("authzid: {id:?}"));
    } else {
        out.push("whoami not advertised".into());
    }
    Ok(out)
}

fn password_modify_anon(c: &Ctx) -> Lines {
    // No identity and no passwords: nothing a server could change. Still a
    // write request, so not run against the oracle.
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let (_, r) = a.extended(PasswordModify { user_id: None, old_pass: None, new_pass: None }.into(), vec![])?;
    rc_lines("password modify, anonymous", &r, &mut out);
    check(r.rc != 0, || "anonymous password modify succeeded".into())?;
    Ok(out)
}

fn starttls(c: &Ctx) -> Lines {
    let mut out = Vec::new();
    match Conn::open(&c.t, Tls::StartTls) {
        Ok(mut s) => {
            let f = s.find("", Scope::Base, "(objectClass=*)", &["security", "supportedExtension"])?;
            out.push(format!("StartTLS: established; root DSE {}", summary(&f)));
            // A second StartTLS on a protected connection is an error (RFC 4511 §4.14.1).
            let (_, r) = s.extended(Exop { name: Some(STARTTLS.into()), val: None }, vec![])?;
            rc_lines("second StartTLS", &r, &mut out);
            check(r.rc != 0, || "a second StartTLS succeeded".into())?;
            s.close();
        }
        Err(Error::Ldap(ldap3::LdapError::LdapResult { result })) => {
            rc_lines("StartTLS", &result, &mut out);
            if c.dse.extension(STARTTLS) && !c.t.is_oracle() && c.dse.one("security").is_some_and(|s| s != "none") {
                return Err(Error::Check(format!("StartTLS refused: {}", crate::client::rc(&result))));
            }
        }
        Err(e) => out.push(format!("StartTLS: {e}")),
    }
    // StartTLS with a value is a protocol error.
    let mut a = c.anon()?;
    let (_, r) = a.extended(Exop { name: Some(STARTTLS.into()), val: Some(ber::octets(b"x")) }, vec![])?;
    rc_lines("StartTLS with a value", &r, &mut out);
    Ok(out)
}

fn ldaps(c: &Ctx) -> Lines {
    let mut out = Vec::new();
    match Conn::open(&c.t, Tls::Ldaps) {
        Ok(mut s) => {
            let f = s.find("", Scope::Base, "(objectClass=*)", &["security", "port"])?;
            out.push(format!("LDAPS: established; root DSE {}", summary(&f)));
            if let Some(e) = f.entries.first() {
                out.push(format!("security: {}", crate::client::values(e, "security").join(" ")));
            }
            let (_, r) = s.extended(Exop { name: Some(STARTTLS.into()), val: None }, vec![])?;
            rc_lines("StartTLS over LDAPS", &r, &mut out);
            s.close();
        }
        Err(Error::Ldap(ldap3::LdapError::Io { source })) => {
            out.push(format!("LDAPS port {}: {:?}", c.t.ldaps_port, source.kind()));
        }
        Err(Error::Ldap(ldap3::LdapError::Timeout { .. })) => {
            out.push(format!("LDAPS port {}: timed out", c.t.ldaps_port));
        }
        Err(e) => out.push(format!("LDAPS: {e}")),
    }
    Ok(out)
}

fn sasl_external(c: &Ctx) -> Lines {
    if c.t.client.is_none() {
        return skip("no --client-cert");
    }
    if !c.dse.sasl("EXTERNAL") {
        return skip("EXTERNAL not in supportedSASLMechanisms");
    }
    let mut out = Vec::new();
    for tls in [Tls::StartTls, Tls::Ldaps] {
        let mut s = Conn::open_with(&c.t, tls, true)?;
        let r = s.external()?;
        expect_rc(&format!("EXTERNAL over {tls:?}"), &r, 0)?;
        if c.dse.extension(WHOAMI) {
            let (e, r) = s.extended(WhoAmI.into(), vec![])?;
            expect_rc("whoami", &r, 0)?;
            let id = whoami_id(&e);
            check(id.starts_with("dn:"), || format!("EXTERNAL identity {id:?}"))?;
            out.push(format!("{tls:?}: {id}"));
        } else {
            out.push(format!("{tls:?}: bound"));
        }
        s.close();
    }
    Ok(out)
}

fn sasl_external_no_cert(c: &Ctx) -> Lines {
    let mut out = Vec::new();
    let mut a = c.anon()?;
    let r = a.external()?;
    rc_lines("EXTERNAL in the clear", &r, &mut out);
    check(r.rc != 0, || "EXTERNAL with no certificate succeeded".into())?;
    if let Ok(mut s) = Conn::open(&c.t, Tls::StartTls) {
        let r = s.external()?;
        rc_lines("EXTERNAL over TLS with no certificate", &r, &mut out);
        check(r.rc != 0, || "EXTERNAL with no certificate succeeded".into())?;
    }
    Ok(out)
}

fn whoami_bound(c: &Ctx) -> Lines {
    c.need_ext(WHOAMI, "whoami")?;
    let mut sb = c.sandbox("whoami")?;
    let user = sb.user("who", "Pass-word-1")?;
    let mut u = c.bound(&user, "Pass-word-1")?;
    let (e, r) = u.extended(WhoAmI.into(), vec![])?;
    expect_rc("whoami", &r, 0)?;
    let id = whoami_id(&e);
    check(id.eq_ignore_ascii_case(&format!("dn:{user}")), || format!("whoami {id}"))?;
    let mut a = c.admin()?;
    let (e, _) = a.extended(WhoAmI.into(), vec![])?;
    Ok(vec![format!("user: {id}"), format!("administrator: {}", whoami_id(&e))])
}

fn password_modify(c: &Ctx) -> Lines {
    c.need_ext(PASSMOD, "password modify")?;
    let mut sb = c.sandbox("passmod")?;
    let user = sb.user("pm", "Old-pass-1")?;
    let mut out = Vec::new();
    // The user changes their own password.
    let mut u = c.bound(&user, "Old-pass-1")?;
    let (_, r) = u.extended(PasswordModify { user_id: None, old_pass: Some("Old-pass-1"), new_pass: Some("New-pass-2") }.into(), vec![])?;
    expect_rc("own password", &r, 0)?;
    u.close();
    let mut x = c.anon()?;
    let r = x.bind(&user, "Old-pass-1")?;
    expect_rc("old password", &r, 49)?;
    c.bound(&user, "New-pass-2")?.close();
    out.push("own change: old refused, new accepted".into());
    // A wrong old password.
    let mut u = c.bound(&user, "New-pass-2")?;
    let (_, r) = u.extended(PasswordModify { user_id: None, old_pass: Some("wrong"), new_pass: Some("Other-3") }.into(), vec![])?;
    rc_lines("wrong old password", &r, &mut out);
    check(r.rc != 0, || "a wrong old password was taken".into())?;
    // The administrator resets it, with a generated password.
    let mut a = c.admin()?;
    let (e, r) = a.extended(PasswordModify { user_id: Some(&format!("dn:{user}")), old_pass: None, new_pass: None }.into(), vec![])?;
    expect_rc("reset", &r, 0)?;
    let generated = <PasswordModifyResp as ExopParser>::parse(e.val.as_deref().unwrap_or(&[])).gen_pass;
    check(!generated.is_empty(), || "no generated password".into())?;
    c.bound(&user, &generated)?.close();
    out.push(format!("reset: a generated password of {} characters binds", generated.chars().count()));
    // Anonymous users may not change another's.
    let mut n = c.anon()?;
    let (_, r) = n.extended(PasswordModify { user_id: Some(&format!("dn:{user}")), old_pass: None, new_pass: Some("x-Y-z-9") }.into(), vec![])?;
    rc_lines("anonymous change of another's", &r, &mut out);
    check(r.rc != 0, || "anonymous change succeeded".into())?;
    Ok(out)
}
