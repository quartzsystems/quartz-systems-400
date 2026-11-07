// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/users?p=&s=&c=&g=&sort=&page=`: the user profiles (`WRKUSRPRF`);
//! group profiles are on Groups.

use std::cmp::Reverse;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, FilterField, List, Paging, actions, buttons, filter, form, list_table,
    paging,
};

use super::page;
use crate::env::users::Profile;
use crate::routes::Rq;
use crate::routes::common::{all_or, keep, matches, strings, word};

/// The Status filter's choices.
const STATUS: &[&str] = &["*ALL", "*ENABLED", "*DISABLED"];
/// The user classes.
pub const CLASSES: &[&str] = &["*USER", "*SYSOPR", "*PGMR", "*SECADM", "*SECOFR"];

/// A profile's status word, green or red.
pub fn status(p: &Profile) -> String {
    if p.enabled {
        word("*ENABLED", Some(true))
    } else {
        word("*DISABLED", Some(false))
    }
}

/// Whether `p` is in group `g` (primary or supplemental).
pub fn in_group(p: &Profile, g: &str) -> bool {
    p.group == g || p.supplemental.iter().any(|s| s == g)
}

fn sort(ps: &mut [Profile], key: &str) {
    match key {
        "p" => ps.sort_by(|a, b| a.name.cmp(&b.name)),
        "t" => ps.sort_by(|a, b| a.text.cmp(&b.text)),
        "s" => ps.sort_by_key(|p| !p.enabled),
        "c" => ps.sort_by(|a, b| a.class.cmp(&b.class)),
        "l" => ps.sort_by_key(|p| Reverse(p.last_signon)),
        "g" => ps.sort_by(|a, b| a.group.cmp(&b.group)),
        _ => {}
    }
}

/// The list.
#[allow(clippy::too_many_lines)]
pub fn list(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let pat = all_or(q.str("p"));
    let st = STATUS
        .iter()
        .find(|s| q.str("s") == **s)
        .copied()
        .unwrap_or("*ALL");
    let mut classes = vec!["*ALL"];
    classes.extend_from_slice(CLASSES);
    let cl = classes
        .iter()
        .find(|c| q.str("c") == **c)
        .copied()
        .unwrap_or("*ALL");
    let grp = all_or(q.str("g"));
    let key = q.str("sort").to_string();
    let mut ps: Vec<Profile> = rq
        .env()
        .users
        .profiles(rq.user())
        .into_iter()
        .filter(|p| !p.is_group && matches(&pat, &p.name))
        .filter(|p| st == "*ALL" || p.enabled == (st == "*ENABLED"))
        .filter(|p| cl == "*ALL" || p.class == cl)
        .filter(|p| grp == "*ALL" || in_group(p, &grp))
        .collect();
    sort(&mut ps, &key);
    let filters = keep(&[("p", &pat), ("s", st), ("c", cl), ("g", &grp)]);
    let mut kept = filters.clone();
    if !key.is_empty() {
        kept.push(("sort".into(), key.clone()));
    }
    let p = Paging::from_query(q.str("page"), q.str("rows"), ps.len(), kept);
    let chosen = |v: &str| {
        if v == "*ALL" {
            String::new()
        } else {
            v.to_string()
        }
    };
    let f = filter(
        &[
            FilterField::Text("Profile".into(), "p".into(), pat.clone(), 10),
            FilterField::Select("Status".into(), "s".into(), strings(STATUS), chosen(st)),
            FilterField::Select("Class".into(), "c".into(), strings(&classes), chosen(cl)),
            FilterField::Text("Group".into(), "g".into(), grp.clone(), 10),
        ],
        false,
    );
    let rows = ps[p.range()]
        .iter()
        .map(|u| {
            let base = format!("/user/{}", u.name);
            let props = rq.ctx.url(&format!("{base}/general"));
            let last = u.last_signon.map(|s| rq.when(s)).unwrap_or_default();
            vec![
                Cell::act(actions(&[
                    (props.clone(), "Properties"),
                    (rq.ctx.url(&format!("{base}/delete")), "Delete"),
                    (rq.ctx.url(&format!("{base}/password")), "Reset password"),
                    (rq.ctx.url(&format!("/jobs?user={}", u.name)), "Jobs"),
                ])),
                Cell::mono_html(format!("<a href=\"{}\">{}</a>", esc(&props), esc(&u.name))),
                Cell::text(&u.text),
                Cell::mono_html(status(u)),
                Cell::mono(&u.class),
                Cell::text(&last),
                Cell::mono(&u.group),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Profile", "p"),
            Col::sorted("Text description", "t"),
            Col::sorted("Status", "s"),
            Col::sorted("User class", "c"),
            Col::sorted("Last sign-on", "l"),
            Col::sorted("Group", "g"),
        ],
        rows,
        empty: "No user profiles match the filter.".into(),
        keep: filters,
    };
    let create = form(
        &rq.ctx,
        &rq.ctx.url("/users/create/step/1"),
        &buttons(&[
            Button::new("create", "Create user..."),
            Button::new("refresh", "Refresh"),
        ]),
    );
    let body = format!("{f}{}{}{create}", list_table(&list), paging(&p));
    page(
        rq,
        &["Users"],
        "Users",
        Some("User profiles on the system (WRKUSRPRF)."),
        body,
        "users",
        Some("/users"),
    )
}
