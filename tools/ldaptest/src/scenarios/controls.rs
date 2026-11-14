// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Controls: paged results, sort, manageDsaIT and referrals, persistent
//! search, proxied authorization, tree delete, assertion, pre- and
//! post-read, unknown controls.

use std::collections::HashSet;
use std::thread;
use std::time::Duration;

use ldap3::controls::{
    Assertion, ManageDsaIt, PagedResults, PostRead, PreRead, ProxyAuth, RawControl, ReadEntryResp,
};
use ldap3::exop::{WhoAmI, WhoAmIResp};
use ldap3::{Mod, Scope};

use super::read::{bases, summary};
use super::{Ctx, Lines, Sandbox, Scenario};
use crate::client::{Conn, Opts, expect_rc, rc, rc_lines, values};
use crate::ctrl::{self, SortKey};
use crate::error::check;
use crate::target::Suite;
use crate::{Error, Target};

/// This suite.
pub const LIST: [Scenario; 19] = [
    o("paged-results", paged_results),
    o("paged-abandon", paged_abandon),
    o("paged-outstanding", paged_outstanding),
    o("paged-bad-cookie", paged_bad_cookie),
    o("sort", sort),
    o("sort-paged", sort_paged),
    o("manage-dsa-it", manage_dsa_it),
    o("psearch-accept", psearch_accept),
    o("proxied-authz-anon", proxied_authz_anon),
    o("assertion-read", assertion_read),
    o("unknown-control", unknown_control),
    w("paged-many", paged_many),
    w("sort-many", sort_many),
    w("referrals", referrals),
    w("persistent-search", persistent_search),
    w("proxied-authz", proxied_authz),
    w("tree-delete", tree_delete),
    w("assertion-write", assertion_write),
    w("pre-post-read", pre_post_read),
];

const fn o(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        suite: Suite::Controls,
        oracle: true,
        run,
    }
}

const fn w(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        suite: Suite::Controls,
        oracle: false,
        run,
    }
}

/// A paged search's outcome.
#[derive(Debug, Default)]
pub struct Paged {
    /// Pages fetched.
    pub pages: usize,
    /// Every entry's DN, in order.
    pub dns: Vec<String>,
    /// The last result.
    pub last: String,
    /// The last result's code.
    pub rc: u32,
}

/// Pages through `base` `size` at a time, `extra` controls on each request.
pub fn paged(
    c: &mut Conn,
    base: &str,
    scope: Scope,
    filter: &str,
    size: i32,
    extra: &[RawControl],
) -> Result<Paged, Error> {
    paged_with(c, base, scope, filter, size, extra, false)
}

/// [`paged`], the paged results control critical when `critical`.
pub fn paged_with(
    c: &mut Conn,
    base: &str,
    scope: Scope,
    filter: &str,
    size: i32,
    extra: &[RawControl],
    critical: bool,
) -> Result<Paged, Error> {
    let mut p = Paged::default();
    let mut cookie = Vec::new();
    loop {
        let mut ctrls = extra.to_vec();
        let mut pr: RawControl = PagedResults {
            size,
            cookie: cookie.clone(),
        }
        .into();
        pr.crit = critical;
        ctrls.push(pr);
        let f = c.search(
            base,
            scope,
            filter,
            &["1.1"],
            &Opts {
                ctrls,
                ..Opts::default()
            },
        )?;
        p.pages += 1;
        p.dns.extend(f.dns());
        p.last = rc(&f.res);
        p.rc = f.res.rc;
        if f.res.rc != 0 {
            return Ok(p);
        }
        let resp = ctrl::find(&f.res.ctrls, ctrl::PAGED)
            .and_then(|r| r.val.as_deref())
            .ok_or_else(|| Error::Check(format!("page {}: no paged results response", p.pages)))?;
        cookie = <PagedResults as ldap3::controls::ControlParser>::parse(resp).cookie;
        if cookie.is_empty() {
            return Ok(p);
        }
        check(p.pages < 1_000_000, || "paging does not end".into())?;
    }
}

fn paged_results(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        let all = a.find(&b, Scope::Subtree, "(objectClass=*)", &["1.1"])?;
        for size in [1, 2, 1000] {
            let p = paged(&mut a, &b, Scope::Subtree, "(objectClass=*)", size, &[])?;
            out.push(format!(
                "{b} size {size}: {} pages, {} entries, {}",
                p.pages,
                p.dns.len(),
                p.last
            ));
            if p.rc == 0 {
                let mut x = p.dns.clone();
                x.sort();
                let mut y = all.dns();
                y.sort();
                check(x == y, || {
                    format!("{b} size {size}: paged entries differ from a plain search")
                })?;
            }
        }
    }
    Ok(out)
}

fn paged_abandon(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        let f = a.search(
            &b,
            Scope::Subtree,
            "(objectClass=*)",
            &["1.1"],
            &Opts {
                ctrls: vec![
                    PagedResults {
                        size: 1,
                        cookie: vec![],
                    }
                    .into(),
                ],
                ..Opts::default()
            },
        )?;
        out.push(format!("{b} first page: {}", summary(&f)));
        let Some(cookie) = ctrl::find(&f.res.ctrls, ctrl::PAGED)
            .and_then(|r| r.val.as_deref())
            .map(|v| <PagedResults as ldap3::controls::ControlParser>::parse(v).cookie)
        else {
            out.push(format!("{b}: no paged response"));
            continue;
        };
        if cookie.is_empty() {
            out.push(format!("{b}: one page only"));
            continue;
        }
        let f = a.search(
            &b,
            Scope::Subtree,
            "(objectClass=*)",
            &["1.1"],
            &Opts {
                ctrls: vec![PagedResults { size: 0, cookie }.into()],
                ..Opts::default()
            },
        )?;
        let back = ctrl::find(&f.res.ctrls, ctrl::PAGED)
            .and_then(|r| r.val.as_deref())
            .map(|v| {
                <PagedResults as ldap3::controls::ControlParser>::parse(v)
                    .cookie
                    .len()
            });
        out.push(format!(
            "{b} size 0 with the cookie: {}, cookie octets {back:?}",
            summary(&f)
        ));
    }
    Ok(out)
}

/// Several paged searches left open at once on one connection (IBM keeps
/// `ibm-slapdPagedResLmt`), then each ended with a size of 0.
fn paged_outstanding(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let Some(b) = bases(c).into_iter().last() else {
        return crate::error::skip("no base to search");
    };
    let mut cookies = Vec::new();
    for i in 1..=4 {
        let f = a.search(
            &b,
            Scope::Subtree,
            "(objectClass=*)",
            &["1.1"],
            &Opts {
                ctrls: vec![
                    PagedResults {
                        size: 1,
                        cookie: vec![],
                    }
                    .into(),
                ],
                ..Opts::default()
            },
        )?;
        out.push(format!("{b} open {i}: {}", summary(&f)));
        if !f.res.text.is_empty() {
            out.push(format!("{b} open {i} diag: {}", f.res.text.trim()));
        }
        if let Some(v) = ctrl::find(&f.res.ctrls, ctrl::PAGED).and_then(|r| r.val.as_deref()) {
            cookies.push(<PagedResults as ldap3::controls::ControlParser>::parse(v).cookie);
        }
    }
    for (i, cookie) in cookies
        .into_iter()
        .enumerate()
        .filter(|(_, k)| !k.is_empty())
    {
        let f = a.search(
            &b,
            Scope::Subtree,
            "(objectClass=*)",
            &["1.1"],
            &Opts {
                ctrls: vec![PagedResults { size: 0, cookie }.into()],
                ..Opts::default()
            },
        )?;
        out.push(format!("{b} end {}: {}", i + 1, summary(&f)));
    }
    let f = a.search(
        &b,
        Scope::Subtree,
        "(objectClass=*)",
        &["1.1"],
        &Opts {
            ctrls: vec![
                PagedResults {
                    size: 1,
                    cookie: vec![],
                }
                .into(),
            ],
            ..Opts::default()
        },
    )?;
    out.push(format!("{b} open after ending them: {}", summary(&f)));
    Ok(out)
}

fn paged_bad_cookie(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        let ctrls = vec![
            PagedResults {
                size: 1,
                cookie: b"ldaptest-not-a-cookie".to_vec(),
            }
            .into(),
        ];
        let f = a.search(
            &b,
            Scope::Subtree,
            "(objectClass=*)",
            &["1.1"],
            &Opts {
                ctrls,
                ..Opts::default()
            },
        )?;
        out.push(format!("{b} bad cookie: {}", summary(&f)));
        if !f.res.text.is_empty() {
            out.push(format!("{b} bad cookie diag: {}", f.res.text.trim()));
        }
        // Critical, paged to the end so nothing is left outstanding.
        let mut fresh = c.anon()?;
        let p = paged_with(
            &mut fresh,
            &b,
            Scope::Subtree,
            "(objectClass=*)",
            1,
            &[],
            true,
        )?;
        out.push(format!(
            "{b} critical: {} pages, {} entries, {}",
            p.pages,
            p.dns.len(),
            p.last
        ));
    }
    Ok(out)
}

/// The sort response's lines.
fn sort_result(f: &crate::client::Found) -> String {
    match ctrl::find(&f.res.ctrls, ctrl::SORT_RESPONSE).and_then(|r| r.val.as_deref()) {
        Some(v) => match ctrl::sort_response(v) {
            Ok(r) => format!(
                "sortResult {} {}{}",
                r.result,
                crate::code::name(r.result as u32),
                r.attr.map(|a| format!(" attr {a}")).unwrap_or_default()
            ),
            Err(e) => format!("sort response undecodable: {e}"),
        },
        None => "no sort response".into(),
    }
}

/// Values of `attr` per entry, in the order returned.
fn first_values(f: &crate::client::Found, attr: &str) -> Vec<String> {
    f.entries
        .iter()
        .map(|e| {
            values(e, attr)
                .first()
                .map_or(String::new(), |s| (*s).to_owned())
        })
        .collect()
}

fn sort(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        for (label, keys, crit) in [
            ("cn asc", vec![SortKey::asc("cn")], true),
            ("cn desc", vec![SortKey::desc("cn")], true),
            (
                "objectclass,cn",
                vec![SortKey::asc("objectClass"), SortKey::asc("cn")],
                false,
            ),
            (
                "caseExactOrderingMatch",
                vec![SortKey {
                    attr: "cn".into(),
                    rule: Some("2.5.13.6".into()),
                    reverse: false,
                }],
                false,
            ),
            (
                "nosuchattribute critical",
                vec![SortKey::asc("nosuchattribute")],
                true,
            ),
            (
                "nosuchattribute",
                vec![SortKey::asc("nosuchattribute")],
                false,
            ),
            (
                "four keys",
                vec![
                    SortKey::asc("cn"),
                    SortKey::asc("sn"),
                    SortKey::asc("ou"),
                    SortKey::asc("o"),
                ],
                false,
            ),
        ] {
            for scope in [Scope::OneLevel, Scope::Subtree] {
                let sl = if scope == Scope::OneLevel {
                    "one"
                } else {
                    "sub"
                };
                let ctrls = vec![ctrl::sort_request(&keys, crit)];
                let f = a.search(
                    &b,
                    scope,
                    "(objectClass=*)",
                    &["cn"],
                    &Opts {
                        ctrls,
                        ..Opts::default()
                    },
                )?;
                out.push(format!(
                    "{b} {sl} {label}: {}, {}",
                    summary(&f),
                    sort_result(&f)
                ));
                if label.starts_with("cn ") && f.res.rc == 0 {
                    let got = first_values(&f, "cn");
                    out.push(format!("{b} {sl} {label} order: {}", got.join(" | ")));
                    let mut want = got.clone();
                    // Entries with no cn sort last ascending (RFC 2891 §1.2).
                    want.sort_by(|x, y| sort_cmp(x, y));
                    if label == "cn desc" {
                        want.reverse();
                    }
                    check(got == want, || {
                        format!("{b} {sl} {label}: order {got:?}, wanted {want:?}")
                    })?;
                }
            }
        }
    }
    Ok(out)
}

/// Case-insensitive, empty (no value) last.
fn sort_cmp(x: &str, y: &str) -> std::cmp::Ordering {
    match (x.is_empty(), y.is_empty()) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => x.to_lowercase().cmp(&y.to_lowercase()),
    }
}

fn sort_paged(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        let s = ctrl::sort_request(&[SortKey::asc("cn")], false);
        let p = paged(&mut a, &b, Scope::Subtree, "(objectClass=*)", 1, &[s])?;
        out.push(format!(
            "{b} sorted, size 1: {} pages, {} entries, {}",
            p.pages,
            p.dns.len(),
            p.last
        ));
        out.push(format!("{b} order: {}", p.dns.join(" | ")));
    }
    Ok(out)
}

fn manage_dsa_it(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let mut dns = bases(c);
    dns.push("dc=ldaptest-nowhere".into());
    for dn in dns {
        for crit in [false, true] {
            let mut m: RawControl = ManageDsaIt.into();
            m.crit = crit;
            let f = a.search(
                &dn,
                Scope::Base,
                "(objectClass=*)",
                &["1.1"],
                &Opts {
                    ctrls: vec![m],
                    ..Opts::default()
                },
            )?;
            out.push(format!(
                "{dn} manageDsaIT{}: {}",
                if crit { " critical" } else { "" },
                summary(&f)
            ));
        }
        let f = a.find(&dn, Scope::Base, "(objectClass=*)", &["1.1"])?;
        out.push(format!("{dn} without: {}", summary(&f)));
    }
    let f = a.find(
        bases(c).first().map_or("", String::as_str),
        Scope::Subtree,
        "(objectClass=referral)",
        &["ref"],
    )?;
    out.push(format!("referral objects: {}", summary(&f)));
    Ok(out)
}

fn psearch_accept(c: &Ctx) -> Lines {
    let mut out = Vec::new();
    let Some(b) = bases(c).into_iter().next() else {
        return crate::error::skip("no base to search");
    };
    let mut probe = c.anon()?;
    let n = probe
        .find(&b, Scope::OneLevel, "(objectClass=*)", &["1.1"])?
        .entries
        .len();
    let mut a = c.anon()?;
    let raw = a.raw();
    raw.with_controls(vec![ctrl::psearch(15, false, true)]);
    let mut st = raw.streaming_search(&b, Scope::OneLevel, "(objectClass=*)", vec!["1.1"])?;
    let mut got = 0;
    let mut ended = false;
    for _ in 0..n {
        match st.next()? {
            Some(re) if !re.is_ref() && !re.is_intermediate() => got += 1,
            Some(_) => {}
            None => {
                ended = true;
                break;
            }
        }
    }
    let id = st.last_id();
    let res = st.result();
    if ended {
        rc_lines(&format!("{b} psearch ended"), &res, &mut out);
    } else {
        out.push(format!(
            "{b} psearch: {got} of {n} initial entries, still open"
        ));
        let _ = a.raw().abandon(id);
    }
    // What the connection answers after the abandon, at once and a second later.
    let f = a.find("", Scope::Base, "(objectClass=*)", &["1.1"])?;
    out.push(format!("after abandon: {}", summary(&f)));
    thread::sleep(Duration::from_secs(1));
    let f = a.find("", Scope::Base, "(objectClass=*)", &["1.1"])?;
    out.push(format!("a second after abandon: {}", summary(&f)));
    let mut fresh = c.anon()?;
    let f = fresh.find("", Scope::Base, "(objectClass=*)", &["1.1"])?;
    out.push(format!("another connection: {}", summary(&f)));
    let mut a2 = c.anon()?;
    let mut nc = ctrl::psearch(15, true, true);
    nc.crit = false;
    let raw = a2.raw();
    raw.with_controls(vec![nc]);
    raw.with_timeout(Duration::from_secs(2));
    let mut st = raw.streaming_search(&b, Scope::OneLevel, "(objectClass=*)", vec!["1.1"])?;
    match st.next() {
        Ok(None) => {
            let res = st.result();
            rc_lines(
                &format!("{b} psearch non-critical changesOnly"),
                &res,
                &mut out,
            );
        }
        Ok(Some(_)) => out.push(format!(
            "{b} psearch non-critical changesOnly: an entry with no change"
        )),
        Err(ldap3::LdapError::Timeout { .. }) => out.push(format!(
            "{b} psearch non-critical changesOnly: waiting (2 s)"
        )),
        Err(e) => return Err(e.into()),
    }
    Ok(out)
}

fn proxied_authz_anon(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let b = bases(c).into_iter().next().unwrap_or_default();
    for id in [
        format!("dn:{}", c.t.admin_dn),
        String::new(),
        "u:ldaptest-nobody".into(),
    ] {
        let ctrls = vec![
            ProxyAuth {
                authzid: id.clone(),
            }
            .into(),
        ];
        let f = a.search(
            &b,
            Scope::Base,
            "(objectClass=*)",
            &["1.1"],
            &Opts {
                ctrls,
                ..Opts::default()
            },
        )?;
        out.push(format!("proxy \"{id}\": {}", summary(&f)));
        if !f.res.text.is_empty() {
            out.push(format!("proxy \"{id}\" diag: {}", f.res.text.trim()));
        }
    }
    Ok(out)
}

fn assertion_read(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    for b in bases(c) {
        let v = b.split_once('=').map_or("", |(_, v)| v).to_owned();
        for (flt, crit) in [
            (format!("(cn={v})"), true),
            ("(cn=ldaptest-no)".into(), true),
            ("(cn=ldaptest-no)".into(), false),
        ] {
            let mut ac = Assertion::new(flt.clone());
            ac.crit = crit;
            let f = a.search(
                &b,
                Scope::Base,
                "(objectClass=*)",
                &["1.1"],
                &Opts {
                    ctrls: vec![ac],
                    ..Opts::default()
                },
            )?;
            out.push(format!(
                "{b} assert {flt}{}: {}",
                if crit { " critical" } else { "" },
                summary(&f)
            ));
        }
        let mut ac = Assertion::new(format!("(cn={v})"));
        ac.crit = true;
        let r = {
            let raw = a.raw();
            raw.with_controls(vec![ac]);
            raw.compare(&b, "objectClass", "top")?.0
        };
        rc_lines(&format!("{b} compare with assertion"), &r, &mut out);
    }
    if c.dse.control(ctrl::ASSERTION) {
        let b = bases(c).into_iter().next().unwrap_or_default();
        let mut ac = Assertion::new("(cn=ldaptest-no)");
        ac.crit = true;
        let f = a.search(
            &b,
            Scope::Base,
            "(objectClass=*)",
            &["1.1"],
            &Opts {
                ctrls: vec![ac],
                ..Opts::default()
            },
        )?;
        expect_rc("false assertion", &f.res, 122)?;
    }
    Ok(out)
}

fn unknown_control(c: &Ctx) -> Lines {
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let b = bases(c).into_iter().next().unwrap_or_default();
    for crit in [true, false] {
        let ctrls = vec![RawControl {
            ctype: "1.3.6.1.4.1.99999.1.2.3".into(),
            crit,
            val: Some(vec![1, 2, 3]),
        }];
        let f = a.search(
            &b,
            Scope::Base,
            "(objectClass=*)",
            &["1.1"],
            &Opts {
                ctrls,
                ..Opts::default()
            },
        )?;
        rc_lines(
            &format!("unknown control{}", if crit { " critical" } else { "" }),
            &f.res,
            &mut out,
        );
        expect_rc("unknown control", &f.res, if crit { 12 } else { 0 })?;
    }
    // A known request control on an operation it does not apply to.
    let mut p: RawControl = PagedResults {
        size: 1,
        cookie: vec![],
    }
    .into();
    p.crit = true;
    let r = {
        let raw = a.raw();
        raw.with_controls(vec![p]);
        raw.compare(&b, "objectClass", "top")?.0
    };
    rc_lines("paged results on compare, critical", &r, &mut out);
    Ok(out)
}

// The scenarios below write; they need --suffix and the administrator.

fn paged_many(c: &Ctx) -> Lines {
    c.need_control(ctrl::PAGED, "paged results")?;
    let mut sb = c.sandbox("paged")?;
    fill(&mut sb, c.t.entries)?;
    let mut a = c.admin()?;
    let started = std::time::Instant::now();
    let p = paged(
        &mut a,
        &sb.dn,
        Scope::OneLevel,
        "(objectClass=person)",
        c.t.page,
        &[],
    )?;
    let took = started.elapsed();
    check(p.rc == 0, || format!("paged: {}", p.last))?;
    let uniq: HashSet<&String> = p.dns.iter().collect();
    check(
        p.dns.len() == c.t.entries && uniq.len() == c.t.entries,
        || {
            format!(
                "{} entries ({} distinct), wanted {}",
                p.dns.len(),
                uniq.len(),
                c.t.entries
            )
        },
    )?;
    let want_pages = c.t.entries.div_ceil(usize::try_from(c.t.page).unwrap_or(1));
    check(p.pages >= want_pages, || {
        format!("{} pages, wanted at least {want_pages}", p.pages)
    })?;
    // Anonymous users page too.
    let mut anon = c.anon()?;
    let pa = paged(
        &mut anon,
        &sb.dn,
        Scope::OneLevel,
        "(objectClass=person)",
        c.t.page,
        &[],
    )?;
    Ok(vec![
        format!(
            "{} entries in {} pages of {}, {:.0} ms",
            p.dns.len(),
            p.pages,
            c.t.page,
            took.as_secs_f64() * 1e3
        ),
        format!("anonymous: {} entries, {}", pa.dns.len(), pa.last),
    ])
}

/// Adds `n` people `cn=userNNNNNNN` (with `sn` in a different order) to the sandbox.
pub fn fill(sb: &mut Sandbox, n: usize) -> Result<(), Error> {
    for i in 0..n {
        let cn = format!("user{i:07}");
        let sn = format!("s{:07}", (i * 7919) % n.max(1));
        sb.add(
            &format!("cn={cn}"),
            &[
                (
                    "objectClass",
                    &["top", "person", "organizationalPerson", "inetOrgPerson"],
                ),
                ("cn", &[&cn]),
                ("sn", &[&sn]),
            ],
        )?;
    }
    Ok(())
}

fn sort_many(c: &Ctx) -> Lines {
    c.need_control(ctrl::SORT_REQUEST, "server-side sort")?;
    let mut sb = c.sandbox("sort")?;
    let n = c.t.entries.min(1000);
    fill(&mut sb, n)?;
    let mut a = c.admin()?;
    let mut out = Vec::new();
    for (label, key) in [
        ("sn asc", SortKey::asc("sn")),
        ("sn desc", SortKey::desc("sn")),
        ("cn desc", SortKey::desc("cn")),
    ] {
        let ctrls = vec![ctrl::sort_request(std::slice::from_ref(&key), true)];
        let f = sb_search(&mut a, &sb.dn, &key.attr, ctrls)?;
        expect_rc(label, &f.res, 0)?;
        let got = first_values(&f, &key.attr);
        let mut want = got.clone();
        want.sort_by(|x, y| sort_cmp(x, y));
        if key.reverse {
            want.reverse();
        }
        check(got == want && got.len() == n, || {
            format!("{label}: not in order ({} entries)", got.len())
        })?;
        out.push(format!(
            "{label}: {n} entries in order, {}",
            sort_result(&f)
        ));
    }
    Ok(out)
}

fn sb_search(
    a: &mut Conn,
    base: &str,
    attr: &str,
    ctrls: Vec<RawControl>,
) -> Result<crate::client::Found, Error> {
    a.search(
        base,
        Scope::OneLevel,
        "(objectClass=person)",
        &[attr],
        &Opts {
            ctrls,
            ..Opts::default()
        },
    )
}

fn referrals(c: &Ctx) -> Lines {
    c.need_control(ctrl::MANAGE_DSA_IT, "manageDsaIT")?;
    let mut sb = c.sandbox("ref")?;
    let url = "ldap://ldaptest.invalid/ou=far,dc=example";
    let rdn = "ou=ref";
    let dn = sb.add(
        rdn,
        &[
            ("objectClass", &["top", "referral", "extensibleObject"]),
            ("ou", &["ref"]),
            ("ref", &[url]),
        ],
    )?;
    let mut a = c.anon()?;
    let mut out = Vec::new();
    let f = a.find(&sb.dn, Scope::Subtree, "(objectClass=*)", &["1.1"])?;
    out.push(format!("subtree: {}", summary(&f)));
    check(
        f.refs
            .iter()
            .any(|r| r.iter().any(|u| u.starts_with("ldap://ldaptest.invalid/"))),
        || format!("no continuation reference: {:?}", f.refs),
    )?;
    let f = a.find(&dn, Scope::Base, "(objectClass=*)", &["1.1"])?;
    rc_lines("base on the referral", &f.res, &mut out);
    expect_rc("base on the referral", &f.res, 10)?;
    let f = a.find(
        &format!("cn=below,{dn}"),
        Scope::Base,
        "(objectClass=*)",
        &["1.1"],
    )?;
    rc_lines("below the referral", &f.res, &mut out);
    expect_rc("below the referral", &f.res, 10)?;
    let m: RawControl = ManageDsaIt.into();
    let f = a.search(
        &dn,
        Scope::Base,
        "(objectClass=*)",
        &["ref"],
        &Opts {
            ctrls: vec![m.clone()],
            ..Opts::default()
        },
    )?;
    expect_rc("manageDsaIT", &f.res, 0)?;
    let refs: Vec<&str> = f
        .entries
        .first()
        .map(|e| values(e, "ref"))
        .unwrap_or_default();
    check(refs == [url], || format!("ref {refs:?}"))?;
    out.push(format!("with manageDsaIT: {}", summary(&f)));
    // Compare and modify on it act on the referral object only with the control.
    let r = a.compare(&dn, "ou", "ref")?;
    rc_lines("compare without", &r, &mut out);
    let r = sb.admin.delete(&dn, vec![m])?;
    expect_rc("delete with manageDsaIT", &r, 0)?;
    Ok(out)
}

fn persistent_search(c: &Ctx) -> Lines {
    c.need_control(ctrl::PSEARCH, "persistent search")?;
    let sb = c.sandbox("psearch")?;
    let mut a = c.admin()?;
    let raw = a.raw();
    raw.with_controls(vec![ctrl::psearch(15, true, true)]);
    raw.with_timeout(Duration::from_secs(15));
    let mut st = raw.streaming_search(
        &sb.dn,
        Scope::Subtree,
        "(objectClass=*)",
        vec!["cn", "description"],
    )?;
    let t: Target = c.t.clone();
    let base = sb.dn.clone();
    let writer = thread::spawn(move || -> Result<(), Error> {
        thread::sleep(Duration::from_millis(500));
        let mut w = Conn::open(&t, crate::conn::Tls::None)?;
        let r = w.bind(&t.admin_dn, t.admin_pw.as_deref().unwrap_or(""))?;
        expect_rc("bind", &r, 0)?;
        let dn = format!("cn=ps,{base}");
        let r = w.add(
            &dn,
            &[
                ("objectClass", &["top", "person"]),
                ("cn", &["ps"]),
                ("sn", &["ps"]),
            ],
            vec![],
        )?;
        expect_rc("add", &r, 0)?;
        let r = w.modify(
            &dn,
            vec![Mod::Replace(
                "description".into(),
                HashSet::from(["changed".to_owned()]),
            )],
            vec![],
        )?;
        expect_rc("modify", &r, 0)?;
        let r = w.moddn(&dn, "cn=ps2", true, None)?;
        expect_rc("modify DN", &r, 0)?;
        let r = w.delete(&format!("cn=ps2,{base}"), vec![])?;
        expect_rc("delete", &r, 0)?;
        w.close();
        Ok(())
    });
    let mut kinds = Vec::new();
    let mut out = Vec::new();
    for _ in 0..4 {
        let Some(re) = st.next()? else { break };
        if re.is_ref() || re.is_intermediate() {
            continue;
        }
        let ecn = ctrl::find(&re.1, ctrl::ECN).and_then(|r| r.val.clone());
        let e = ldap3::SearchEntry::construct(re);
        let change = match ecn {
            Some(v) => ctrl::entry_change(&v)?,
            None => {
                return Err(Error::Check(format!(
                    "{}: no entry change notification",
                    e.dn
                )));
            }
        };
        out.push(format!(
            "change {} {}{}",
            change.kind,
            e.dn,
            change
                .previous_dn
                .as_ref()
                .map(|p| format!(" from {p}"))
                .unwrap_or_default()
        ));
        kinds.push(change.kind);
    }
    let id = st.last_id();
    drop(st.result());
    let _ = a.raw().abandon(id);
    writer
        .join()
        .map_err(|_| Error::Check("writer panicked".into()))??;
    check(
        kinds
            == [
                ctrl::change::ADD,
                ctrl::change::MODIFY,
                ctrl::change::MODDN,
                ctrl::change::DELETE,
            ],
        || format!("changes {kinds:?}, wanted add, modify, modDN, delete"),
    )?;
    drop(sb);
    Ok(out)
}

fn proxied_authz(c: &Ctx) -> Lines {
    c.need_control(ctrl::PROXY_AUTHZ, "proxied authorization")?;
    let mut sb = c.sandbox("proxy")?;
    let user = sb.user("pxuser", "Pass-word-1")?;
    let mut out = Vec::new();
    // The administrator acting as the user: the user's rights apply.
    let mut a = c.admin()?;
    let target = sb.child("ou=byproxy");
    let r = a.add(
        &target,
        &[
            ("objectClass", &["top", "organizationalUnit"]),
            ("ou", &["byproxy"]),
        ],
        vec![
            ProxyAuth {
                authzid: format!("dn:{user}"),
            }
            .into(),
        ],
    )?;
    rc_lines("add as the user by proxy", &r, &mut out);
    expect_rc("add as the user by proxy", &r, 50)?;
    if c.dse.extension("1.3.6.1.4.1.4203.1.11.3") {
        let (e, r) = a.extended(
            WhoAmI.into(),
            vec![
                ProxyAuth {
                    authzid: format!("dn:{user}"),
                }
                .into(),
            ],
        )?;
        expect_rc("whoami by proxy", &r, 0)?;
        let id =
            <WhoAmIResp as ldap3::exop::ExopParser>::parse(e.val.as_deref().unwrap_or(&[])).authzid;
        check(id.eq_ignore_ascii_case(&format!("dn:{user}")), || {
            format!("whoami by proxy: {id}")
        })?;
        out.push(format!("whoami by proxy: {id}"));
    }
    // The user may not act as the administrator.
    let mut u = c.bound(&user, "Pass-word-1")?;
    let f = u.search(
        &sb.dn,
        Scope::Base,
        "(objectClass=*)",
        &["1.1"],
        &Opts {
            ctrls: vec![
                ProxyAuth {
                    authzid: format!("dn:{}", c.t.admin_dn),
                }
                .into(),
            ],
            ..Opts::default()
        },
    )?;
    rc_lines("user proxying the administrator", &f.res, &mut out);
    expect_rc("user proxying the administrator", &f.res, 123)?;
    Ok(out)
}

fn tree_delete(c: &Ctx) -> Lines {
    c.need_control(ctrl::TREE_DELETE, "tree delete")?;
    let mut sb = c.sandbox("tdel")?;
    let top = sb.ou("top", None)?;
    let mid = sb.ou("mid", Some(&top))?;
    sb.ou("leaf1", Some(&mid))?;
    sb.ou("leaf2", Some(&mid))?;
    let mut out = Vec::new();
    let r = sb.admin.delete(&top, vec![])?;
    rc_lines("delete without the control", &r, &mut out);
    expect_rc("delete without the control", &r, 66)?;
    let r = sb
        .admin
        .delete(&top, vec![ctrl::bare(ctrl::TREE_DELETE, true)])?;
    expect_rc("tree delete", &r, 0)?;
    let f = sb
        .admin
        .find(&sb.dn, Scope::Subtree, "(objectClass=*)", &["1.1"])?;
    check(f.entries.len() == 1, || {
        format!("{} entries left", f.entries.len())
    })?;
    out.push(format!(
        "tree delete: {}, sandbox has {} entry",
        rc(&r),
        f.entries.len()
    ));
    // Anonymous users may not.
    let leaf = sb.ou("again", None)?;
    let mut a = c.anon()?;
    let r = a.delete(&leaf, vec![ctrl::bare(ctrl::TREE_DELETE, true)])?;
    rc_lines("anonymous tree delete", &r, &mut out);
    expect_rc("anonymous tree delete", &r, 50)?;
    Ok(out)
}

fn assertion_write(c: &Ctx) -> Lines {
    c.need_control(ctrl::ASSERTION, "assertion")?;
    let mut sb = c.sandbox("assert")?;
    let dn = sb.add(
        "cn=a",
        &[
            ("objectClass", &["top", "person"]),
            ("cn", &["a"]),
            ("sn", &["one"]),
        ],
    )?;
    let mut out = Vec::new();
    let replace = |v: &str| vec![Mod::Replace("sn".to_owned(), HashSet::from([v.to_owned()]))];
    let mut no = Assertion::new("(sn=other)");
    no.crit = true;
    let r = sb.admin.modify(&dn, replace("two"), vec![no])?;
    rc_lines("modify, false assertion", &r, &mut out);
    expect_rc("modify, false assertion", &r, 122)?;
    let mut yes = Assertion::new("(sn=one)");
    yes.crit = true;
    let r = sb.admin.modify(&dn, replace("two"), vec![yes])?;
    expect_rc("modify, true assertion", &r, 0)?;
    let f = sb
        .admin
        .find(&dn, Scope::Base, "(objectClass=*)", &["sn"])?;
    let sn: Vec<&str> = f
        .entries
        .first()
        .map(|e| values(e, "sn"))
        .unwrap_or_default();
    check(sn == ["two"], || format!("sn {sn:?}"))?;
    let mut no = Assertion::new("(sn=one)");
    no.crit = true;
    let r = sb.admin.delete(&dn, vec![no])?;
    expect_rc("delete, false assertion", &r, 122)?;
    out.push("modify and delete obey the assertion".into());
    Ok(out)
}

fn pre_post_read(c: &Ctx) -> Lines {
    c.need_control(ctrl::PRE_READ, "pre-read")?;
    c.need_control(ctrl::POST_READ, "post-read")?;
    let mut sb = c.sandbox("read")?;
    let dn = sb.add(
        "cn=r",
        &[
            ("objectClass", &["top", "person"]),
            ("cn", &["r"]),
            ("sn", &["before"]),
        ],
    )?;
    let r = sb.admin.modify(
        &dn,
        vec![Mod::Replace(
            "sn".to_owned(),
            HashSet::from(["after".to_owned()]),
        )],
        vec![
            PreRead::new(vec!["sn"]),
            PostRead::new(vec!["sn", "modifyTimestamp"]),
        ],
    )?;
    expect_rc("modify", &r, 0)?;
    let get = |oid: &str| -> Result<ReadEntryResp, Error> {
        let v = ctrl::find(&r.ctrls, oid)
            .and_then(|c| c.val.as_deref())
            .ok_or_else(|| Error::Check(format!("no {oid} response")))?;
        Ok(<ReadEntryResp as ldap3::controls::ControlParser>::parse(v))
    };
    let pre = get(ctrl::PRE_READ)?;
    let post = get(ctrl::POST_READ)?;
    let sn = |e: &ReadEntryResp| {
        e.attrs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("sn"))
            .map(|(_, v)| v.clone())
    };
    check(sn(&pre) == Some(vec!["before".into()]), || {
        format!("pre-read {:?}", pre.attrs)
    })?;
    check(sn(&post) == Some(vec!["after".into()]), || {
        format!("post-read {:?}", post.attrs)
    })?;
    // Post-read on an add, pre-read on a delete.
    let dn2 = sb.child("cn=r2");
    let r = sb.admin.add(
        &dn2,
        &[
            ("objectClass", &["top", "person"]),
            ("cn", &["r2"]),
            ("sn", &["x"]),
        ],
        vec![PostRead::new(vec!["cn"])],
    )?;
    expect_rc("add with post-read", &r, 0)?;
    check(ctrl::find(&r.ctrls, ctrl::POST_READ).is_some(), || {
        "add: no post-read response".into()
    })?;
    let r = sb.admin.delete(&dn2, vec![PreRead::new(vec!["sn"])])?;
    expect_rc("delete with pre-read", &r, 0)?;
    check(ctrl::find(&r.ctrls, ctrl::PRE_READ).is_some(), || {
        "delete: no pre-read response".into()
    })?;
    Ok(vec![
        format!("pre-read sn: {:?}", sn(&pre).unwrap_or_default()),
        format!(
            "post-read: {}",
            crate::client::names(&post.attrs, &post.bin_attrs).join(" ")
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_order_puts_missing_last() {
        let mut v = vec!["b".to_owned(), String::new(), "A".into(), "c".into()];
        v.sort_by(|x, y| sort_cmp(x, y));
        assert_eq!(v, ["A", "b", "c", ""]);
    }
}
