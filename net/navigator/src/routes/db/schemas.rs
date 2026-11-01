// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/db[/<schema>]?show=&sort=`: the schemas as a tree beside one
//! schema's attributes and its objects of one kind (tables first).

use std::cmp::Reverse;
use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, Prop, PropGroup, actions, link, list_table, mono, props_table};

use super::mb10;
use crate::PRODUCT;
use crate::env::db::{DbObject, ObjKind, Schema};
use crate::routes::Rq;
use crate::util::thousands;

/// The schemas page; `name` `None` shows the first schema.
pub fn page(rq: &Rq<'_>, name: Option<&str>) -> Response {
    let db = &rq.env().db;
    let all = db.schemas(rq.user());
    let wanted = name.map(str::to_ascii_uppercase);
    let Some(schema) = (match &wanted {
        Some(n) => all.iter().find(|s| s.name == *n),
        None => all.first(),
    }) else {
        return rq.not_found(&format!("schema {}", name.unwrap_or("")));
    };
    let q = rq.query();
    let show = ObjKind::of(q.str("show")).unwrap_or(ObjKind::Tables);
    let mut tree = String::from("<div><span class=\"glyph\">[-]</span> <b>Schemas</b></div>");
    for s in &all {
        if s.name == schema.name {
            let _ = write!(tree, "<div class=\"navitem\"><span class=\"glyph\">[-]</span> <b>{}</b></div>", esc(&s.name));
        } else {
            let _ = write!(
                tree,
                "<div class=\"navitem\"><span class=\"glyph\">[+]</span> {}</div>",
                link(&rq.ctx.url(&format!("/db/{}", s.name)), &s.name)
            );
        }
    }
    let mut shown = Vec::new();
    for kind in ObjKind::ALL {
        let objs = db.objects(rq.user(), &schema.name, kind);
        if kind == show {
            let _ = write!(
                tree,
                "<div class=\"navitem2\"><span class=\"glyph\">[-]</span> <b>{}</b> ({})</div>",
                kind.label(),
                objs.len()
            );
            for o in &objs {
                let name = if kind == ObjKind::Tables {
                    link(&rq.ctx.url(&format!("/db/{}/{}/columns", schema.name, o.name)), &o.name)
                } else {
                    esc(&o.name)
                };
                let _ = write!(tree, "<div class=\"navitem2\" style=\"margin-left:42px\">{name}</div>");
            }
            shown = objs;
        } else {
            let href = if kind == ObjKind::Tables {
                format!("/db/{}", schema.name)
            } else {
                format!("/db/{}?show={}", schema.name, kind.key())
            };
            let _ = write!(
                tree,
                "<div class=\"navitem2\"><span class=\"glyph\">[+]</span> {} ({})</div>",
                link(&rq.ctx.url(&href), kind.label()),
                objs.len()
            );
        }
    }
    let right = format!(
        "<h2>Schema {}</h2>{}<h3>{}</h3>{}",
        esc(&schema.name),
        props_table(&PRODUCT, &[attributes(rq, schema)], true),
        show.label(),
        objects(rq, &schema.name, show, shown, q.str("sort"))
    );
    let body = format!(
        "<table width=\"100%\" cellspacing=\"0\"><tr><td width=\"200\" valign=\"top\" style=\"background:#f0f0f0; border-right:1px solid #c0c0c0; padding:4px\" class=\"small\">{tree}</td><td valign=\"top\" style=\"padding-left:8px\">{right}</td></tr></table>"
    );
    super::page(
        rq,
        &["Schemas"],
        "Schemas",
        Some("Schemas (libraries) and their database objects."),
        body,
        "db",
        Some("/db"),
    )
}

fn attributes(rq: &Rq<'_>, s: &Schema) -> PropGroup {
    let journal = if s.journal.is_empty() { "None".to_string() } else { mono(&s.journal) };
    PropGroup {
        title: None,
        rows: vec![
            Prop::new("Schema", mono(&s.name)),
            Prop::new("Library text", esc(&s.text)),
            Prop::new("Owner", mono(&s.owner)),
            Prop::new("Journal", journal),
            Prop::new("Size", format!("{} MB", mb10(s.size))),
            Prop::new("Created", esc(&rq.stamp(s.created).0)),
        ],
    }
}

/// `MM/DD/YY HH:MM`.
fn changed(rq: &Rq<'_>, secs: u64) -> String {
    let (d, t) = rq.stamp(secs);
    format!("{d} {}", &t[..t.len().min(5)])
}

fn objects(rq: &Rq<'_>, schema: &str, kind: ObjKind, mut objs: Vec<DbObject>, sort: &str) -> String {
    let keep = if kind == ObjKind::Tables {
        Vec::new()
    } else {
        vec![("show".to_string(), kind.key().to_string())]
    };
    match sort {
        "t" => objs.sort_by(|a, b| a.name.cmp(&b.name)),
        "r" => objs.sort_by_key(|o| Reverse(o.rows)),
        _ => {}
    }
    let list = if kind == ObjKind::Tables {
        List {
            cols: vec![
                Col::plain("Actions"),
                Col::sorted("Table", "t"),
                Col::plain("Text"),
                Col::sorted("Rows", "r"),
                Col::plain("Size (MB)"),
                Col::plain("Last changed"),
                Col::plain("Journaled"),
            ],
            rows: objs
                .iter()
                .map(|o| {
                    let base = format!("/db/{schema}/{}", o.name);
                    vec![
                        Cell::act(actions(&[
                            (rq.ctx.url(&format!("{base}/columns")), "Columns"),
                            (rq.ctx.url(&format!("{base}/data")), "View data"),
                            (rq.ctx.url(&format!("{base}/properties")), "Properties"),
                        ])),
                        Cell::mono_html(link(&rq.ctx.url(&format!("{base}/columns")), &o.name)),
                        Cell::text(&o.text),
                        Cell::num(&thousands(o.rows)),
                        Cell::num(&mb10(o.size)),
                        Cell::text(&changed(rq, o.changed)),
                        Cell::text(if o.journaled { "Yes" } else { "No" }),
                    ]
                })
                .collect(),
            empty: "The schema has no tables.".into(),
            keep,
        }
    } else {
        List {
            cols: vec![
                Col::sorted("Name", "t"),
                Col::plain("Text"),
                Col::plain("Over"),
                Col::plain("Last changed"),
            ],
            rows: objs
                .iter()
                .map(|o| {
                    let over = if o.over.is_empty() {
                        Cell::text("")
                    } else {
                        Cell::mono_html(link(&rq.ctx.url(&format!("/db/{schema}/{}/columns", o.over)), &o.over))
                    };
                    vec![Cell::mono(&o.name), Cell::text(&o.text), over, Cell::text(&changed(rq, o.changed))]
                })
                .collect(),
            empty: format!("The schema has no {}.", kind.label().to_ascii_lowercase()),
            keep,
        }
    };
    list_table(&list)
}
