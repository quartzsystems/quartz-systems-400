// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/autl[/<name>[/objects|/remove|/delete]]`: authorization lists
//! (`WRKAUTL`), a list's entries (`DSPAUTL`) with Add (`ADDAUTLE`),
//! Change (`CHGAUTLE`) and Remove (`RMVAUTLE`, confirmed), the objects
//! it secures (`DSPAUTLOBJ`), Create (`CRTAUTL`) and Delete (`DLTAUTL`,
//! confirmed).

use std::cmp::Reverse;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, checkbox, form, input, list_table,
    props,
};

use super::{confirm_page, page, run};
use crate::env::users::{AuthEntry, AuthList};
use crate::routes::Rq;
use crate::routes::common::{Confirm, mono, pick, quote, strings};

/// The authorities an entry may have.
const AUTHORITIES: &[&str] = &["*USE", "*CHANGE", "*ALL", "*EXCLUDE", "*AUTL"];

/// Route `/autl/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq),
        ["create"] => create(rq),
        [name, rest @ ..] => {
            let name = name.to_ascii_uppercase();
            let Some(l) = rq
                .env()
                .users
                .lists(rq.user())
                .into_iter()
                .find(|l| l.name == name)
            else {
                return rq.not_found(&format!("authorization list {name}"));
            };
            match rest {
                [] => entries(rq, &l),
                ["objects"] => objects(rq, &l),
                ["remove"] => remove(rq, &l),
                ["delete"] => delete(rq, &l),
                _ => rq.not_found(&format!(
                    "page {} of authorization list {name}",
                    rest.join("/")
                )),
            }
        }
    }
}

fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        return rq.redirect("/autl/create");
    }
    let mut ls = rq.env().users.lists(rq.user());
    match rq.query().str("sort") {
        "n" => ls.sort_by(|a, b| a.name.cmp(&b.name)),
        "o" => ls.sort_by_key(|l| Reverse(l.objects)),
        "e" => ls.sort_by_key(|l| Reverse(l.entries.len())),
        _ => {}
    }
    let rows = ls
        .iter()
        .map(|l| {
            let base = format!("/autl/{}", l.name);
            vec![
                Cell::act(actions(&[
                    (rq.ctx.url(&base), "Open"),
                    (rq.ctx.url(&format!("{base}/objects")), "Objects"),
                    (rq.ctx.url(&format!("{base}/delete")), "Delete"),
                ])),
                Cell::mono(&l.name),
                Cell::text(&l.text),
                Cell::num(&l.objects.to_string()),
                Cell::num(&l.entries.len().to_string()),
                Cell::mono(&l.owner),
            ]
        })
        .collect();
    let table = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Authorization list", "n"),
            Col::plain("Text description"),
            Col::sorted("Objects secured", "o"),
            Col::sorted("Entries", "e"),
            Col::plain("Owner"),
        ],
        rows,
        empty: "There are no authorization lists.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}{}",
        list_table(&table),
        form(&rq.ctx, "", &buttons(&[Button::bare("Create...")]))
    );
    page(
        rq,
        &["Authorization lists"],
        "Authorization lists",
        Some("Authorization lists (WRKAUTL)."),
        body,
        "autl",
        Some("/autl"),
    )
}

/// The ten object authorities and list management an entry gives.
fn marks(e: &AuthEntry) -> [bool; 11] {
    let (opr, mgt, data) = match e.authority.as_str() {
        "*ALL" => (true, true, [true; 5]),
        "*CHANGE" => (true, false, [true; 5]),
        "*USE" => (true, false, [true, false, false, false, true]),
        _ => (false, false, [false; 5]),
    };
    [
        opr, mgt, mgt, mgt, mgt, data[0], data[1], data[2], data[3], data[4], e.list_mgt,
    ]
}

/// `AUT()` of an authority and list management.
fn aut(level: &str, mgt: bool) -> String {
    if mgt {
        format!("{level} *AUTLMGT")
    } else {
        level.to_string()
    }
}

/// `/autl/<name>`: the entries, Add and Change.
#[allow(clippy::too_many_lines)]
fn entries(rq: &Rq<'_>, l: &AuthList) -> Response {
    let here = format!("/autl/{}", l.name);
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect(&here);
        }
        let a = AUTHORITIES
            .iter()
            .find(|a| f.str("a") == **a)
            .copied()
            .unwrap_or("*USE");
        let u = f.str("u").trim().to_ascii_uppercase();
        let cmd = if f.has("change") {
            format!(
                "CHGAUTLE AUTL({}) USER({u}) AUT({})",
                l.name,
                aut(a, f.has("m"))
            )
        } else {
            format!("ADDAUTLE AUTL({}) USER({u}) AUT({a})", l.name)
        };
        return run(rq, &cmd, &here);
    }
    let mut sorted = l.entries.clone();
    match rq.query().str("sort") {
        "u" => sorted.sort_by(|a, b| a.user.cmp(&b.user)),
        "a" => sorted.sort_by(|a, b| a.authority.cmp(&b.authority)),
        _ => {}
    }
    let rows = sorted
        .iter()
        .map(|e| {
            let mut r = vec![
                Cell::act(actions(&[
                    (rq.ctx.url(&format!("{here}?edit={}", e.user)), "Change"),
                    (
                        rq.ctx.url(&format!("{here}/remove?user={}", e.user)),
                        "Remove",
                    ),
                ])),
                Cell::mono(&e.user),
                Cell::mono(&e.authority),
            ];
            r.extend(
                marks(e)
                    .iter()
                    .map(|m| Cell::text(if *m { "X" } else { "" })),
            );
            r
        })
        .collect();
    let mut cols = vec![
        Col::plain("Actions"),
        Col::sorted("User", "u"),
        Col::sorted("Object authority", "a"),
    ];
    cols.extend(
        [
            "Opr", "Mgt", "Exist", "Alter", "Ref", "Read", "Add", "Upd", "Dlt", "Exec", "List mgt",
        ]
        .iter()
        .map(|c| Col::plain(c)),
    );
    let table = List {
        cols,
        rows,
        empty: "The list has no entries.".into(),
        keep: Vec::new(),
    };
    let edit = rq.query().get("edit").map(str::to_ascii_uppercase);
    let editing = edit.and_then(|u| l.entries.iter().find(|e| e.user == u).cloned());
    let tail = match editing {
        Some(e) => form(
            &rq.ctx,
            "",
            &format!(
                "<h3>Change entry</h3><table class=\"props\"><tr><th>User</th><td>{}<input type=\"hidden\" name=\"u\" value=\"{}\"></td></tr><tr><th>Authority</th><td>{}</td></tr><tr><th>List management</th><td>{}</td></tr></table>{}",
                mono(&e.user),
                esc(&e.user),
                pick("a", &strings(AUTHORITIES), &e.authority),
                checkbox("m", e.list_mgt),
                buttons(&[
                    Button::new("change", "Change"),
                    Button::new("cancel", "Cancel")
                ])
            ),
        ),
        None => form(
            &rq.ctx,
            "",
            &format!(
                "<h3>Add entry</h3><table class=\"props\"><tr><th>User</th><td>{}</td></tr><tr><th>Authority</th><td>{}</td></tr></table>{}",
                input("u", "", 10),
                pick("a", &strings(AUTHORITIES), ""),
                buttons(&[Button::new("add", "Add")])
            ),
        ),
    };
    let head = format!(
        "<p class=\"small\">Authorization list: <b class=\"mono\">{}</b> &nbsp; Owner: {} &nbsp; Secures {} objects &nbsp; <a href=\"{}\">All lists</a></p>",
        esc(&l.name),
        mono(&l.owner),
        l.objects,
        esc(&rq.ctx.url("/autl"))
    );
    let title = format!("Authorization list: {}", l.name);
    page(
        rq,
        &["Authorization lists", &l.name],
        &title,
        None,
        format!("{head}{}{tail}", list_table(&table)),
        "autl",
        None,
    )
}

/// `/autl/<name>/objects`: the objects the list secures.
fn objects(rq: &Rq<'_>, l: &AuthList) -> Response {
    let rows = rq
        .env()
        .users
        .secured(rq.user(), &l.name)
        .into_iter()
        .map(|o| {
            vec![
                Cell::mono(&o.object),
                Cell::mono(&o.lib),
                Cell::mono(&o.typ),
                Cell::mono(&o.owner),
            ]
        })
        .collect();
    let table = List {
        cols: vec![
            Col::plain("Object"),
            Col::plain("Library"),
            Col::plain("Type"),
            Col::plain("Owner"),
        ],
        rows,
        empty: "The list secures no objects.".into(),
        keep: Vec::new(),
    };
    let head = format!(
        "<p class=\"small\">Authorization list: <b class=\"mono\">{}</b> &nbsp; <a href=\"{}\">Entries</a> &nbsp; <a href=\"{}\">All lists</a></p>",
        esc(&l.name),
        esc(&rq.ctx.url(&format!("/autl/{}", l.name))),
        esc(&rq.ctx.url("/autl"))
    );
    let title = format!("Objects secured by {}", l.name);
    page(
        rq,
        &["Authorization lists", &l.name, "Objects"],
        &title,
        Some("The objects the authorization list secures (DSPAUTLOBJ)."),
        format!("{head}{}", list_table(&table)),
        "autl",
        None,
    )
}

/// `/autl/<name>/remove?user=`: confirm `RMVAUTLE`.
fn remove(rq: &Rq<'_>, l: &AuthList) -> Response {
    let here = format!("/autl/{}", l.name);
    let u = rq.query().str("user").trim().to_ascii_uppercase();
    let Some(e) = l.entries.iter().find(|e| e.user == u) else {
        return rq.not_found(&format!("entry {u} of authorization list {}", l.name));
    };
    let command = format!("RMVAUTLE AUTL({}) USER({u})", l.name);
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect(&here);
        }
        return run(rq, &command, &here);
    }
    confirm_page(
        rq,
        &Confirm {
            what: "Remove Authorization List Entry",
            crumb: &["Authorization lists", &l.name],
            sentence: "The user loses the authority the list gives to the objects it secures. Press OK to remove the entry listed below, or Cancel to return without removing it.".into(),
            cols: &["Authorization list", "User", "Object authority"],
            rows: vec![vec![mono(&l.name), mono(&e.user), mono(&aut(&e.authority, e.list_mgt))]],
            options: String::new(),
            command,
        },
    )
}

/// `/autl/<name>/delete`: confirm `DLTAUTL`.
fn delete(rq: &Rq<'_>, l: &AuthList) -> Response {
    let command = format!("DLTAUTL AUTL({})", l.name);
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/autl");
        }
        return run(rq, &command, "/autl");
    }
    confirm_page(
        rq,
        &Confirm {
            what: "Delete Authorization List",
            crumb: &["Authorization lists"],
            sentence: "The objects the list secures are no longer secured by it. Press OK to delete the list below, or Cancel to return without deleting it.".into(),
            cols: &["Authorization list", "Text description", "Objects secured", "Owner"],
            rows: vec![vec![mono(&l.name), esc(&l.text), l.objects.to_string(), mono(&l.owner)]],
            options: String::new(),
            command,
        },
    )
}

/// `/autl/create`: name and text, then `CRTAUTL`.
fn create(rq: &Rq<'_>) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/autl");
        }
        let name = f.str("name").trim().to_ascii_uppercase();
        let mut c = format!("CRTAUTL AUTL({name}) AUT(*EXCLUDE)");
        let text = f.str("text").trim();
        if !text.is_empty() {
            c = format!("{c} TEXT({})", quote(text));
        }
        return run(rq, &c, "/autl");
    }
    let body = props(
        &rq.ctx,
        &[PropGroup {
            title: None,
            rows: vec![
                Prop::new("Authorization list", input("name", "", 10)).required(),
                Prop::new("Text description", input("text", "", 40)),
            ],
        }],
        &[Button::new("ok", "Create"), Button::new("cancel", "Cancel")],
    );
    page(
        rq,
        &["Authorization lists", "Create"],
        "Create authorization list",
        Some("A new list with you as its owner and *PUBLIC excluded (CRTAUTL)."),
        body,
        "autl",
        None,
    )
}
