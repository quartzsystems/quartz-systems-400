// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database: Run SQL, the schemas and a table's pages against the golden
//! pages, and what Run SQL keeps. Golden corrections are listed in
//! docs/NAVIGATOR.md.

mod common;

use common::{Browser, assert_golden};
use qsweb::partials::{Cell, Col, List, list_table};

/// The prototype's statement.
const STATEMENT: &str =
    "SELECT asset_tag, location, manufacturer, model, serial, asset_type, status, acquired
  FROM hlinv.assets
 WHERE status IN ('ACTIVE', 'SPARE')
 ORDER BY location, asset_tag";

/// The statements the prototype lists as previous, oldest first.
const EARLIER: [&str; 4] = [
    "SELECT * FROM qsys2.system_status_info",
    "UPDATE hlinv.assets SET status = 'RETIRED' WHERE asset_tag = 'QS000301'",
    "SELECT * FROM qsys2.active_job_info WHERE job_status = 'MSGW'",
    "SELECT COUNT(*) FROM hlinv.assets GROUP BY location",
];

fn ok(b: &mut Browser, route: &str) -> String {
    let r = b.get(route);
    assert_eq!(r.status, 200, "{route}: {}", r.text());
    r.text()
}

fn run(b: &mut Browser, sql: &str) {
    let r = b.post(
        "/sql",
        &[
            ("schema", "HLINV"),
            ("naming", "*SQL"),
            ("iso", "*NONE"),
            ("sql", sql),
            ("run", "Run"),
        ],
    );
    assert_eq!(r.header("Location"), Some("/qs400/navigator/sql"));
}

/// The prototype's SQL0100 text is another vendor's; the message file's own.
const SQL0100: (&str, &str) = (
    "Row not found for FETCH, UPDATE or DELETE; or the result of a query is an empty table.",
    "No row was found for FETCH.",
);

#[test]
fn run_sql() {
    let mut b = Browser::signed_on();
    for s in EARLIER {
        run(&mut b, s);
    }
    run(&mut b, STATEMENT);
    assert_golden(
        &ok(&mut b, "/sql"),
        "nav/run-sql.html",
        &[
            SQL0100,
            (
                "<a href=\"#\">Download as CSV</a>",
                "<a href=\"run-sql.html\">Download as CSV</a>",
            ),
        ],
    );
    // A previous statement comes back into the box.
    let t = ok(&mut b, "/sql?h=3");
    assert!(
        t.contains("class=\"conf\">UPDATE hlinv.assets SET status = &#39;RETIRED&#39;"),
        "{t}"
    );
    // The full result as CSV.
    let r = b.get("/sql?csv=1");
    assert_eq!(r.header("Content-Type"), Some("text/csv; charset=utf-8"));
    let csv = r.text();
    assert!(
        csv.starts_with("ASSET_TAG,LOCATION,") && csv.lines().count() == 13,
        "{csv}"
    );
    // Sorting the result shown.
    let t = ok(&mut b, "/sql?sort=manufacturer");
    assert!(t.find("ARISTA").unwrap() < t.find("QUANTA").unwrap());
    // Clear forgets the result but keeps the statements.
    b.post("/sql", &[("sql", ""), ("clear", "Clear")]);
    let t = ok(&mut b, "/sql");
    assert!(!t.contains("<h2>Results</h2>") && t.contains("ORDER BY location, asset_tag</a>"));
}

#[test]
fn run_sql_as_the_user() {
    let mut b = Browser::as_user("WEBUSER");
    run(
        &mut b,
        "UPDATE hlinv.assets SET status = 'SPARE' WHERE asset_tag = 'QS000101'",
    );
    let t = ok(&mut b, "/sql");
    assert!(t.contains(">SQL0551</a>"), "{t}");
    assert!(
        b.fx.db
            .ran
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.starts_with("UPDATE"))
    );
    // History is per user.
    let mut q = Browser::signed_on();
    assert!(!ok(&mut q, "/sql").contains("Previous statements"));
    // Errors show their message; Explain shows a plan.
    run(&mut b, "SELECT nothing FROM hlinv.assets");
    assert!(ok(&mut b, "/sql").contains(">SQL0206</a> - Column NOTHING not found."));
    b.post(
        "/sql",
        &[
            ("schema", "HLINV"),
            ("sql", "SELECT * FROM assets"),
            ("explain", "Explain"),
        ],
    );
    assert!(ok(&mut b, "/sql").contains("Table scan        HLINV.ASSETS"));
}

#[test]
fn history_keeps_fifty() {
    let mut b = Browser::signed_on();
    for n in 0..55 {
        run(
            &mut b,
            &format!("SELECT * FROM hlinv.assets WHERE rack_u = {n}"),
        );
    }
    let h = b.nav.sql.of("QSECOFR").history;
    assert_eq!(h.len(), 50);
    assert!(h[0].ends_with("= 54"));
}

#[test]
fn schemas() {
    let mut b = Browser::signed_on();
    assert_golden(&ok(&mut b, "/db"), "nav/schemas.html", &[]);
    assert_golden(&ok(&mut b, "/db/HLINV"), "nav/schemas.html", &[]);
    let t = ok(&mut b, "/db/HLINV?show=indexes");
    assert!(t.contains("<h3>Indexes</h3>") && t.contains("ASSETS_SERIAL"));
    assert!(
        ok(&mut b, "/db/HLINV?sort=r").find("NOTES").unwrap()
            < ok(&mut b, "/db/HLINV?sort=r")
                .find("ASSETS</a></td>")
                .unwrap()
    );
    assert_eq!(b.get("/db/NOSUCH").status, 404);
}

#[test]
fn table_columns() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, "/db/HLINV/ASSETS/columns"),
        "nav/table-columns.html",
        // The DDL link went nowhere.
        &[(
            "<a href=\"#\">Generate SQL (DDL)</a>",
            "<a href=\"table-columns.html\">Generate SQL (DDL)</a>",
        )],
    );
    for (tab, has) in [
        ("keys", "ASSETS_LOC_FK"),
        ("indexes", "LOCATION, ASSET_TAG"),
        ("triggers", "HLINV/ASSETSTRG"),
        ("properties", "Deleted rows"),
        ("ddl", "CREATE TABLE HLINV.ASSETS ("),
    ] {
        assert!(
            ok(&mut b, &format!("/db/HLINV/ASSETS/{tab}")).contains(has),
            "{tab}"
        );
    }
    assert_eq!(b.get("/db/HLINV/NOSUCH/columns").status, 404);
}

/// Rows 1 to 25 of ASSETS as the data page shows them.
fn first_page() -> String {
    let fx = navigator::fixture::fixture();
    let t = fx.db.tables.lock().unwrap();
    let a = &t[0];
    list_table(&List {
        cols: a
            .columns
            .iter()
            .map(|c| Col::sorted(&c.name, &c.name.to_ascii_lowercase()))
            .collect(),
        rows: a.rows[..25]
            .iter()
            .map(|r| {
                r.iter()
                    .map(|v| Cell::mono(v.as_deref().unwrap_or("NULL")))
                    .collect()
            })
            .collect(),
        empty: String::new(),
        keep: Vec::new(),
    })
}

#[test]
fn table_data() {
    let mut b = Browser::signed_on();
    let golden = common::golden("nav/table-data.html");
    let start = golden.find("<table class=\"list\"").unwrap();
    let end = start + golden[start..].find("</table>").unwrap() + "</table>".len();
    let page = first_page();
    assert_golden(
        &ok(&mut b, "/db/HLINV/ASSETS/data"),
        "nav/table-data.html",
        &[
            // The prototype showed Run SQL's eight columns and twelve rows
            // for SELECT * over 412: the table's columns, 25 a page.
            (&golden[start..end], page.as_str()),
            ("12 rows returned", "412 rows returned"),
            (
                "<a href=\"#\">Download as CSV</a>",
                "<a href=\"table-data.html\">Download as CSV</a>",
            ),
        ],
    );
    let t = ok(&mut b, "/db/HLINV/ASSETS/data?w=status%20%3D%20%27SPARE%27");
    assert!(
        t.contains("QS000403")
            && t.contains("1 entry")
            && t.contains("WHERE status = &#39;SPARE&#39; ORDER BY ASSET_TAG"),
        "{t}"
    );
    let t = ok(&mut b, "/db/HLINV/ASSETS/data?w=nope%20%3D%201");
    assert!(t.contains(">SQL0206</a>"));
    let r = b.get("/db/HLINV/ASSETS/data?csv=1");
    assert_eq!(r.text().lines().count(), 413);
}
