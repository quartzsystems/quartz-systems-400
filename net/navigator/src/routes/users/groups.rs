// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/groups`: the group profiles and their members; Create group is
//! `CRTUSRPRF PASSWORD(*NONE) GID(*GEN)`. A group's Properties and
//! Delete are its profile's pages.

use std::cmp::Reverse;

use qsweb::http::Response;
use qsweb::partials::{Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, input, list_table, props};

use super::list::in_group;
use super::{GROUP, page, run, secadm};
use crate::env::users::Profile;
use crate::routes::Rq;
use crate::routes::common::quote;

/// The list; its POST (Create group...) goes to the form.
pub fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        return rq.redirect("/groups/create");
    }
    let all = rq.env().users.profiles(rq.user());
    let members = |g: &Profile| all.iter().filter(|p| in_group(p, &g.name)).count();
    let mut gs: Vec<(&Profile, usize)> = all.iter().filter(|p| p.is_group).map(|g| (g, members(g))).collect();
    match rq.query().str("sort") {
        "g" => gs.sort_by(|a, b| a.0.name.cmp(&b.0.name)),
        "m" => gs.sort_by_key(|(_, n)| Reverse(*n)),
        _ => {}
    }
    let rows = gs
        .iter()
        .map(|(g, n)| {
            vec![
                Cell::act(actions(&[
                    (rq.ctx.url(&format!("/users?g={}", g.name)), "Members"),
                    (rq.ctx.url(&format!("/user/{}/general", g.name)), "Properties"),
                    (rq.ctx.url(&format!("/user/{}/delete", g.name)), "Delete"),
                ])),
                Cell::mono(&g.name),
                Cell::text(&g.text),
                Cell::num(&n.to_string()),
                Cell::num(&g.gid.map_or("*NONE".into(), |g| g.to_string())),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Group profile", "g"),
            Col::plain("Text description"),
            Col::sorted("Members", "m"),
            Col::plain("GID"),
        ],
        rows,
        empty: "There are no group profiles.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}{}",
        list_table(&list),
        form(&rq.ctx, "", &buttons(&[Button::bare("Create group...")]))
    );
    page(rq, &["Groups"], "Groups", None, body, "groups", Some("/groups"))
}


/// `/groups/create`: the group's name and text, then `CRTUSRPRF`.
pub fn create(rq: &Rq<'_>) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/groups");
        }
        let name = f.str("name").trim().to_ascii_uppercase();
        if let Err(r) = secadm(rq, "Create group profile", &name, &[GROUP, "Groups"]) {
            return r;
        }
        let mut c = format!("CRTUSRPRF USRPRF({name}) PASSWORD(*NONE) GID(*GEN)");
        let text = f.str("text").trim();
        if !text.is_empty() {
            c = format!("{c} TEXT({})", quote(text));
        }
        return run(rq, &c, "/groups");
    }
    let body = props(
        &rq.ctx,
        &[PropGroup {
            title: None,
            rows: vec![
                Prop::new("Group profile", input("name", "", 10)).required(),
                Prop::new("Text description", input("text", "", 40)),
            ],
        }],
        &[Button::new("ok", "Create"), Button::new("cancel", "Cancel")],
    );
    let body = format!(
        "{body}<p class=\"small grey\">Equivalent command: <span class=\"mono\">CRTUSRPRF USRPRF(name) PASSWORD(*NONE) GID(*GEN) TEXT('text')</span></p>"
    );
    page(
        rq,
        &["Groups", "Create group"],
        "Create group",
        Some("A group profile has no password; users name it as their group (CRTUSRPRF)."),
        body,
        "groups",
        None,
    )
}
