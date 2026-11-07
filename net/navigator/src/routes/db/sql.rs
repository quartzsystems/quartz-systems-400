// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/sql`: Run SQL. A POST runs one statement as the user (or explains
//! it) and keeps it with its result; the GET shows the last one. `?q=`
//! starts from a statement, `?h=N` from a previous one, `?sort=` sorts
//! the result shown, `?csv=1` runs the last query again for all its rows.

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Button, Prop, PropGroup, buttons, form, props_table};

use super::{Last, MAX_ROWS, csv, message_line, messages, result_table, returned, sort_rows};
use crate::PRODUCT;
use crate::env::db::SqlOptions;
use crate::routes::Rq;
use crate::routes::common::{pick, strings};

/// The namings offered.
const NAMINGS: [&str; 2] = ["*SQL", "*SYS"];
/// The isolation levels offered.
const ISOLATIONS: [&str; 4] = ["*NONE", "*CHG", "*CS", "*ALL"];

/// `/sql`.
pub fn page(rq: &Rq<'_>) -> Response {
    if rq.post() {
        return post(rq);
    }
    let q = rq.query();
    let state = rq.nav.sql.of(rq.user());
    if q.has("csv") {
        return download(rq, state.last.as_ref());
    }
    let schemas = rq.env().db.sql_schemas(rq.user());
    let mut last = state.last.clone();
    let mut statement = last
        .as_ref()
        .map(|l| l.statement.clone())
        .unwrap_or_default();
    let mut opts = last.as_ref().map_or_else(
        || SqlOptions {
            schema: schemas.first().cloned().unwrap_or_default(),
            ..SqlOptions::default()
        },
        |l| l.opts.clone(),
    );
    if let Some(s) = q.get("q") {
        statement = s.to_string();
        last = None;
        if let Some(sc) = q.get("s") {
            opts.schema = sc.to_ascii_uppercase();
        }
    } else if let Some(s) = q
        .get("h")
        .and_then(|h| h.parse::<usize>().ok())
        .and_then(|h| state.history.get(h))
    {
        statement.clone_from(s);
        last = None;
    }
    let fields = PropGroup {
        title: None,
        rows: vec![
            Prop::new(
                "Schema",
                format!(
                    "{} &nbsp; Naming: {} &nbsp; Isolation: {}",
                    pick("schema", &schemas, &opts.schema),
                    pick("naming", &strings(&NAMINGS), &opts.naming),
                    pick("iso", &strings(&ISOLATIONS), &opts.isolation)
                ),
            ),
            Prop::new(
                "Statement",
                format!(
                    "<textarea name=\"sql\" rows=\"10\" cols=\"100\" class=\"conf\">{}</textarea>",
                    esc(&statement)
                ),
            ),
        ],
    };
    let mut body = form(
        &rq.ctx,
        "",
        &format!(
            "{}{}",
            props_table(&PRODUCT, &[fields], false),
            buttons(&[
                Button::new("run", "Run"),
                Button::new("explain", "Explain"),
                Button::new("clear", "Clear")
            ])
        ),
    );
    if let Some(l) = &last {
        body.push_str(&result(rq, l, q.str("sort")));
    }
    let previous: Vec<(usize, &String)> = state
        .history
        .iter()
        .enumerate()
        .filter(|(_, s)| last.as_ref().is_none_or(|l| l.statement != **s))
        .collect();
    if !previous.is_empty() {
        body.push_str("<h2>Previous statements</h2><div class=\"small mono\">");
        for (n, (i, s)) in previous.iter().enumerate() {
            if n > 0 {
                body.push_str("<br>");
            }
            let _ = write!(
                body,
                "<a href=\"{}\">{}</a>",
                esc(&rq.ctx.url(&format!("/sql?h={i}"))),
                esc(s)
            );
        }
        body.push_str("</div>");
    }
    super::page(
        rq,
        &["Run SQL"],
        "Run SQL",
        Some("Run an SQL statement against the local database."),
        body,
        "sql",
        Some("/sql"),
    )
}

/// The last statement's result, plan or messages.
fn result(rq: &Rq<'_>, l: &Last, sort: &str) -> String {
    let mut s = String::new();
    if let Some(plan) = &l.plan {
        s.push_str("<h2>Access plan</h2>");
        match plan {
            Ok(lines) => {
                let _ = write!(s, "<pre class=\"out\">{}\n</pre>", esc(&lines.join("\n")));
            }
            Err(m) => s.push_str(&message_line(std::slice::from_ref(m))),
        }
    }
    if let Some(o) = &l.outcome {
        s.push_str("<h2>Results</h2>");
        if o.query && !o.failed() {
            let mut rows = o.rows.clone();
            sort_rows(&o.columns, &mut rows, sort);
            s.push_str(&result_table(&o.columns, &rows, Vec::new()));
            s.push_str(&returned(rows.len(), &rq.ctx.url("/sql?csv=1")));
        }
        s.push_str(&message_line(&messages(rq, o)));
    }
    s
}

/// The POST: Run, Explain or Clear.
fn post(rq: &Rq<'_>) -> Response {
    let f = rq.form();
    let user = rq.user().to_string();
    if f.has("clear") {
        rq.nav.sql.update(&user, |u| u.last = None);
        return rq.redirect("/sql");
    }
    let schemas = rq.env().db.sql_schemas(&user);
    let choose = |v: &str, all: &[String]| {
        all.iter()
            .find(|o| *o == v)
            .or(all.first())
            .cloned()
            .unwrap_or_default()
    };
    let opts = SqlOptions {
        schema: choose(f.str("schema"), &schemas),
        naming: choose(f.str("naming"), &strings(&NAMINGS)),
        isolation: choose(f.str("iso"), &strings(&ISOLATIONS)),
    };
    let statement = f.str("sql").replace("\r\n", "\n").trim_end().to_string();
    if statement.trim().is_empty() {
        return rq.redirect("/sql");
    }
    let db = &rq.env().db;
    let last = if f.has("explain") {
        Last {
            plan: Some(db.explain(&user, &statement, &opts)),
            statement,
            opts,
            outcome: None,
        }
    } else {
        rq.nav.sql.remember(&user, &statement);
        Last {
            outcome: Some(db.run_sql(&user, &statement, &opts, Some(MAX_ROWS))),
            statement,
            opts,
            plan: None,
        }
    };
    rq.nav.sql.update(&user, |u| u.last = Some(last));
    rq.redirect("/sql")
}

/// The last query's full result as CSV, run again.
fn download(rq: &Rq<'_>, last: Option<&Last>) -> Response {
    let Some(l) = last.filter(|l| l.outcome.as_ref().is_some_and(|o| o.query)) else {
        return rq.redirect("/sql");
    };
    let o = rq.env().db.run_sql(rq.user(), &l.statement, &l.opts, None);
    if o.failed() {
        return rq.redirect("/sql");
    }
    Response::download(
        "result.csv",
        "text/csv; charset=utf-8",
        csv(&o.columns, &o.rows),
    )
}
