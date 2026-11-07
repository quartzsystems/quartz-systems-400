// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Tables (`/db/tables?sp=&s=&t=&n=&sort=`): the catalog as the
//! desktop's Schemas shows it, the schemas matching a pattern, a
//! schema's tables, views, aliases and files, 100 a page. The filter
//! stays with the session, so paging keeps it. `?copy=NAME` shows the
//! qualified name ready to copy.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, Paging, list_table, paging};

use super::{GROUP, crumb, navigator, pick, short_stamp, thousands};
use crate::env::db::Kind;
use crate::data::sqltext::like;
use crate::policy;
use crate::routes::Rq;

/// The Objects choice for every kind.
const ALL: &str = "All";

/// A pattern as SQL `LIKE` takes it: `*ALL` or nothing is everything,
/// a trailing `*` is generic.
pub fn like_pattern(p: &str) -> String {
    let p = p.trim().to_ascii_uppercase();
    if p.is_empty() || p == "*ALL" {
        return "%".into();
    }
    match p.strip_suffix('*') {
        Some(g) => format!("{g}%"),
        None => p,
    }
}

/// A filter value: the query's, else the session's, else `default`;
/// what the query gives is kept.
fn sticky(rq: &Rq<'_>, key: &str, default: &str) -> String {
    let q = rq.query();
    let skey = format!("db.tables.{key}");
    if let Some(v) = q.get(key) {
        rq.set(&[(&skey, v)]);
        return v.to_string();
    }
    let v = rq.get(&skey);
    if v.is_empty() { default.to_string() } else { v }
}

/// `GET /db/tables`.
#[allow(clippy::too_many_lines)] // one page
pub fn page(rq: &Rq<'_>) -> Response {
    const CR: &[&str] = &["Tables"];
    if let Err(r) = rq.policy(policy::DB_BROWSE, &[GROUP, "Tables"]) {
        return r;
    }
    let db = &rq.env().db;
    let user = rq.user();
    let sp = sticky(rq, "sp", "*ALL");
    let schemas = db.schemas(user, &like_pattern(&sp));
    let preferred = rq.prefs().get(crate::prefs::SCHEMA);
    let mut s = sticky(rq, "s", "");
    if !schemas.contains(&s) {
        s = if schemas.contains(&preferred) { preferred } else { schemas.first().cloned().unwrap_or_default() };
    }
    let t = sticky(rq, "t", ALL);
    let n = sticky(rq, "n", "*ALL");
    let sort = sticky(rq, "sort", "");
    let mut messages = Vec::new();
    let mut entries = if s.is_empty() {
        Vec::new()
    } else {
        match db.tables(user, &s) {
            Ok(e) => e,
            Err(m) => {
                messages.push(m);
                Vec::new()
            }
        }
    };
    let kind = Kind::ALL.into_iter().find(|k| k.plural() == t);
    let np = like_pattern(&n);
    entries.retain(|e| kind.is_none_or(|k| e.kind == k) && like(&np, &e.name));
    match sort.as_str() {
        "t" => entries.sort_by(|a, b| a.name.cmp(&b.name)),
        "r" => entries.sort_by_key(|a| std::cmp::Reverse(a.rows)),
        _ => {}
    }
    let q = rq.query();
    let p = Paging::from_query(q.str("page"), q.get("rows").unwrap_or("100"), entries.len(), Vec::new());
    let browse = rq.allowed(policy::DB_BROWSE);
    let write = rq.allowed(policy::DB_WRITE);
    let from = rq.allowed(policy::DB_COPY_FROM);
    let to = rq.allowed(policy::DB_COPY_TO);
    let url = |r: &str| esc(&crate::PRODUCT.url(r));
    let rows = entries[p.range()]
        .iter()
        .map(|e| {
            let view = format!("/db/{}/{}", s, e.name);
            let st = query(&[("s", &s), ("t", &e.name)]);
            let mut acts = Vec::new();
            if browse {
                acts.push(format!("<a href=\"{}\">View data</a>", url(&view)));
            }
            acts.push(format!(
                "<a href=\"{}\">Columns</a>",
                esc(&navigator(rq, &format!("/db/{}/{}/columns", s, e.name)))
            ));
            if write {
                acts.push(format!("<a href=\"{}\">Insert record</a>", url(&format!("{view}/insert"))));
            }
            if from {
                acts.push(format!("<a href=\"{}\">Transfer from</a>", url(&format!("/db/copy/from{st}"))));
            }
            if to {
                acts.push(format!("<a href=\"{}\">Transfer to</a>", url(&format!("/db/copy/to{st}"))));
            }
            acts.push(format!("<a href=\"{}\">Copy name</a>", url(&format!("/db/tables{}", query(&[("copy", &e.name)])))));
            vec![
                Cell::act(acts.join(" | ")),
                Cell::mono_html(if browse { format!("<a href=\"{}\">{}</a>", url(&view), esc(&e.name)) } else { esc(&e.name) }),
                Cell::text(e.kind.label()),
                Cell::text(&e.text),
                Cell::num(&thousands(e.rows)),
                Cell::text(&short_stamp(rq, e.changed)),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Name", "t"),
            Col::plain("Type"),
            Col::plain("Label / text"),
            Col::sorted("Rows", "r"),
            Col::plain("Last changed"),
        ],
        rows,
        empty: "No tables, views or files match.".into(),
        keep: Vec::new(),
    };
    let objects: Vec<String> = Kind::ALL.iter().map(|k| k.plural().to_string()).chain([ALL.to_string()]).collect();
    let filter = format!(
        "<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr><td>Schema pattern:</td><td><input type=\"text\" name=\"sp\" value=\"{}\" size=\"12\" class=\"cmd\"></td><td>Schema:</td><td>{}</td><td>Objects:</td><td>{}</td><td>Name:</td><td><input type=\"text\" name=\"n\" value=\"{}\" size=\"12\"></td><td><input type=\"submit\" value=\"Go\"> </td></tr></table></form>",
        esc(&sp),
        pick("s", &schemas, &s),
        pick("t", &objects, &t),
        esc(&n)
    );
    let mut body = String::new();
    if let Some(c) = q.get("copy") {
        let _ = write!(
            body,
            "<p class=\"small\">Qualified name: <input type=\"text\" value=\"{}\" size=\"40\" class=\"cmd\" readonly></p>",
            esc(&format!("{s}.{c}"))
        );
    }
    body.push_str(&filter);
    body.push_str(&list_table(&list));
    body.push_str(&paging(&p));
    rq.page(Page {
        title: "Tables".into(),
        crumb: crumb(CR),
        h1: "Tables".into(),
        desc: Some("The catalog, as QS/400 Access for Desktop's Schemas shows it.".into()),
        messages,
        body,
        help: "db-tables".into(),
        nav: Some((GROUP, Some("/db/tables"))),
        ..Page::default()
    })
}
