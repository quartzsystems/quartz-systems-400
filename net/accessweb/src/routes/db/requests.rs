// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! My requests (`/requests?t=&o=&sort=`): saved SQL scripts and copy
//! requests, the user's and those shared with one of its groups or
//! `*PUBLIC`. Run and Edit open Run SQL or the copy page with the
//! request; Download gives the `.sql` or `.qdx` file (the desktop's
//! formats); Copy, Share and Delete (through the confirmation) are the
//! owner's. A `.qdx` or `.sql` file uploaded becomes a request.

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::Kind as MsgKind;
use qsweb::pages::confirm_body;
use qsweb::partials::{Button, Cell, Col, List, buttons, form, input, list_table, upload_form};

use super::state;
use super::{GROUP, crumb, pick, short_stamp};
use crate::data::qdx;
use crate::env::db::{ReqType, Request};
use crate::routes::Rq;
use crate::{PRODUCT, msgs, policy};

const CRUMB: &[&str] = &[GROUP, "My requests"];

/// The Type select's choices.
const TYPES: &[&str] = &["All", "SQL", "Copy data to table", "Copy data from table"];
/// The Owner select's choices.
const OWNERS: &[&str] = &["Mine", "Shared with me", "All"];

/// Route `/requests/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    if let Err(r) = rq.policy(policy::DB_BROWSE, CRUMB) {
        return r;
    }
    match rest {
        [] => list(rq),
        ["save"] => save(rq),
        [id, what @ ..] => {
            let Some(r) = id.parse::<u32>().ok().and_then(|id| find(rq, id)) else {
                return rq.not_found(&format!("request {id}"));
            };
            match what {
                ["download"] => download(&r),
                ["copy"] => copy(rq, &r),
                ["share"] if r.owner == rq.user() => share(rq, &r),
                ["delete", "confirm"] if r.owner == rq.user() => delete(rq, &r),
                _ => rq.not_found(&format!("request {id}")),
            }
        }
    }
}

fn find(rq: &Rq<'_>, id: u32) -> Option<Request> {
    let groups = rq.env().system.groups(rq.user());
    rq.env()
        .db
        .requests(rq.user(), &groups)
        .into_iter()
        .find(|r| r.id == id)
}

/// What a copy request moves (`vendors.csv -> HLINV.MANUFACTURERS`).
pub fn source(r: &Request) -> String {
    match r.typ {
        ReqType::Sql => {
            let one = r.body.split_whitespace().collect::<Vec<_>>().join(" ");
            if one.len() <= 80 {
                return one;
            }
            let mut out = String::new();
            for w in one.split(' ') {
                if out.len() + w.len() + 1 > 75 {
                    break;
                }
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(w);
            }
            out.push_str(" ...");
            out
        }
        _ => match qdx::from_text(&r.body) {
            Ok(qdx::Request::Upload { upload: u, .. }) => {
                format!("{} -> {}.{}", u.file, u.schema, u.table)
            }
            Ok(qdx::Request::Download { download: d, .. }) => {
                let from = match &d.source {
                    qdx::Source::Table { schema, table, .. } => format!("{schema}.{table}"),
                    qdx::Source::Query { sql } => {
                        sql.split_whitespace().take(6).collect::<Vec<_>>().join(" ")
                    }
                };
                format!("{from} -> {}", d.file)
            }
            Err(e) => e.to_string(),
        },
    }
}

/// Where Run and Edit go.
fn open_url(r: &Request, run: bool) -> String {
    let id = r.id.to_string();
    match r.typ {
        ReqType::Sql => PRODUCT.url(&format!(
            "/sql{}",
            query(&[("req", &id), ("run", if run { "1" } else { "" })])
        )),
        ReqType::CopyTo => PRODUCT.url(&format!("/db/copy/to{}", query(&[("req", &id)]))),
        ReqType::CopyFrom => PRODUCT.url(&format!("/db/copy/from{}", query(&[("req", &id)]))),
    }
}

#[allow(clippy::too_many_lines)]
fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        return import(rq);
    }
    let q = rq.query();
    let t = q.get("t").filter(|t| TYPES.contains(t)).unwrap_or("All");
    let o = q.get("o").filter(|o| OWNERS.contains(o)).unwrap_or("Mine");
    let groups = rq.env().system.groups(rq.user());
    let mut all: Vec<Request> = rq
        .env()
        .db
        .requests(rq.user(), &groups)
        .into_iter()
        .filter(|r| t == "All" || r.typ.label() == t)
        .filter(|r| match o {
            "Mine" => r.owner == rq.user(),
            "Shared with me" => r.owner != rq.user(),
            _ => true,
        })
        .collect();
    match q.str("sort") {
        "n" => all.sort_by(|a, b| a.name.cmp(&b.name)),
        "l" => all.sort_by_key(|a| std::cmp::Reverse(a.last_run)),
        _ => {}
    }
    let url = |r: &str| esc(&PRODUCT.url(r));
    let share = rq.allowed(policy::DB_SHARE);
    let save = rq.allowed(policy::DB_SAVE);
    let rows = all
        .iter()
        .map(|r| {
            let mine = r.owner == rq.user();
            let base = format!("/requests/{}", r.id);
            let mut acts = vec![format!("<a href=\"{}\">Run</a>", esc(&open_url(r, true)))];
            if mine {
                acts.push(format!("<a href=\"{}\">Edit</a>", esc(&open_url(r, false))));
            }
            acts.push(format!(
                "<a href=\"{}\">Download .qdx / .sql</a>",
                url(&format!("{base}/download"))
            ));
            if mine && save {
                acts.push(format!(
                    "<a href=\"{}\">Copy</a>",
                    url(&format!("{base}/copy"))
                ));
            }
            if mine {
                acts.push(format!(
                    "<a href=\"{}\">Delete</a>",
                    url(&format!("{base}/delete/confirm"))
                ));
            }
            if mine && share {
                acts.push(format!(
                    "<a href=\"{}\">Share</a>",
                    url(&format!("{base}/share"))
                ));
            }
            vec![
                Cell::act(acts.join(" | ")),
                Cell::text(&r.name),
                Cell::text(r.typ.label()),
                Cell::html(qsweb::partials::mono(&source(r))),
                Cell::text(&r.format),
                Cell::text(&short_stamp(rq, r.last_run)),
                Cell::text(&r.shared),
            ]
        })
        .collect();
    let lst = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Request", "n"),
            Col::plain("Type"),
            Col::plain("Statement / source"),
            Col::plain("Format"),
            Col::sorted("Last run", "l"),
            Col::plain("Shared with"),
        ],
        rows,
        empty: "No requests.".into(),
        keep: Vec::new(),
    };
    let mut body = format!(
        "<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr><td>Type:</td><td>{}</td><td>Owner:</td><td>{}</td><td><input type=\"submit\" value=\"Go\"> </td></tr></table></form>",
        super::picks("t", TYPES, t),
        super::picks("o", OWNERS, o)
    );
    body.push_str(&list_table(&lst));
    body.push_str(&upload_form(
        &rq.ctx,
        "",
        &format!(
            "<div class=\"buttons\"><input type=\"submit\" value=\"New SQL request...\" formaction=\"{}\"><input type=\"submit\" value=\"New copy request...\" formaction=\"{}\"> &nbsp; Upload a .qdx or .sql: <input type=\"file\" name=\"f\" size=\"25\"> <input type=\"submit\" value=\"Import\"></div>",
            url("/sql"),
            url("/db/copy/to")
        ),
    ));
    rq.page(Page {
        title: "My requests".into(),
        crumb: crumb(&["My requests"]),
        h1: "My requests".into(),
        desc: Some("Saved SQL scripts and transfer requests; .qdx and .sql files are the desktop's own formats.".into()),
        body,
        help: "db-requests".into(),
        nav: Some((GROUP, Some("/requests"))),
        ..Page::default()
    })
}

/// The format a request's body keeps.
fn copy_format(body: &str) -> String {
    match qdx::from_text(body) {
        Ok(qdx::Request::Upload { upload: u, .. }) => u.options.format.file().label().into(),
        Ok(qdx::Request::Download { download: d, .. }) => d.options.format.file().label().into(),
        Err(_) => String::new(),
    }
}

/// `POST /requests` with a file: a `.qdx` or `.sql` becomes a request.
fn import(rq: &Rq<'_>) -> Response {
    if let Err(r) = rq.policy(policy::DB_SAVE, CRUMB) {
        return r;
    }
    let Some(p) = rq
        .req
        .multipart()
        .unwrap_or_default()
        .into_iter()
        .find(|p| p.name == "f" && p.filename.is_some())
    else {
        return rq.done(
            "/requests",
            rq.message(
                MsgKind::Error,
                msgs::FILE_UNREADABLE,
                &["(none)", "no file was sent"],
            ),
        );
    };
    let name = p.filename.clone().unwrap_or_default();
    let Ok(text) = String::from_utf8(p.data) else {
        return rq.done(
            "/requests",
            rq.message(
                MsgKind::Error,
                msgs::NOT_A_REQUEST,
                &[&name, "it is not text"],
            ),
        );
    };
    let stem = name
        .rsplit_once('.')
        .map_or(name.as_str(), |x| x.0)
        .to_string();
    let r = if name.to_ascii_lowercase().ends_with(".sql") {
        Request {
            name: stem,
            typ: ReqType::Sql,
            body: text,
            format: "HTML".into(),
            ..Request::default()
        }
    } else {
        match qdx::from_text(&text) {
            Ok(q) => Request {
                name: stem,
                typ: if matches!(q, qdx::Request::Upload { .. }) {
                    ReqType::CopyTo
                } else {
                    ReqType::CopyFrom
                },
                format: copy_format(&text),
                body: text,
                ..Request::default()
            },
            Err(e) => {
                return rq.done(
                    "/requests",
                    rq.message(
                        MsgKind::Error,
                        msgs::NOT_A_REQUEST,
                        &[&name, &e.to_string()],
                    ),
                );
            }
        }
    };
    let name = r.name.clone();
    match rq.env().db.save_request(rq.user(), &r) {
        Ok(_) => rq.done(
            "/requests",
            rq.message(MsgKind::Done, msgs::REQUEST_SAVED, &[&name]),
        ),
        Err(m) => rq.done("/requests", m),
    }
}

/// Keep a request; `/requests` with its message.
pub fn keep(rq: &Rq<'_>, r: &Request) -> Response {
    if let Err(resp) = rq.policy(policy::DB_SAVE, CRUMB) {
        return resp;
    }
    match rq.env().db.save_request(rq.user(), r) {
        Ok(_) => rq.done(
            "/requests",
            rq.message(MsgKind::Done, msgs::REQUEST_SAVED, &[&r.name]),
        ),
        Err(m) => rq.done("/requests", m),
    }
}

/// `GET /requests/save?type=sql`: name the Run SQL script; `POST`
/// keeps it.
fn save(rq: &Rq<'_>) -> Response {
    if let Err(r) = rq.policy(policy::DB_SAVE, CRUMB) {
        return r;
    }
    let script = state::work(rq).lock().expect("work").sql.script.clone();
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/sql");
        }
        let name = f.str("name").trim().to_string();
        if name.is_empty() {
            return rq.done(
                "/requests/save",
                rq.message(MsgKind::Error, msgs::BAD_VALUE, &["", "Request name"]),
            );
        }
        let fmt = f.str("fmt").to_string();
        return keep(
            rq,
            &Request {
                name,
                typ: ReqType::Sql,
                body: script,
                format: if fmt.is_empty() { "HTML".into() } else { fmt },
                ..Request::default()
            },
        );
    }
    let formats: Vec<String> = ["HTML", "CSV", "TSV", "XLSX", "ODS", "Fixed width", "PDF"]
        .iter()
        .map(ToString::to_string)
        .collect();
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "<table class=\"props\" cellspacing=\"0\">\n<tr><th>Request name <span class=\"req\">*</span></th><td>{}</td></tr>\n<tr><th>Output</th><td>{}</td></tr>\n<tr><th>Script</th><td><pre class=\"conf\">{}</pre></td></tr>\n</table>\n{}",
            input("name", "", 40),
            pick("fmt", &formats, ""),
            esc(&script),
            buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
        ),
    );
    rq.page(Page {
        title: "Save request".into(),
        crumb: crumb(&["My requests", "Save request"]),
        h1: "Save request".into(),
        desc: Some(
            "Keep the Run SQL script with your requests, to run it again or share it.".into(),
        ),
        body,
        help: "db-requests".into(),
        nav: Some((GROUP, Some("/requests"))),
        ..Page::default()
    })
}

fn download(r: &Request) -> Response {
    let file: String = r
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    match r.typ {
        ReqType::Sql => Response::download(
            &format!("{file}.sql"),
            "application/sql; charset=utf-8",
            r.body.clone().into_bytes(),
        ),
        _ => Response::download(
            &format!("{file}.qdx"),
            "application/toml; charset=utf-8",
            r.body.clone().into_bytes(),
        ),
    }
}

fn copy(rq: &Rq<'_>, r: &Request) -> Response {
    if let Err(resp) = rq.policy(policy::DB_SAVE, CRUMB) {
        return resp;
    }
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/requests");
        }
        let name = f.str("name").trim();
        let name = if name.is_empty() {
            format!("Copy of {}", r.name)
        } else {
            name.to_string()
        };
        return keep(
            rq,
            &Request {
                id: 0,
                name,
                last_run: 0,
                shared: String::new(),
                ..r.clone()
            },
        );
    }
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "<table class=\"props\" cellspacing=\"0\">\n<tr><th>Request</th><td>{}</td></tr>\n<tr><th>New name</th><td>{}</td></tr>\n</table>\n{}",
            esc(&r.name),
            input("name", &format!("Copy of {}", r.name), 40),
            buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
        ),
    );
    rq.page(Page {
        title: "Copy request".into(),
        crumb: crumb(&["My requests", "Copy request"]),
        h1: "Copy request".into(),
        body,
        help: "db-requests".into(),
        nav: Some((GROUP, Some("/requests"))),
        ..Page::default()
    })
}

/// The choices of Share: nobody, the user's groups, `*PUBLIC`.
const NOBODY: &str = "(nobody)";

fn share(rq: &Rq<'_>, r: &Request) -> Response {
    if let Err(resp) = rq.policy(policy::DB_SHARE, CRUMB) {
        return resp;
    }
    let mut choices = vec![NOBODY.to_string()];
    choices.extend(rq.env().system.groups(rq.user()));
    for p in rq.env().system.profiles(rq.user()) {
        if p.group && !choices.contains(&p.name) {
            choices.push(p.name);
        }
    }
    choices.push(policy::PUBLIC.into());
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/requests");
        }
        let to = f.str("to");
        if !choices.iter().any(|c| c == to) {
            return rq.done(
                "/requests",
                rq.message(MsgKind::Error, msgs::BAD_VALUE, &[to, "Share with"]),
            );
        }
        let shared = if to == NOBODY {
            String::new()
        } else {
            to.to_string()
        };
        let mut n = r.clone();
        n.shared.clone_from(&shared);
        return match rq.env().db.save_request(rq.user(), &n) {
            Ok(_) if shared.is_empty() => rq.done(
                "/requests",
                rq.message(MsgKind::Done, msgs::REQUEST_SAVED, &[&r.name]),
            ),
            Ok(_) => rq.done(
                "/requests",
                rq.message(MsgKind::Done, msgs::REQUEST_SHARED, &[&r.name, &shared]),
            ),
            Err(m) => rq.done("/requests", m),
        };
    }
    let chosen = if r.shared.is_empty() {
        NOBODY
    } else {
        r.shared.as_str()
    };
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "<table class=\"props\" cellspacing=\"0\">\n<tr><th>Request</th><td>{}</td></tr>\n<tr><th>Share with</th><td>{}</td></tr>\n</table>\n{}",
            esc(&r.name),
            pick("to", &choices, chosen),
            buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
        ),
    );
    rq.page(Page {
        title: "Share request".into(),
        crumb: crumb(&["My requests", "Share request"]),
        h1: "Share request".into(),
        desc: Some("A shared request is listed for the group's members, or for everyone with *PUBLIC; they can run and copy it, not change it.".into()),
        body,
        help: "db-requests".into(),
        nav: Some((GROUP, Some("/requests"))),
        ..Page::default()
    })
}

fn delete(rq: &Rq<'_>, r: &Request) -> Response {
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/requests");
        }
        return match rq.env().db.delete_request(rq.user(), r.id) {
            Ok(()) => rq.done(
                "/requests",
                rq.message(MsgKind::Done, msgs::REQUEST_DELETED, &[&r.name]),
            ),
            Err(m) => rq.done("/requests", m),
        };
    }
    let lst = List {
        cols: vec![
            Col::plain("Request"),
            Col::plain("Type"),
            Col::plain("Statement / source"),
            Col::plain("Shared with"),
        ],
        rows: vec![vec![
            Cell::text(&r.name),
            Cell::text(r.typ.label()),
            Cell::html(qsweb::partials::mono(&source(r))),
            Cell::text(&r.shared),
        ]],
        empty: String::new(),
        keep: Vec::new(),
    };
    let body = confirm_body(
        &rq.ctx,
        "Deleting a request removes it from your requests and from the lists of those it is shared with. Press OK to delete the request below, or Cancel to return without deleting.",
        &list_table(&lst),
        "",
    );
    rq.page(Page {
        title: "Confirm Delete Request".into(),
        crumb: crumb(&["My requests", "Confirm Delete Request"]),
        h1: "Confirm Delete Request".into(),
        body,
        help: "confirm".into(),
        nav: Some((GROUP, Some("/requests"))),
        ..Page::default()
    })
}
