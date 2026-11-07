// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/db/<schema>/<table>/{columns,keys,indexes,triggers,properties,ddl}`
//! from the catalog, and `/db/<schema>/<table>/data?w=&sort=&page=`: the
//! table's rows through Run SQL's statement and result table.

use std::fmt::Write as _;

use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::partials::{
    Cell, Col, List, Paging, Prop, PropGroup, input_cmd, link, list_table, mono, paging,
    props_table, tabs,
};

use super::{MAX_ROWS, csv, mb10, message_line, messages, result_table, returned, sort_rows};
use crate::PRODUCT;
use crate::env::db::{SqlOptions, TableDesc};
use crate::routes::Rq;
use crate::util::thousands;

/// The tabs: label and route segment.
const TABS: [(&str, &str); 5] = [
    ("Columns", "columns"),
    ("Key constraints", "keys"),
    ("Indexes", "indexes"),
    ("Triggers", "triggers"),
    ("Properties", "properties"),
];

fn find(rq: &Rq<'_>, schema: &str, table: &str) -> Result<TableDesc, Response> {
    let (s, t) = (schema.to_ascii_uppercase(), table.to_ascii_uppercase());
    rq.env()
        .db
        .table(rq.user(), &s, &t)
        .ok_or_else(|| rq.not_found(&format!("table {s}.{t}")))
}

/// Run SQL's link for `statement` in `schema`.
fn run_sql_link(rq: &Rq<'_>, statement: &str, schema: &str) -> String {
    rq.ctx.url(&format!(
        "/sql{}",
        query(&[("q", statement), ("s", schema)])
    ))
}

/// One tab of a table.
pub fn tab(rq: &Rq<'_>, schema: &str, table: &str, tab: &str) -> Response {
    let t = match find(rq, schema, table) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let current = TABS.iter().position(|(_, k)| *k == tab);
    if current.is_none() && tab != "ddl" {
        return rq.not_found(&format!("db/{schema}/{table}/{tab}"));
    }
    let base = format!("/db/{}/{}", t.schema, t.table.name);
    let bar: Vec<(&str, String)> = TABS
        .iter()
        .map(|(l, k)| (*l, rq.ctx.url(&format!("{base}/{k}"))))
        .collect();
    let all = format!("SELECT * FROM {}.{}", t.schema, t.table.name);
    let mut body = tabs(&bar, current.unwrap_or(usize::MAX));
    let _ = write!(
        body,
        "<p class=\"small\">{} rows &nbsp; {} &nbsp; {} &nbsp; {}</p>",
        thousands(t.table.rows),
        link(&rq.ctx.url(&format!("{base}/data")), "View data"),
        link(&run_sql_link(rq, &all, &t.schema), "Run SQL"),
        link(&rq.ctx.url(&format!("{base}/ddl")), "Generate SQL (DDL)")
    );
    let content = match tab {
        "columns" => columns(&t, rq.query().str("sort")),
        "keys" => keys(&t),
        "indexes" => indexes(rq, &t),
        "triggers" => triggers(&t),
        "properties" => properties(rq, &t),
        _ => match rq.env().db.ddl(rq.user(), &t.schema, &t.table.name) {
            Ok(sql) => format!("<pre class=\"out\">{}</pre>", esc(&sql)),
            Err(m) => message_line(&[m]),
        },
    };
    body.push_str(&content);
    let title = format!("Table: {}.{}", t.schema, t.table.name);
    super::page(
        rq,
        &["Schemas", &t.schema, &t.table.name],
        &title,
        None,
        body,
        "db-table",
        None,
    )
}

fn columns(t: &TableDesc, sort: &str) -> String {
    let mut cols: Vec<(usize, &crate::env::db::Column)> = t.columns.iter().enumerate().collect();
    if sort == "c" {
        cols.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    }
    list_table(&List {
        cols: vec![
            Col::plain("#"),
            Col::sorted("Column", "c"),
            Col::plain("System name"),
            Col::plain("Type"),
            Col::plain("Length"),
            Col::plain("Scale"),
            Col::plain("Nullable"),
            Col::plain("Default"),
            Col::plain("Text"),
        ],
        rows: cols
            .iter()
            .map(|(i, c)| {
                vec![
                    Cell::num(&(i + 1).to_string()),
                    Cell::mono(&c.name),
                    Cell::mono(&c.system_name),
                    Cell::mono(&c.typ),
                    Cell::num(&c.length.to_string()),
                    Cell::num(&c.scale.map(|s| s.to_string()).unwrap_or_default()),
                    Cell::text(if c.nullable { "Yes" } else { "No" }),
                    Cell::mono(&c.default),
                    Cell::text(&c.text),
                ]
            })
            .collect(),
        empty: "The table has no columns.".into(),
        keep: Vec::new(),
    })
}

fn keys(t: &TableDesc) -> String {
    list_table(&List {
        cols: vec![
            Col::plain("Constraint"),
            Col::plain("Type"),
            Col::plain("Columns"),
            Col::plain("References"),
        ],
        rows: t
            .constraints
            .iter()
            .map(|c| {
                vec![
                    Cell::mono(&c.name),
                    Cell::text(&c.kind),
                    Cell::mono(&c.columns),
                    Cell::mono(&c.references),
                ]
            })
            .collect(),
        empty: "The table has no constraints.".into(),
        keep: Vec::new(),
    })
}

fn indexes(rq: &Rq<'_>, t: &TableDesc) -> String {
    list_table(&List {
        cols: vec![
            Col::plain("Index"),
            Col::plain("Schema"),
            Col::plain("Key columns"),
            Col::plain("Unique"),
            Col::plain("Text"),
        ],
        rows: t
            .indexes
            .iter()
            .map(|x| {
                vec![
                    Cell::mono(&x.name),
                    Cell::mono_html(link(
                        &rq.ctx.url(&format!("/db/{}?show=indexes", x.schema)),
                        &x.schema,
                    )),
                    Cell::mono(&x.columns),
                    Cell::text(if x.unique { "Yes" } else { "No" }),
                    Cell::text(&x.text),
                ]
            })
            .collect(),
        empty: "No index is built over the table.".into(),
        keep: Vec::new(),
    })
}

fn triggers(t: &TableDesc) -> String {
    list_table(&List {
        cols: vec![
            Col::plain("Trigger"),
            Col::plain("Event"),
            Col::plain("Time"),
            Col::plain("Program"),
            Col::plain("Enabled"),
        ],
        rows: t
            .triggers
            .iter()
            .map(|x| {
                vec![
                    Cell::mono(&x.name),
                    Cell::text(&x.event),
                    Cell::text(&x.time),
                    Cell::mono(&x.program),
                    Cell::text(if x.enabled { "Yes" } else { "No" }),
                ]
            })
            .collect(),
        empty: "The table has no triggers.".into(),
        keep: Vec::new(),
    })
}

fn properties(rq: &Rq<'_>, t: &TableDesc) -> String {
    let when = |s: u64| esc(&rq.when(s));
    let rows = vec![
        Prop::new("Table", mono(&format!("{}.{}", t.schema, t.table.name))),
        Prop::new(
            "System name",
            mono(&format!("{}/{}", t.schema, t.system_name)),
        ),
        Prop::new("Text", esc(&t.table.text)),
        Prop::new("Owner", mono(&t.owner)),
        Prop::new("Record format", mono(&t.format)),
        Prop::new("Created", when(t.created)),
        Prop::new("Last changed", when(t.table.changed)),
        Prop::new("Rows", thousands(t.table.rows)),
        Prop::new("Deleted rows", thousands(t.deleted)),
        Prop::new("Size", format!("{} MB", mb10(t.table.size))),
        Prop::new(
            "Journal",
            if t.journal.is_empty() {
                "None".into()
            } else {
                mono(&t.journal)
            },
        ),
    ];
    props_table(&PRODUCT, &[PropGroup { title: None, rows }], true)
}

/// `/db/<schema>/<table>/data`: the rows, a Where filter, 25 a page.
#[allow(clippy::many_single_char_names)]
pub fn data(rq: &Rq<'_>, schema: &str, table: &str) -> Response {
    let t = match find(rq, schema, table) {
        Ok(t) => t,
        Err(r) => return r,
    };
    let q = rq.query();
    let w = q.str("w").trim().to_string();
    let sort = q.str("sort").to_string();
    let order = t
        .columns
        .iter()
        .find(|c| c.name.to_ascii_lowercase() == sort)
        .map(|c| c.name.clone())
        .or_else(|| t.primary_key().map(ToString::to_string))
        .unwrap_or_default();
    let mut statement = format!("SELECT * FROM {}.{}", t.schema, t.table.name);
    if !w.is_empty() {
        let _ = write!(statement, " WHERE {w}");
    }
    if !order.is_empty() {
        let _ = write!(statement, " ORDER BY {order}");
    }
    let opts = SqlOptions {
        schema: t.schema.clone(),
        ..SqlOptions::default()
    };
    let db = &rq.env().db;
    if q.has("csv") {
        let o = db.run_sql(rq.user(), &statement, &opts, None);
        if !o.failed() {
            let name = format!("{}.csv", t.table.name.to_ascii_lowercase());
            return Response::download(&name, "text/csv; charset=utf-8", csv(&o.columns, &o.rows));
        }
    }
    let mut o = db.run_sql(rq.user(), &statement, &opts, Some(MAX_ROWS));
    let base = format!("/db/{}/{}", t.schema, t.table.name);
    let mut body = format!(
        "<p class=\"small\">Statement: {} &nbsp; {} &nbsp; {}</p>",
        mono(&statement),
        link(&run_sql_link(rq, &statement, &t.schema), "Edit in Run SQL"),
        link(&rq.ctx.url(&format!("{base}/columns")), "Columns")
    );
    let hidden = if sort.is_empty() {
        String::new()
    } else {
        format!(
            "<input type=\"hidden\" name=\"sort\" value=\"{}\">",
            esc(&sort)
        )
    };
    let _ = write!(
        body,
        "<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr><td>Where:</td><td>{}</td><td>{hidden}<input type=\"submit\" value=\"Apply\"> </td></tr></table></form>",
        input_cmd("w", &w, 50)
    );
    let mut keep: Vec<(String, String)> = Vec::new();
    if !w.is_empty() {
        keep.push(("w".into(), w.clone()));
    }
    if o.failed() {
        body.push_str(&message_line(&messages(rq, &o)));
    } else {
        sort_rows(&o.columns, &mut o.rows, &sort);
        let mut kept = keep.clone();
        if !sort.is_empty() {
            kept.push(("sort".into(), sort.clone()));
        }
        let p = Paging::from_query(q.str("page"), q.str("rows"), o.rows.len(), kept.clone());
        body.push_str(&result_table(&o.columns, &o.rows[p.range()], keep));
        let mut csv_q: Vec<(&str, &str)> =
            kept.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        csv_q.push(("csv", "1"));
        body.push_str(&returned(
            o.rows.len(),
            &rq.ctx.url(&format!("{base}/data{}", query(&csv_q))),
        ));
        body.push_str(&paging(&p));
    }
    let title = format!("View data: {}.{}", t.schema, t.table.name);
    super::page(
        rq,
        &["Schemas", &t.schema, &t.table.name, "Data"],
        &title,
        None,
        body,
        "db-data",
        None,
    )
}
