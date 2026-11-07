// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A table's rows (`/db/<schema>/<table>?q=&col=&sort=&page=&fmt=`):
//! 25 a page, Find in a column, sorted by any column and then the key,
//! at most the user's maximum rows; Edit and Delete (through the
//! confirmation) per row when the table has a key. Another output than
//! HTML downloads every row the page selects.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::{Kind as MsgKind, Message};
use qsweb::pages::confirm_body;
use qsweb::partials::{Cell, Col, List, Paging, list_table, paging};

use super::{GROUP, cell, crumb, failure, name_of, pick, with_conn};
use crate::data::files::{self, Format};
use crate::data::sqltext;
use crate::env::db::{Column, ResultColumn, Val};
use crate::routes::Rq;
use crate::{PRODUCT, policy, prefs};

/// The Output select's choices, and the preference each stands for.
const OUTPUTS: &[(&str, &str)] = &[
    ("HTML", "HTML"),
    ("CSV", "CSV"),
    ("PDF", "PDF"),
    ("Excel-compatible (.xls)", "Excel-compatible"),
];

/// A table's columns, or the page saying why not.
pub fn columns(rq: &Rq<'_>, s: &str, t: &str) -> Result<Vec<Column>, Response> {
    rq.env().db.columns(rq.user(), s, t).map_err(|m| {
        let page = Page {
            title: t.to_string(),
            crumb: crumb(&["Tables", t]),
            h1: format!("Table: {s}.{t}"),
            messages: vec![m],
            body: format!(
                "<p><a href=\"{}\">Tables</a></p>",
                esc(&PRODUCT.url("/db/tables"))
            ),
            help: "db-view".into(),
            nav: Some((GROUP, Some("/db/tables"))),
            ..Page::default()
        };
        rq.page_status(404, page)
    })
}

/// The key columns' names.
pub fn keys(cols: &[Column]) -> Vec<String> {
    cols.iter()
        .filter(|c| c.key)
        .map(|c| c.name.clone())
        .collect()
}

/// `?key=` values for a row.
fn key_query(cols: &[Column], row: &[Val]) -> String {
    let pairs: Vec<(&str, String)> = cols
        .iter()
        .zip(row)
        .filter(|(c, _)| c.key)
        .map(|(_, v)| ("key", v.text().trim_end().to_string()))
        .collect();
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    query(&refs)
}

/// The `?key=` values, as parameters, one for each key column.
pub fn key_values(rq: &Rq<'_>, keys: &[String]) -> Option<Vec<Val>> {
    let q = rq.query();
    let v: Vec<Val> = q
        .all("key")
        .into_iter()
        .map(|k| Val::Text(k.to_string()))
        .collect();
    (v.len() == keys.len() && !keys.is_empty()).then_some(v)
}

fn result_columns(cols: &[Column]) -> Vec<ResultColumn> {
    cols.iter()
        .map(|c| ResultColumn {
            name: c.name.clone(),
            typ: c.typ.clone(),
        })
        .collect()
}

/// `GET /db/<schema>/<table>`.
#[allow(clippy::too_many_lines)] // one page
pub fn page(rq: &Rq<'_>, s: &str, t: &str) -> Response {
    let (s, t) = (name_of(s), name_of(t));
    if let Err(r) = rq.policy(policy::DB_BROWSE, &[GROUP, "Tables", &t]) {
        return r;
    }
    let cols = match columns(rq, &s, &t) {
        Ok(c) => c,
        Err(r) => return r,
    };
    let names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
    let q = rq.query();
    let find = q.str("q").trim().to_string();
    let col = q
        .get("col")
        .map(name_of)
        .filter(|c| names.contains(c))
        .unwrap_or_else(|| names[0].clone());
    let sort = q.get("sort").map(name_of).filter(|c| names.contains(c));
    let keys = keys(&cols);
    let mut order: Vec<String> = sort.iter().cloned().collect();
    order.extend(keys.iter().filter(|k| Some(*k) != sort.as_ref()).cloned());
    let find_col = (!find.is_empty()).then_some(col.as_str());
    let params: Vec<Vec<Val>> = if find.is_empty() {
        Vec::new()
    } else {
        vec![vec![Val::Text(sqltext::find_pattern(&find))]]
    };
    let max = rq.prefs().max_rows();
    // Another format: every row selected, as a file.
    let fmt = q.str("fmt");
    if let Some(f) = Format::of(fmt).filter(|f| *f != Format::Html) {
        let sql = sqltext::all(&s, &t, &names, &order, find_col);
        let r = with_conn(rq, |c| {
            rq.env().db.run(rq.user(), c, &sql, &params, usize::MAX)
        });
        return match r {
            Ok(o) if !o.failed => {
                let opts = files::Write {
                    title: t.clone(),
                    ..files::Write::default()
                };
                let data = files::write(f, &result_columns(&cols), &o.rows, &opts);
                let ext = if f == Format::Xls {
                    "xls"
                } else {
                    f.extension()
                };
                Response::download(
                    &format!("{}.{ext}", t.to_ascii_lowercase()),
                    f.content_type(),
                    data,
                )
            }
            Ok(o) => rq.done(
                &format!("/db/{s}/{t}"),
                failure(&o).unwrap_or_else(|| Message::new(MsgKind::Error, "SQL0901", "")),
            ),
            Err(m) => rq.done(&format!("/db/{s}/{t}"), m),
        };
    }
    let mut messages: Vec<Message> = Vec::new();
    let db = &rq.env().db;
    let counted = with_conn(rq, |c| {
        let n = db.run(rq.user(), c, &sqltext::count(&s, &t, find_col), &params, 1);
        if let Some(m) = failure(&n) {
            return Err(m);
        }
        let total: usize = n
            .rows
            .first()
            .and_then(|r| r.first())
            .and_then(|v| v.text().parse().ok())
            .unwrap_or(0);
        let shown = total.min(max);
        let rows = q
            .get("rows")
            .map_or_else(|| rq.prefs().get(prefs::ROWS), ToString::to_string);
        let p = Paging::from_query(
            q.str("page"),
            &rows,
            shown,
            vec![("q".into(), find.clone())],
        );
        let range = p.range();
        let o = db.run(
            rq.user(),
            c,
            &sqltext::page(
                &s,
                &t,
                &names,
                find_col,
                &order,
                range.start,
                range.len().max(1),
            ),
            &params,
            range.len().max(1),
        );
        if let Some(m) = failure(&o) {
            return Err(m);
        }
        Ok((total, p, o.rows))
    });
    let (total, p, rows) = match counted {
        Ok(Ok(x)) => x,
        Ok(Err(m)) | Err(m) => {
            messages.push(m);
            (0, Paging::from_query("1", "", 0, Vec::new()), Vec::new())
        }
    };
    let write = rq.allowed(policy::DB_WRITE) && !keys.is_empty();
    let base = format!("/db/{s}/{t}");
    let list_rows: Vec<Vec<Cell>> = rows
        .iter()
        .map(|r| {
            let mut out = Vec::new();
            if write {
                let k = key_query(&cols, r);
                out.push(Cell::act(format!(
                    "<a href=\"{}\">Edit</a> | <a href=\"{}\">Delete</a>",
                    esc(&PRODUCT.url(&format!("{base}/edit{k}"))),
                    esc(&PRODUCT.url(&format!("{base}/delete/confirm{k}")))
                )));
            }
            out.extend(r.iter().zip(&cols).map(|(v, c)| cell(v, c.numeric())));
            out
        })
        .collect();
    let mut lcols = Vec::new();
    if write {
        lcols.push(Col::plain("Actions"));
    }
    lcols.extend(
        names
            .iter()
            .map(|n| Col::sorted(n, &n.to_ascii_lowercase())),
    );
    let list = List {
        cols: lcols,
        rows: list_rows,
        empty: if find.is_empty() {
            "The table has no rows.".into()
        } else {
            "No rows match.".into()
        },
        keep: vec![
            ("q".into(), find.clone()),
            (
                "col".into(),
                if find.is_empty() {
                    String::new()
                } else {
                    col.clone()
                },
            ),
        ],
    };
    if total > max {
        messages.push(rq.message(MsgKind::Info, crate::msgs::INCOMPLETE, &[&format!(
            "the page shows the first {max} of {total} rows, the maximum your preferences set; another output downloads every row"
        )]));
    }
    let pref = rq.prefs().get(prefs::OUTPUT);
    let chosen = OUTPUTS.iter().find(|o| o.1 == pref).map_or("HTML", |o| o.0);
    let url = |r: &str| esc(&PRODUCT.url(r));
    let st = query(&[("s", &s), ("t", &t)]);
    let mut links = vec![format!("<a href=\"{}\">Tables</a>", url("/db/tables"))];
    if write || rq.allowed(policy::DB_WRITE) {
        links.push(format!(
            "<a href=\"{}\">Insert record</a>",
            url(&format!("{base}/insert"))
        ));
    }
    if rq.allowed(policy::DB_COPY_TO) {
        links.push(format!(
            "<a href=\"{}\">Copy data to table</a>",
            url(&format!("/db/copy/to{st}"))
        ));
    }
    if rq.allowed(policy::DB_SELECT) || rq.allowed(policy::DB_SQL) {
        links.push(format!(
            "<a href=\"{}\">Run SQL</a>",
            url(&format!("/sql{}", query(&[("table", &format!("{s}.{t}"))])))
        ));
    }
    let outputs: Vec<String> = OUTPUTS.iter().map(|o| o.0.to_string()).collect();
    let mut body = format!(
        "<form method=\"get\"><p class=\"small\">{} &nbsp; Output: {} <input type=\"submit\" value=\"Go\"></p></form>",
        links.join(" &nbsp; "),
        pick("fmt", &outputs, chosen)
    );
    let _ = write!(
        body,
        "<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr><td>Find:</td><td><input type=\"text\" name=\"q\" value=\"{}\" size=\"20\"> in {}</td><td>Sort by:</td><td>{}</td><td><input type=\"submit\" value=\"Go\"> </td></tr></table></form>",
        esc(&find),
        pick("col", &names, &col),
        pick("sort", &names, sort.as_deref().unwrap_or(""))
    );
    body.push_str(&list_table(&list));
    body.push_str(&paging(&p));
    rq.page(Page {
        title: format!("View: {s}.{t}"),
        crumb: crumb(&["Tables", &t]),
        h1: format!("View: {s}.{t}"),
        messages,
        body,
        help: "db-view".into(),
        ..Page::default()
    })
}

/// `GET /db/<schema>/<table>/delete/confirm?key=`: the row to delete;
/// `POST` deletes it.
pub fn delete(rq: &Rq<'_>, s: &str, t: &str) -> Response {
    let (s, t) = (name_of(s), name_of(t));
    let cr = [GROUP, "Tables", t.as_str(), "Delete record"];
    if let Err(r) = rq.policy(policy::DB_WRITE, &cr) {
        return r;
    }
    let cols = match columns(rq, &s, &t) {
        Ok(c) => c,
        Err(r) => return r,
    };
    let keys = keys(&cols);
    let back = format!("/db/{s}/{t}");
    let Some(kv) = key_values(rq, &keys) else {
        return rq.not_found(&format!("record of {s}.{t} with that key"));
    };
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect(&back);
        }
        let r = with_conn(rq, |c| {
            rq.env().db.run(
                rq.user(),
                c,
                &sqltext::delete(&s, &t, &keys),
                std::slice::from_ref(&kv),
                0,
            )
        });
        let m = match r {
            Ok(o) => failure(&o).unwrap_or_else(|| {
                if o.changed == 0 {
                    rq.message(MsgKind::Info, "SQL0100", &["DELETE"])
                } else {
                    rq.message(MsgKind::Done, "SQL7955", &[&t, &s, &o.changed.to_string()])
                }
            }),
            Err(m) => m,
        };
        return rq.done(&back, m);
    }
    let found = with_conn(rq, |c| {
        let names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
        rq.env().db.run(
            rq.user(),
            c,
            &sqltext::one(&s, &t, &names, &keys),
            std::slice::from_ref(&kv),
            1,
        )
    });
    let row = match found {
        Ok(o) if !o.failed && !o.rows.is_empty() => o.rows[0].clone(),
        _ => return rq.not_found(&format!("record of {s}.{t} with that key")),
    };
    let list = List {
        cols: cols.iter().map(|c| Col::plain(&c.name)).collect(),
        rows: vec![
            row.iter()
                .zip(&cols)
                .map(|(v, c)| cell(v, c.numeric()))
                .collect(),
        ],
        empty: String::new(),
        keep: Vec::new(),
    };
    let body = confirm_body(
        &rq.ctx,
        &format!(
            "Deleting a record removes it from table {t} in {s}, as the signed-on user. Press OK to delete the record below, or Cancel to return without deleting."
        ),
        &list_table(&list),
        "",
    );
    rq.page(Page {
        title: "Confirm Delete Record".into(),
        crumb: crumb(&["Tables", &t, "Confirm Delete Record"]),
        h1: "Confirm Delete Record".into(),
        body,
        help: "confirm".into(),
        nav: Some((GROUP, Some("/db/tables"))),
        ..Page::default()
    })
}
