// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/user/<name>/...`: a profile's properties (`DSPUSRPRF`) by tab,
//! Apply as `CHGUSRPRF`, Reset password (`CHGUSRPRF PASSWORD()
//! PWDEXP()`) and Delete (`DLTUSRPRF`, confirmed, asking what becomes of
//! the objects the profile owns).

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, buttons, checkbox, form, input, input_cmd, list_table, props,
    props_table, radio, select, tabs,
};

use super::list::{CLASSES, in_group, status};
use super::{GROUP, confirm_page, page, run, secadm};
use crate::env::users::Profile;
use crate::env::users::SPECIAL;
use crate::routes::Rq;
use crate::routes::common::{Confirm, mono, pick, quote, strings};

/// The tabs: label and the route's last segment (Jobs is the active
/// jobs list).
const TABS: &[(&str, &str)] = &[
    ("General", "general"),
    ("Groups", "groups"),
    ("Authorities", "authorities"),
    ("Jobs", ""),
    ("Personal", "personal"),
];

/// Route a profile's page.
pub fn route(rq: &Rq<'_>, p: &Profile, rest: &[&str]) -> Response {
    match rest {
        ["general"] => general(rq, p),
        ["groups"] => groups(rq, p),
        ["authorities"] => authorities(rq, p),
        ["personal"] => personal(rq, p),
        ["password"] => password(rq, p),
        ["delete"] => delete(rq, p),
        _ => rq.not_found(&format!("page {} of user profile {}", rest.join("/"), p.name)),
    }
}

/// The tabs line with `current` bold.
fn tab_line(rq: &Rq<'_>, p: &Profile, current: usize) -> String {
    let t: Vec<(&str, String)> = TABS
        .iter()
        .map(|(l, r)| {
            let u = if r.is_empty() { format!("/jobs?user={}", p.name) } else { format!("/user/{}/{r}", p.name) };
            (*l, rq.ctx.url(&u))
        })
        .collect();
    tabs(&t, current)
}

/// A tab's page.
fn tab_page(rq: &Rq<'_>, p: &Profile, body: String) -> Response {
    let title = format!("User: {}", p.name);
    page(rq, &["Users", &p.name], &title, None, body, "user", None)
}

/// The group profiles' names.
fn group_names(rq: &Rq<'_>) -> Vec<String> {
    rq.env().users.profiles(rq.user()).into_iter().filter(|g| g.is_group).map(|g| g.name).collect()
}

/// `*NONE` for empty.
fn or_none(v: &str) -> String {
    if v.is_empty() { "*NONE".into() } else { v.to_string() }
}

/// The supplemental groups as typed (`*NONE` for none).
fn supplemental(p: &Profile) -> String {
    if p.supplemental.is_empty() { "*NONE".into() } else { p.supplemental.join(" ") }
}

/// `SPCAUT`'s value of a list.
fn spcaut(list: &[String]) -> String {
    if list.is_empty() { "*NONE".into() } else { list.join(" ") }
}

/// After Apply or OK: back to the tab, or to Users.
fn after(rq: &Rq<'_>, p: &Profile, tab: &str) -> String {
    if rq.form().has("ok") { "/users".into() } else { format!("/user/{}/{tab}", p.name) }
}

/// Run `CHGUSRPRF` with `changes`, or go back when there are none.
fn change(rq: &Rq<'_>, p: &Profile, changes: &str, back: &str) -> Response {
    if changes.is_empty() {
        return rq.redirect(back);
    }
    if let Err(r) = secadm(rq, "Change user profile", &p.name, &[GROUP, "Users", &p.name]) {
        return r;
    }
    run(rq, &format!("CHGUSRPRF USRPRF({}){changes}", p.name), back)
}

/// `KEYWORD(value)` appended when `new` differs from `old`.
fn differ(c: &mut String, kw: &str, old: &str, new: &str) {
    let new = new.trim();
    if !new.is_empty() && new != old {
        let _ = write!(c, " {kw}({new})");
    }
}

fn general(rq: &Rq<'_>, p: &Profile) -> Response {
    if rq.post() {
        return general_post(rq, p);
    }
    general_page(rq, p)
}

/// General's POST: Reset password, or Apply and OK as one `CHGUSRPRF`.
fn general_post(rq: &Rq<'_>, p: &Profile) -> Response {
    {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/users");
        }
        let here = format!("/user/{}/general", p.name);
        if f.has("reset") {
            let pw = f.str("pw");
            if pw.is_empty() {
                return rq.done(&here, rq.message(Kind::Error, "CPD0071", &["PASSWORD"]));
            }
            let exp = if f.has("exp") { "*YES" } else { "*NO" };
            if let Err(r) = secadm(rq, "Reset password", &p.name, &[GROUP, "Users", &p.name]) {
                return r;
            }
            return run(rq, &format!("CHGUSRPRF USRPRF({}) PASSWORD({}) PWDEXP({exp})", p.name, quote(pw)), &here);
        }
        let mut c = String::new();
        let text = f.str("t").trim();
        if f.has("t") && text != p.text {
            let _ = write!(c, " TEXT({})", quote(text));
        }
        let st = if p.enabled { "*ENABLED" } else { "*DISABLED" };
        differ(&mut c, "STATUS", st, f.str("s"));
        differ(&mut c, "USRCLS", &p.class, f.str("c"));
        differ(&mut c, "GRPPRF", &or_none(&p.group), f.str("g"));
        differ(&mut c, "SUPGRPPRF", &supplemental(p), &f.str("sg").to_ascii_uppercase());
        differ(&mut c, "PWDEXPITV", &p.pwd_interval, &f.str("pi").to_ascii_uppercase());
        differ(&mut c, "INLPGM", &p.initial_program, &f.str("ip").to_ascii_uppercase());
        differ(&mut c, "INLMNU", &p.initial_menu, &f.str("im").to_ascii_uppercase());
        differ(&mut c, "CURLIB", &p.current_library, &f.str("cl").to_ascii_uppercase());
        differ(&mut c, "LMTCPB", &p.limit_capabilities, f.str("lc"));
        differ(&mut c, "JOBD", &p.jobd, &f.str("jd").to_ascii_uppercase());
        differ(&mut c, "OUTQ", &p.outq, &f.str("oq").to_ascii_uppercase());
        differ(&mut c, "MSGQ", &p.msgq, &f.str("mq").to_ascii_uppercase());
        let hd = f.str("hd").trim();
        if !hd.is_empty() && hd != p.home {
            let _ = write!(c, " HOMEDIR({})", quote(hd));
        }
        let chosen: Vec<String> = SPECIAL
            .iter()
            .enumerate()
            .filter(|(i, _)| f.has(&format!("a{}", i + 1)))
            .map(|(_, s)| (*s).to_string())
            .collect();
        let had: Vec<String> = SPECIAL.iter().filter(|s| p.special.iter().any(|h| h == *s)).map(|s| (*s).to_string()).collect();
        if chosen != had {
            let _ = write!(c, " SPCAUT({})", spcaut(&chosen));
        }
        change(rq, p, &c, &after(rq, p, "general"))
    }
}

/// General's page.
fn general_page(rq: &Rq<'_>, p: &Profile) -> Response {
    let mut groups = vec![or_none(&p.group)];
    if !p.group.is_empty() {
        groups.push("*NONE".into());
    }
    groups.extend(group_names(rq).into_iter().filter(|g| *g != p.group));
    let st = if p.enabled { "*ENABLED" } else { "*DISABLED" };
    let (changed, _) = rq.stamp(p.pwd_changed);
    let last = p.last_signon.map(|s| rq.when(s)).unwrap_or_default();
    let mut spc = String::new();
    for (i, s) in SPECIAL.iter().enumerate() {
        if i > 0 {
            spc.push(' ');
        }
        let _ = write!(spc, "{} {s}", checkbox(&format!("a{}", i + 1), p.special.iter().any(|h| h == s)));
    }
    let pw = format!(
        "<input type=\"password\" name=\"pw\" size=\"12\"> &nbsp; <input type=\"submit\" name=\"reset\" value=\"Reset password...\"> &nbsp; Set to expired: {}",
        checkbox("exp", false)
    );
    let gs = [
        PropGroup {
            title: Some("Profile".into()),
            rows: vec![
                Prop::new("User profile", mono(&p.name)),
                Prop::new("Text description", input("t", &p.text, 40)),
                Prop::new("Status", pick("s", &strings(&["*ENABLED", "*DISABLED"]), st)),
                Prop::new("User class", pick("c", &strings(CLASSES), &p.class)),
                Prop::new("Group profile", pick("g", &groups, "")),
                Prop::new("Supplemental groups", input("sg", &supplemental(p), 30)),
            ],
        },
        PropGroup {
            title: Some("Password".into()),
            rows: vec![
                Prop::new("Password", pw),
                Prop::new("Password expiration interval", format!("{} days", input("pi", &p.pwd_interval, 8))),
                Prop::new("Last changed", changed),
                Prop::new("Sign-on attempts not valid", p.invalid_signons.to_string()),
                Prop::new("Last sign-on", last),
            ],
        },
        PropGroup {
            title: Some("Job attributes".into()),
            rows: vec![
                Prop::new("Initial program", input_cmd("ip", &p.initial_program, 20)),
                Prop::new("Initial menu", input_cmd("im", &p.initial_menu, 20)),
                Prop::new("Current library", input_cmd("cl", &p.current_library, 10)),
                Prop::new("Limit capabilities", select("lc", &["*YES", "*PARTIAL", "*NO"], &p.limit_capabilities)),
                Prop::new("Job description", input_cmd("jd", &p.jobd, 20)),
                Prop::new("Output queue", input_cmd("oq", &p.outq, 20)),
                Prop::new("Message queue", input_cmd("mq", &p.msgq, 20)),
                Prop::new("Home directory", input_cmd("hd", &p.home, 30)),
            ],
        },
        PropGroup {
            title: Some("Special authorities".into()),
            rows: vec![Prop::new("", spc)],
        },
    ];
    let body = props(
        &rq.ctx,
        &gs,
        &[Button::new("apply", "Apply"), Button::new("ok", "OK"), Button::new("cancel", "Cancel")],
    );
    tab_page(rq, p, format!("{}{body}", tab_line(rq, p, 0)))
}

fn groups(rq: &Rq<'_>, p: &Profile) -> Response {
    let all = rq.env().users.profiles(rq.user());
    let text = |g: &str| all.iter().find(|q| q.name == g).map(|q| q.text.clone()).unwrap_or_default();
    let link = |n: &str| format!("<a href=\"{}\">{}</a>", esc(&rq.ctx.url(&format!("/user/{n}/general"))), esc(n));
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    if !p.group.is_empty() {
        rows.push(vec![Cell::mono_html(link(&p.group)), Cell::text("Primary"), Cell::text(&text(&p.group))]);
    }
    for s in &p.supplemental {
        rows.push(vec![Cell::mono_html(link(s)), Cell::text("Supplemental"), Cell::text(&text(s))]);
    }
    let list = List {
        cols: vec![Col::plain("Group profile"), Col::plain("Membership"), Col::plain("Text description")],
        rows,
        empty: "The profile is in no group.".into(),
        keep: Vec::new(),
    };
    let mut body = format!("{}{}", tab_line(rq, p, 1), list_table(&list));
    if p.is_group {
        let members = List {
            cols: vec![Col::plain("Member"), Col::plain("Membership"), Col::plain("Text description")],
            rows: all
                .iter()
                .filter(|m| in_group(m, &p.name))
                .map(|m| {
                    let how = if m.group == p.name { "Primary" } else { "Supplemental" };
                    vec![Cell::mono_html(link(&m.name)), Cell::text(how), Cell::text(&m.text)]
                })
                .collect(),
            empty: "The group has no members.".into(),
            keep: Vec::new(),
        };
        let gid = p.gid.map_or("*NONE".into(), |g| g.to_string());
        let _ = write!(body, "<h3>Members</h3><p class=\"small\">Group ID: {}</p>{}", mono(&gid), list_table(&members));
    }
    let _ = write!(
        body,
        "<p class=\"small grey\">Change the groups on <a href=\"{}\">General</a>.</p>",
        esc(&rq.ctx.url(&format!("/user/{}/general", p.name)))
    );
    tab_page(rq, p, body)
}

fn authorities(rq: &Rq<'_>, p: &Profile) -> Response {
    let spc = if p.special.is_empty() { "*NONE".to_string() } else { p.special.join(" ") };
    let rows = rq
        .env()
        .users
        .authorities(rq.user(), &p.name)
        .into_iter()
        .map(|a| {
            vec![
                Cell::mono(&a.object),
                Cell::mono(&a.lib),
                Cell::mono(&a.typ),
                Cell::mono(&a.authority),
                Cell::text(if a.owner { "Yes" } else { "" }),
            ]
        })
        .collect();
    let list = List {
        cols: vec![Col::plain("Object"), Col::plain("Library"), Col::plain("Type"), Col::plain("Authority"), Col::plain("Owner")],
        rows,
        empty: "The profile owns no objects and has no private authorities.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}<p class=\"small\">Special authorities: {} &nbsp; Objects owned: {}</p>{}",
        tab_line(rq, p, 2),
        mono(&spc),
        p.owned,
        list_table(&list)
    );
    tab_page(rq, p, body)
}

fn personal(rq: &Rq<'_>, p: &Profile) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/users");
        }
        let mut c = String::new();
        let text = f.str("t").trim();
        if f.has("t") && text != p.text {
            let _ = write!(c, " TEXT({})", quote(text));
        }
        differ(&mut c, "LANGID", &p.language, &f.str("lang").to_ascii_uppercase());
        differ(&mut c, "CNTRYID", &p.country, &f.str("cntry").to_ascii_uppercase());
        differ(&mut c, "CCSID", &p.ccsid, f.str("ccsid"));
        return change(rq, p, &c, &after(rq, p, "personal"));
    }
    let gs = [PropGroup {
        title: Some("Personal".into()),
        rows: vec![
            Prop::new("Text description", input("t", &p.text, 40)),
            Prop::new("Language ID", input("lang", &p.language, 8)),
            Prop::new("Country or region ID", input("cntry", &p.country, 4)),
            Prop::new("Coded character set ID", input("ccsid", &p.ccsid, 6)),
        ],
    }];
    let body = props(
        &rq.ctx,
        &gs,
        &[Button::new("apply", "Apply"), Button::new("ok", "OK"), Button::new("cancel", "Cancel")],
    );
    tab_page(rq, p, format!("{}{body}", tab_line(rq, p, 4)))
}

/// `/user/<name>/password`: Reset password from the list.
fn password(rq: &Rq<'_>, p: &Profile) -> Response {
    let here = format!("/user/{}/password", p.name);
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/users");
        }
        let pw = f.str("pw");
        if pw.is_empty() {
            return rq.done(&here, rq.message(Kind::Error, "CPD0071", &["PASSWORD"]));
        }
        if pw != f.str("pw2") {
            return rq.done(&here, rq.message(Kind::Error, "CPF2361", &[]));
        }
        if let Err(r) = secadm(rq, "Reset password", &p.name, &[GROUP, "Users", &p.name]) {
            return r;
        }
        let exp = if f.has("exp") { "*YES" } else { "*NO" };
        return run(rq, &format!("CHGUSRPRF USRPRF({}) PASSWORD({}) PWDEXP({exp})", p.name, quote(pw)), "/users");
    }
    let gs = [PropGroup {
        title: None,
        rows: vec![
            Prop::new("User profile", mono(&p.name)),
            Prop::new("New password", "<input type=\"password\" name=\"pw\" size=\"12\">".into()),
            Prop::new("Confirm password", "<input type=\"password\" name=\"pw2\" size=\"12\">".into()),
            Prop::new("Set to expired", checkbox("exp", true)),
        ],
    }];
    let inner = format!(
        "{}<p class=\"small grey\">Equivalent command: <span class=\"mono\">CHGUSRPRF USRPRF({}) PASSWORD() PWDEXP(*YES)</span></p>{}",
        props_table(rq.ctx.product, &gs, true),
        esc(&p.name),
        buttons(&[Button::new("ok", "Reset password"), Button::new("cancel", "Cancel")])
    );
    let title = format!("Reset password: {}", p.name);
    page(
        rq,
        &["Users", &p.name, "Reset password"],
        &title,
        Some("Give the profile a new password (CHGUSRPRF). Set to expired makes the user change it at the next sign-on."),
        form(&rq.ctx, "", &inner),
        "user",
        None,
    )
}

/// `/user/<name>/delete`: confirm `DLTUSRPRF`, choosing what becomes of
/// the profile's objects.
fn delete(rq: &Rq<'_>, p: &Profile) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/users");
        }
        let opt = match f.str("own") {
            "*DLT" => "*DLT".to_string(),
            "*CHGOWN" => format!("*CHGOWN {}", f.str("to").trim().to_ascii_uppercase()),
            _ => "*NODLT".to_string(),
        };
        if let Err(r) = secadm(rq, "Delete user profile", &p.name, &[GROUP, "Users"]) {
            return r;
        }
        let back = if p.is_group { "/groups" } else { "/users" };
        return run(rq, &format!("DLTUSRPRF USRPRF({}) OWNOBJOPT({opt})", p.name), back);
    }
    let options = format!(
        "<table class=\"props\"><tr><th>Objects the profile owns</th><td>{} Do not delete the profile if it owns objects<br>{} Delete the objects<br>{} Give them to user profile {}</td></tr></table>",
        radio("own", "*NODLT", true),
        radio("own", "*DLT", false),
        radio("own", "*CHGOWN", false),
        input("to", "QSECOFR", 10)
    );
    let what = if p.is_group { "Delete Group Profile" } else { "Delete User Profile" };
    confirm_page(
        rq,
        &Confirm {
            what,
            crumb: &[if p.is_group { "Groups" } else { "Users" }],
            sentence: "Deleting a user profile removes it and its private authorities; it cannot be undone. Press OK to delete the profile listed below, or Cancel to return without deleting it.".into(),
            cols: &["User profile", "Text description", "Status", "User class", "Objects owned"],
            rows: vec![vec![mono(&p.name), esc(&p.text), status(p), esc(&p.class), p.owned.to_string()]],
            options,
            command: format!("DLTUSRPRF USRPRF({}) OWNOBJOPT(*NODLT)", p.name),
        },
    )
}
