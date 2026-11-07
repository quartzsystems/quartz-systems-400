// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database pages (handoff steps 7-12): the golden pages over the
//! fixture, with the corrections docs/ACCESSWEB.md lists, and what they
//! do: policies, the row cap, Run SQL's connection, inserts, edits and
//! deletes, requests, and copies both ways.

#![allow(clippy::many_single_char_names)]

mod common;

use accessweb::env::db::{Db, Val};
use accessweb::env::policy::{PolicyStore, Setting};
use accessweb::policy;
use common::{Browser, HEADER_USER, assert_golden};
use qsweb::http::Request;

/// The prototype's Run SQL script.
const SCRIPT: &str = "-- statements separated by ;\nSELECT asset_tag, location, manufacturer, model, serial, asset_type, status, acquired\n  FROM hlinv.assets\n WHERE status IN ('ACTIVE', 'SPARE')\n ORDER BY location, asset_tag;\n\nSELECT location, COUNT(*) AS n FROM hlinv.assets GROUP BY location;\n\nUPDATE hlinv.assets SET status = 'RETIRED' WHERE asset_tag = 'QS000301';";

/// The file the prototype's Copy data to table read.
const ASSETS_BR1: &str = "tag,site,vendor,model,sn,kind,state,bought,price,comment\r\nQS000404,QS-BR1-R1,ARISTA,7050SX-64,JPE14210034,SWITCH,SPARE,2023-05-02,1200.00,Branch spare\r\nQS000405,QS-BR1-R1,ARISTA,7050SX-64,JPE14210035,SWITCH,SPARE,2023-05-02,1200.00,\r\n";

/// A multipart POST: text fields and files (name, file name, bytes).
fn multipart(
    b: &mut Browser,
    route: &str,
    fields: &[(&str, &str)],
    files: &[(&str, &str, &[u8])],
) -> qsweb::http::Response {
    let boundary = "----qs400test";
    let mut body = Vec::new();
    let tok = b.csrf.clone();
    for (k, v) in std::iter::once(("csrf", tok.as_str())).chain(fields.iter().copied()) {
        body.extend(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n")
                .as_bytes(),
        );
    }
    for (k, name, data) in files {
        body.extend(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(data);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{boundary}--\r\n").as_bytes());
    let mut req = Request::new("POST", &accessweb::PRODUCT.url(route));
    req.headers.push((
        "content-type".into(),
        format!("multipart/form-data; boundary={boundary}"),
    ));
    req.body = body;
    b.send(req)
}

/// The rows the golden table-view leaves out: `COST`, `RACK_U`,
/// `NOTES` (NULL) and `CHANGED_TS`.
const MORE: &str = "<td class=\"mono\"><span class=\"grey\">-</span></td>\n<td class=\"mono\"><span class=\"grey\">-</span></td>\n<td class=\"mono\"><span class=\"grey\">-</span></td>\n<td class=\"mono\">2026-10-29-13.58.02.000000</td>\n</tr>";

#[test]
fn tables_golden() {
    let mut b = Browser::signed_on();
    let r = b.get("/db/tables?sp=HL%25&s=HLINV");
    assert_eq!(r.status, 200);
    assert_golden(
        &r.text(),
        "web/tables.html",
        &[
            HEADER_USER,
            // Every kind is listed: the Objects select says All.
            (
                "<option>All</option></select>",
                "<option selected>All</option></select>",
            ),
            // The handoff's ASSETS has 12 rows; what is over it too.
            (
                "<td>Hardware assets</td>\n<td class=\"num\">412</td>",
                "<td>Hardware assets</td>\n<td class=\"num\">12</td>",
            ),
            (
                "<td>Assets joined to location</td>\n<td class=\"num\">412</td>",
                "<td>Assets joined to location</td>\n<td class=\"num\">12</td>",
            ),
            (
                "<td>Assets by serial</td>\n<td class=\"num\">412</td>",
                "<td>Assets by serial</td>\n<td class=\"num\">12</td>",
            ),
        ],
    );
    // The filter stays: paging and sorting keep it.
    let t = b.get("/db/tables?sort=t").text();
    assert!(
        t.contains("value=\"HL%\"") && t.find(">ASSETLF1<").unwrap() < t.find(">ASSETS<").unwrap(),
        "{t}"
    );
    let t = b.get("/db/tables?t=Views").text();
    assert!(t.contains(">EXPIRING<") && !t.contains(">ASSETP<"), "{t}");
    let t = b.get("/db/tables?t=All&copy=ASSETS").text();
    assert!(t.contains("value=\"HLINV.ASSETS\""), "{t}");
}

#[test]
fn table_view_golden() {
    let mut b = Browser::signed_on();
    let r = b.get("/db/HLINV/ASSETS");
    assert_eq!(r.status, 200);
    let mut fixes: Vec<(String, String)> = vec![
        (HEADER_USER.0.into(), HEADER_USER.1.into()),
        // The Output select and Go need a form.
        ("<p class=\"small\"><a href=\"tables.html\">Tables</a>".into(), "<form method=\"get\"><p class=\"small\"><a href=\"tables.html\">Tables</a>".into()),
        (
            "<input type=\"submit\" value=\"Go\"></p><form method=\"get\">".into(),
            "<input type=\"submit\" value=\"Go\"></p></form><form method=\"get\">".into(),
        ),
        // ASSETS has twelve columns (insert, Columns).
        (
            "<option>ACQUIRED</option></select>".into(),
            "<option>ACQUIRED</option><option>COST</option><option>RACK_U</option><option>NOTES</option><option>CHANGED_TS</option></select>".into(),
        ),
        (
            "<th><a href=\"?sort=acquired\">ACQUIRED</a></th>".into(),
            "<th><a href=\"?sort=acquired\">ACQUIRED</a></th>\n<th><a href=\"?sort=cost\">COST</a></th>\n<th><a href=\"?sort=rack_u\">RACK_U</a></th>\n<th><a href=\"?sort=notes\">NOTES</a></th>\n<th><a href=\"?sort=changed_ts\">CHANGED_TS</a></th>".into(),
        ),
        // Its twelve rows are one page.
        (
            "Page 1 of 17 &nbsp; <span class=\"grey\">&lt; Previous</span> &nbsp; <a href=\"?page=2\">Next &gt;</a>".into(),
            "Page 1 of 1 &nbsp; <span class=\"grey\">&lt; Previous</span> &nbsp; <span class=\"grey\">Next &gt;</span>".into(),
        ),
        ("412 entries".into(), "12 entries".into()),
    ];
    for d in [
        "2025-03-11",
        "2024-11-02",
        "2024-06-20",
        "2026-09-20",
        "2026-10-01",
        "2024-09-08",
        "2023-05-02",
    ] {
        fixes.push((
            format!("<td class=\"mono\">{d}</td>\n</tr>"),
            format!("<td class=\"mono\">{d}</td>\n{MORE}"),
        ));
    }
    let f: Vec<(&str, &str)> = fixes
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_golden(&r.text(), "web/table-view.html", &f);
}

#[test]
fn table_insert_golden() {
    let mut b = Browser::signed_on();
    // The prototype's user writes dates as YYYY-MM-DD.
    b.fx.prefs
        .map
        .lock()
        .unwrap()
        .get_mut("CWELLMAN")
        .unwrap()
        .push(("df".into(), "YYYY-MM-DD".into()));
    let r = b.get("/db/HLINV/ASSETS/insert");
    assert_eq!(r.status, 200);
    assert_golden(&r.text(), "web/table-insert.html", &[HEADER_USER]);
}

#[test]
fn run_sql_golden() {
    let mut b = Browser::signed_on();
    multipart(
        &mut b,
        "/sql",
        &[("open", "Open .sql")],
        &[(
            "sqlfile",
            "retire-asset.sql",
            b"UPDATE hlinv.assets SET status = 'RETIRED' WHERE asset_tag = 'QS000301';\n",
        )],
    );
    multipart(
        &mut b,
        "/sql",
        &[("open", "Open .sql")],
        &[(
            "sqlfile",
            "assets-by-location.sql",
            b"SELECT location, COUNT(*) AS n FROM hlinv.assets GROUP BY location;\n",
        )],
    );
    let r = multipart(
        &mut b,
        "/sql",
        &[
            ("schema", "HLINV"),
            ("naming", "SQL"),
            ("commit", "Read committed"),
            ("stop", "on"),
            ("sql", SCRIPT),
            ("runall", "Run all"),
        ],
        &[],
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/accessweb/sql"),
        "{}",
        r.text()
    );
    b.fx.db.sample_history("CWELLMAN");
    let r = b.get("/sql");
    assert_eq!(r.status, 200);
    assert_golden(
        &r.text(),
        "web/run-sql.html",
        &[
            HEADER_USER,
            // The system's own completion texts.
            (
                "<td>12 rows fetched.</td>",
                "<td>12 rows fetched in 0.041 seconds.</td>",
            ),
            (
                "<td>4 rows fetched.</td>",
                "<td>4 rows fetched in 0.012 seconds.</td>",
            ),
            // An UPDATE's message is SQL7957.
            ("SQL7956</a>", "SQL7957</a>"),
            (
                "<td>1 row updated in ASSETS in HLINV. Changes",
                "<td>1 rows were updated in ASSETS in HLINV. Changes",
            ),
            // Fetch more and Fetch all need a form, and names.
            (
                "<div class=\"small\">12 rows fetched of 12. <input type=\"submit\" value=\"Fetch more\" disabled> <input type=\"submit\" value=\"Fetch all\" disabled> &nbsp; <a href=\"run-sql.html\">Copy row</a></div>",
                "<form method=\"post\"><div class=\"small\">12 rows fetched of 12. <input type=\"submit\" name=\"fetch\" value=\"Fetch more\" disabled> <input type=\"submit\" name=\"fetchall\" value=\"Fetch all\" disabled> &nbsp; <a href=\"run-sql.html\">Copy row</a></div></form>",
            ),
        ],
    );
    let r = b.get("/sql/result/2");
    assert_eq!(r.status, 200);
    assert_golden(
        &r.text(),
        "web/run-sql-result-2.html",
        &[
            HEADER_USER,
            // Sort keys are the columns' names.
            (
                "<a href=\"?sort=l\">LOCATION</a>",
                "<a href=\"?sort=location\">LOCATION</a>",
            ),
        ],
    );
}

#[test]
fn my_requests_golden() {
    let mut b = Browser::signed_on();
    let r = b.get("/requests");
    assert_eq!(r.status, 200);
    let acts = |run: &str| {
        format!(
            "<td class=\"act\"><a href=\"{run}\">Run</a> | <a href=\"{run}\">Edit</a> | <a href=\"my-requests.html\">Download .qdx / .sql</a> | <a href=\"my-requests.html\">Copy</a> | <a href=\"../shared/confirm.html\">Delete</a> | <a href=\"my-requests.html\">Share</a></td>"
        )
    };
    let (sql, to, from) = (
        acts("run-sql.html"),
        acts("copy-import.html"),
        acts("copy-export.html"),
    );
    let a = format!("{sql}\n<td>Load vendor list</td>");
    let a2 = format!("{to}\n<td>Load vendor list</td>");
    let c = format!("{sql}\n<td>Asset export</td>");
    let c2 = format!("{from}\n<td>Asset export</td>");
    assert_golden(
        &r.text(),
        "web/my-requests.html",
        // A copy request runs and opens on its copy page.
        &[HEADER_USER, (&a, &a2), (&c, &c2)],
    );
}

#[test]
fn copy_import_golden() {
    let mut b = Browser::signed_on();
    let r = multipart(
        &mut b,
        "/db/copy/to",
        &[
            ("p", ""),
            ("fmt", "CSV"),
            ("hdr", "on"),
            ("cp", "1208 (UTF-8)"),
            ("s", "HLINV"),
            ("t", "ASSETS"),
            ("m", "append"),
            ("e", "stop"),
        ],
        &[("f", "assets_br1.csv", ASSETS_BR1.as_bytes())],
    );
    assert_eq!(r.status, 200, "{}", r.text());
    let r = b.get("/db/copy/to");
    assert_golden(
        &r.text(),
        "web/copy-import.html",
        &[
            HEADER_USER,
            // The file came from this computer: no path on the system.
            (
                "value=\"/home/CWELLMAN/upload/assets_br1.csv\"",
                "value=\"\"",
            ),
            (
                "<input type=\"submit\" value=\"Browse...\">",
                "<input type=\"submit\" name=\"browse\" value=\"Browse...\">",
            ),
            // The schema's tables.
            (
                "<option>MANUFACTURERS</option><option>MOVEMENTS</option></select>",
                "<option>MANUFACTURERS</option><option>MOVEMENTS</option><option>CONTRACTS</option><option>NOTES</option></select>",
            ),
            // Four nameless buttons in one form cannot be told apart.
            (
                "<input type=\"submit\" value=\"Copy\"><input type=\"submit\" value=\"Preview 10 rows\"><input type=\"submit\" value=\"Save request (.qdx)\"><input type=\"submit\" value=\"Cancel\">",
                "<input type=\"submit\" name=\"copy\" value=\"Copy\"><input type=\"submit\" name=\"preview\" value=\"Preview 10 rows\"><input type=\"submit\" name=\"save\" value=\"Save request (.qdx)\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\">",
            ),
        ],
    );
}

#[test]
fn copy_export_golden() {
    let mut b = Browser::signed_on();
    let r = b.get("/db/copy/from?s=HLINV&t=ASSETS&q=SELECT%20*%20FROM%20hlinv.assets%20WHERE%20status%20%3D%20%27ACTIVE%27%20ORDER%20BY%20asset_tag");
    assert_eq!(r.status, 200);
    assert_golden(
        &r.text(),
        "web/copy-export.html",
        &[
            HEADER_USER,
            // One form, not two nested, its buttons inside and named.
            (
                "<form method=\"post\"><form method=\"post\">",
                "<form method=\"post\">",
            ),
            (
                "</table></form><div class=\"buttons\">",
                "</table><div class=\"buttons\">",
            ),
            (
                "<input type=\"submit\" value=\"Copy (download)\"><input type=\"submit\" value=\"Save request (.qdx)\"><input type=\"submit\" value=\"Cancel\">",
                "<input type=\"submit\" name=\"copy\" value=\"Copy (download)\"><input type=\"submit\" name=\"save\" value=\"Save request (.qdx)\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\">",
            ),
            // The schema's tables.
            (
                "<option>ASSETS</option><option>LOCATIONS</option><option>MOVEMENTS</option></select>",
                "<option>ASSETS</option><option>LOCATIONS</option><option>MANUFACTURERS</option><option>MOVEMENTS</option><option>CONTRACTS</option><option>NOTES</option></select>",
            ),
        ],
    );
}

/// Wait for a copy's page to say it has ended.
fn finished(b: &mut Browser, route: &str) -> String {
    for _ in 0..200 {
        let t = b.get(route).text();
        if !t.contains("name=\"cancel\" value=\"Cancel\"") {
            return t;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("{route} did not end");
}

#[test]
fn insert_edit_delete() {
    let mut b = Browser::signed_on();
    let mut f: Vec<(String, String)> = [
        ("c1", "QS000404"),
        ("c2", "QS-BR1-R1"),
        ("c3", "ARISTA"),
        ("c4", "7050SX-64"),
        ("c5", "JPE14210034"),
        ("c6", "SWITCH"),
        ("c7", "SPARE"),
        ("c8", "05/02/23"),
        ("c9", "1200.05"),
        ("c10", ""),
        ("n10", "on"),
        ("c11", ""),
    ]
    .iter()
    .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
    .collect();
    f.push(("insert".into(), "Insert".into()));
    let fields: Vec<(&str, &str)> = f.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let r = b.post("/db/HLINV/ASSETS/insert", &fields);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/accessweb/db/HLINV/ASSETS"),
        "{}",
        r.text()
    );
    let t = b.get("/db/HLINV/ASSETS").text();
    assert!(
        t.contains("SQL7956</a> - 1 rows were inserted into ASSETS in HLINV."),
        "{t}"
    );
    // Exact decimals; the date as ISO; long text left empty is NULL.
    let row =
        b.fx.db
            .rows("HLINV", "ASSETS")
            .into_iter()
            .find(|r| r[0] == Val::Text("QS000404".into()))
            .unwrap();
    assert_eq!(
        (&row[7], &row[8], &row[10]),
        (
            &Val::Text("2023-05-02".into()),
            &Val::Num("1200.05".into()),
            &Val::Null
        )
    );
    // The next key is suggested.
    assert!(
        b.get("/db/HLINV/ASSETS/insert")
            .text()
            .contains("value=\"QS000405\"")
    );
    // The database's own refusal, the fields kept.
    let r = b.post("/db/HLINV/ASSETS/insert", &fields);
    assert!(
        r.text().contains("SQL0803") && r.text().contains("value=\"JPE14210034\""),
        "{}",
        r.text()
    );
    // Edit.
    let t = b.get("/db/HLINV/ASSETS/edit?key=QS000404").text();
    assert!(
        t.contains("Edit record: HLINV.ASSETS")
            && t.contains("<span class=\"mono\">QS000404</span>"),
        "{t}"
    );
    let mut e = f.clone();
    e.retain(|(k, _)| k != "insert");
    e.iter_mut().find(|(k, _)| k == "c7").unwrap().1 = "ACTIVE".into();
    e.push(("update".into(), "Update".into()));
    let fields: Vec<(&str, &str)> = e.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let r = b.post("/db/HLINV/ASSETS/edit?key=QS000404", &fields);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/accessweb/db/HLINV/ASSETS")
    );
    assert!(b.get("/db/HLINV/ASSETS").text().contains("SQL7957"));
    // Delete goes through the confirmation; a bare GET deletes nothing.
    let t = b.get("/db/HLINV/ASSETS/delete/confirm?key=QS000404").text();
    assert!(
        t.contains("Confirm Delete Record") && t.contains("QS000404"),
        "{t}"
    );
    assert_eq!(b.fx.db.rows("HLINV", "ASSETS").len(), 13);
    let r = b.post(
        "/db/HLINV/ASSETS/delete/confirm?key=QS000404",
        &[("ok", "OK")],
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/accessweb/db/HLINV/ASSETS")
    );
    assert_eq!(b.fx.db.rows("HLINV", "ASSETS").len(), 12);
    // Everything ran as the user.
    assert!(
        b.fx.db
            .ran
            .lock()
            .unwrap()
            .iter()
            .all(|(u, _)| u == "CWELLMAN")
    );
}

#[test]
fn find_sort_cap_and_download() {
    let mut b = Browser::signed_on();
    let t = b
        .get("/db/HLINV/ASSETS?q=dell&col=MANUFACTURER&sort=SERIAL")
        .text();
    assert!(t.contains("5 entries") && !t.contains(">QUANTA<"), "{t}");
    assert!(t.find(">7XK2Q33<").unwrap() < t.find(">8H3K2P2<").unwrap());
    // The user's maximum rows: the page stops there and says so.
    b.post(
        "/preferences?tab=database",
        &[
            ("ds", "HLINV"),
            ("nm", "sql"),
            ("do", "HTML"),
            ("mr", "5"),
            ("apply", "Apply"),
        ],
    );
    let t = b.get("/db/HLINV/ASSETS").text();
    assert!(t.contains("5 entries") && t.contains("QAD7011"), "{t}");
    // Another output downloads every row.
    let r = b.get("/db/HLINV/ASSETS?fmt=CSV");
    assert_eq!(r.header("Content-Type"), Some("text/csv; charset=utf-8"));
    let csv = r.text();
    assert_eq!(csv.lines().count(), 13, "{csv}");
    assert!(csv.starts_with("ASSET_TAG,LOCATION,"));
    let x = b.get("/db/HLINV/ASSETS?fmt=Excel-compatible%20(.xls)");
    assert!(x.text().contains("<Workbook"));
    assert!(
        b.get("/db/HLINV/ASSETS?fmt=PDF")
            .bytes()
            .starts_with(b"%PDF")
    );
}

#[test]
fn policies_gate_the_database() {
    let mut b = Browser::as_user("WEBUSER");
    b.fx.policy
        .put(
            "WEBUSERS",
            &[
                (policy::DB_WRITE.into(), Setting::Deny),
                (policy::DB_SQL.into(), Setting::Deny),
            ],
        )
        .unwrap();
    // Write actions disappear, and are refused on a direct POST.
    let t = b.get("/db/HLINV/ASSETS").text();
    assert!(
        !t.contains(">Edit</a>") && !t.contains("Insert record"),
        "{t}"
    );
    let r = b.post(
        "/db/HLINV/ASSETS/insert",
        &[("c1", "X"), ("insert", "Insert")],
    );
    assert_eq!(r.status, 403);
    assert!(r.text().contains("XWA0001"));
    // SELECT runs; any other statement is refused with XWA0001, nothing run.
    let r = multipart(
        &mut b,
        "/sql",
        &[("sql", "SELECT * FROM hlinv.assets"), ("runall", "Run all")],
        &[],
    );
    assert_eq!(r.status, 303);
    assert!(b.get("/sql").text().contains("12 rows fetched"));
    let before = b.fx.db.ran.lock().unwrap().len();
    let r = multipart(
        &mut b,
        "/sql",
        &[
            (
                "sql",
                "SELECT * FROM hlinv.assets; DELETE FROM hlinv.assets",
            ),
            ("runall", "Run all"),
        ],
        &[],
    );
    assert_eq!(r.status, 403);
    assert!(
        r.text().contains("XWA0001") && r.text().contains("any statement"),
        "{}",
        r.text()
    );
    assert_eq!(b.fx.db.ran.lock().unwrap().len(), before);
    // Browsing denied: the pages refuse.
    b.fx.policy
        .put("WEBUSERS", &[(policy::DB_BROWSE.into(), Setting::Deny)])
        .unwrap();
    assert_eq!(b.get("/db/tables").status, 403);
}

#[test]
fn run_sql_connection_commit_and_fetch() {
    let mut b = Browser::signed_on();
    // A run under a commit level leaves changes pending: the options
    // cannot change until Commit or Rollback.
    let r = multipart(
        &mut b,
        "/sql",
        &[
            ("schema", "HLINV"),
            ("naming", "SQL"),
            ("commit", "Read committed"),
            ("stop", "on"),
            (
                "sql",
                "DELETE FROM hlinv.assets WHERE asset_tag = 'QS000101'",
            ),
            ("runall", "Run all"),
        ],
        &[],
    );
    assert_eq!(r.status, 303);
    assert_eq!(b.fx.db.rows("HLINV", "ASSETS").len(), 11);
    let r = multipart(
        &mut b,
        "/sql",
        &[
            ("schema", "QGPL"),
            ("naming", "SQL"),
            ("commit", "None"),
            ("sql", "SELECT * FROM assets"),
            ("runall", "Run all"),
        ],
        &[],
    );
    let t = r.text();
    assert!(t.contains("SQL0752") && t.contains("QAD7021"), "{t}");
    // Rollback undoes it.
    multipart(&mut b, "/sql", &[("r", "Rollback")], &[]);
    assert!(b.get("/sql").text().contains("SQL7961"));
    assert_eq!(b.fx.db.rows("HLINV", "ASSETS").len(), 12);
    // The first rows up to the maximum; Fetch more and all read the rest.
    b.post(
        "/preferences?tab=database",
        &[
            ("ds", "HLINV"),
            ("nm", "sql"),
            ("do", "HTML"),
            ("mr", "5"),
            ("apply", "Apply"),
        ],
    );
    multipart(
        &mut b,
        "/sql",
        &[
            ("schema", "HLINV"),
            ("naming", "SQL"),
            ("commit", "None"),
            ("sql", "SELECT * FROM hlinv.assets"),
            ("runall", "Run all"),
        ],
        &[],
    );
    let t = b.get("/sql").text();
    assert!(
        t.contains("5 rows fetched; more rows remain.")
            && t.contains("name=\"fetch\" value=\"Fetch more\">"),
        "{t}"
    );
    b.post("/sql", &[("fetch", "Fetch more")]);
    assert!(
        b.get("/sql")
            .text()
            .contains("10 rows fetched; more rows remain.")
    );
    b.post("/sql", &[("fetchall", "Fetch all")]);
    assert!(b.get("/sql").text().contains("12 rows fetched of 12."));
    // Download the result whole, as any format.
    let r = b.post("/sql", &[("output", "Download as"), ("fmt", "TSV")]);
    assert_eq!(r.text().lines().count(), 13);
    // Errors carry SQLSTATE and SQLCODE; stop at the first.
    multipart(
        &mut b,
        "/sql",
        &[
            ("schema", "HLINV"),
            ("naming", "SQL"),
            ("commit", "None"),
            ("stop", "on"),
            ("sql", "SELEKT 1; SELECT * FROM hlinv.assets"),
            ("runall", "Run all"),
        ],
        &[],
    );
    let t = b.get("/sql").text();
    assert!(
        t.contains("SQL0104")
            && t.contains("42601")
            && t.contains("-104")
            && !t.contains("2: SELECT"),
        "{t}"
    );
    // Run selected needs a selection; nothing to run is said.
    let r = multipart(
        &mut b,
        "/sql",
        &[("sql", "VALUES 1"), ("runsel", "Run selected")],
        &[],
    );
    assert!(r.text().contains("XWA0021"));
    let r = multipart(
        &mut b,
        "/sql",
        &[("sql", "-- nothing"), ("runall", "Run all")],
        &[],
    );
    assert!(r.text().contains("XWA0022"));
    // Save as .sql gives the script.
    let r = multipart(
        &mut b,
        "/sql",
        &[("sql", "VALUES 1;\n"), ("saveas", "Save as .sql")],
        &[],
    );
    assert_eq!(r.text(), "VALUES 1;\n");
    // History keeps what ran, newest first.
    assert_eq!(
        b.fx.db.history("CWELLMAN")[..2],
        ["SELECT * FROM hlinv.assets".to_string(), "SELEKT 1".into()]
    );
}

#[test]
fn sixteen_connections_at_most() {
    let b = Browser::signed_on();
    let db = &b.fx.db;
    let conns: Vec<u32> = (0..16).map(|_| db.open("CWELLMAN").unwrap()).collect();
    let m = db.open("CWELLMAN").unwrap_err();
    assert_eq!(m.id, "QAD7003");
    assert!(m.text.contains("No more connections allowed to QSHQ400."));
    for c in conns {
        db.close("CWELLMAN", c);
    }
}

#[test]
fn requests_save_share_download_import() {
    let mut b = Browser::signed_on();
    multipart(
        &mut b,
        "/sql",
        &[
            ("sql", "SELECT * FROM hlinv.assets;\n"),
            ("save", "Save request..."),
        ],
        &[],
    );
    let t = b.get("/requests/save").text();
    assert!(t.contains("SELECT * FROM hlinv.assets"), "{t}");
    let r = b.post(
        "/requests/save",
        &[("name", "All assets"), ("fmt", "CSV"), ("ok", "OK")],
    );
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/requests"));
    let t = b.get("/requests").text();
    assert!(
        t.contains("XWA0020") && t.contains("<td>All assets</td>"),
        "{t}"
    );
    // Share with a group; its member sees it, others do not.
    let r = b.post("/requests/6/share", &[("to", "WEBUSERS"), ("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/requests"));
    assert!(b.get("/requests").text().contains("XWA0024"));
    b.get("/signoff");
    b.sign_on("WEBUSER", accessweb::fixture::PASSWORD);
    let t = b.get("/requests?o=Shared%20with%20me").text();
    assert!(
        t.contains("All assets")
            && t.contains("Active assets by location")
            && !t.contains("Jobs in MSGW"),
        "{t}"
    );
    assert!(!t.contains(">Share</a>") && !t.contains(">Delete</a>"));
    b.get("/signoff");
    b.sign_on("CWELLMAN", accessweb::fixture::PASSWORD);
    // Download gives the desktop's files.
    let r = b.get("/requests/6/download");
    assert_eq!(
        (r.header("Content-Type"), r.text().as_str()),
        (
            Some("application/sql; charset=utf-8"),
            "SELECT * FROM hlinv.assets;\n"
        )
    );
    let qdx = b.get("/requests/5/download").text();
    assert!(
        qdx.starts_with("[qsaccess]\nkind = \"transfer\"\nversion = 2\n")
            && qdx.contains("format = \"xlsx\""),
        "{qdx}"
    );
    // Import a .qdx and a .sql.
    let r = multipart(
        &mut b,
        "/requests",
        &[],
        &[("f", "assets.qdx", qdx.as_bytes())],
    );
    assert_eq!(r.status, 303);
    let r = multipart(
        &mut b,
        "/requests",
        &[],
        &[(
            "f",
            "bad.qdx",
            b"[qsaccess]\nkind = \"display\"\nversion = 1\n[transfer]\n",
        )],
    );
    assert_eq!(r.status, 303);
    let t = b.get("/requests").text();
    assert!(
        t.contains("XWA0026") && t.contains("not a transfer request"),
        "{t}"
    );
    assert!(t.contains("<td>assets</td>"), "{t}");
    // Delete through the confirmation.
    assert!(
        b.get("/requests/6/delete/confirm")
            .text()
            .contains("Confirm Delete Request")
    );
    b.post("/requests/6/delete/confirm", &[("ok", "OK")]);
    assert!(b.get("/requests").text().contains("XWA0023"));
    // Run opens Run SQL with the script.
    let t = b.get("/sql?req=3").text();
    assert!(t.contains("qsys2.active_job_info"), "{t}");
}

#[test]
fn copy_to_table_and_back() {
    let mut b = Browser::signed_on();
    let fields = [
        ("fmt", "CSV"),
        ("hdr", "on"),
        ("cp", "1208 (UTF-8)"),
        ("s", "HLINV"),
        ("t", "ASSETS"),
        ("m", "append"),
        ("e", "stop"),
    ];
    multipart(
        &mut b,
        "/db/copy/to",
        &fields,
        &[("f", "assets_br1.csv", ASSETS_BR1.as_bytes())],
    );
    // Preview: the values as the table takes them.
    let r = multipart(
        &mut b,
        "/db/copy/to",
        &[("preview", "Preview 10 rows")],
        &[],
    );
    let t = r.text();
    assert!(
        t.contains("<h2>Preview</h2>") && t.contains("1200.00"),
        "{t}"
    );
    // Copy: a progress page, then the summary.
    let r = multipart(&mut b, "/db/copy/to", &[("copy", "Copy")], &[]);
    let at = r.header("Location").unwrap().to_string();
    assert!(at.starts_with("/qs400/accessweb/db/copy/to?job="), "{at}");
    let t = finished(&mut b, &at);
    assert!(
        t.contains("QAD7030")
            && t.contains("Transfer assets_br1.csv -&gt; HLINV.ASSETS complete: 2 rows in"),
        "{t}"
    );
    assert!(
        t.contains("XWA0030</a> - Copy completed: 2 rows copied, 0 errors."),
        "{t}"
    );
    assert_eq!(b.fx.db.rows("HLINV", "ASSETS").len(), 14);
    let row =
        b.fx.db
            .rows("HLINV", "ASSETS")
            .into_iter()
            .find(|r| r[0] == Val::Text("QS000404".into()))
            .unwrap();
    assert_eq!(row[8], Val::Num("1200.00".into()));
    // Again: the keys are taken, the copy stops and keeps nothing.
    let r = multipart(&mut b, "/db/copy/to", &[("copy", "Copy")], &[]);
    let t = finished(&mut b, r.header("Location").unwrap());
    assert!(
        t.contains("QAD7031") && t.contains("failed at row 1 (line 2 of assets_br1.csv)"),
        "{t}"
    );
    assert_eq!(b.fx.db.rows("HLINV", "ASSETS").len(), 14);
    // Update matching keys: the rows change in place.
    let r = multipart(
        &mut b,
        "/db/copy/to",
        &[("m", "update"), ("e", "stop"), ("copy", "Copy")],
        &[],
    );
    let t = finished(&mut b, r.header("Location").unwrap());
    assert!(t.contains("QAD7030"), "{t}");
    assert_eq!(b.fx.db.rows("HLINV", "ASSETS").len(), 14);
    // The update mode has no .qdx form; append saves as the desktop's.
    let r = multipart(
        &mut b,
        "/db/copy/to",
        &[("m", "update"), ("save", "Save request (.qdx)")],
        &[],
    );
    assert!(r.text().contains("XWA0004"), "{}", r.text());
    let r = multipart(
        &mut b,
        "/db/copy/to",
        &[("m", "append"), ("save", "Save request (.qdx)")],
        &[],
    );
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/requests"));
    let groups = vec!["ADMINS".to_string()];
    let saved =
        b.fx.db
            .requests("CWELLMAN", &groups)
            .into_iter()
            .last()
            .unwrap();
    assert!(
        saved.body.contains("direction = \"upload\"")
            && saved.body.contains("target = \"ASSET_TAG\""),
        "{}",
        saved.body
    );
    // Every format back out of the table, and read again.
    for (fmt, ext) in [
        ("CSV", "csv"),
        ("TSV", "tsv"),
        ("XLSX", "xlsx"),
        ("ODS", "ods"),
        ("Fixed width", "txt"),
    ] {
        let r = b.post(
            "/db/copy/from",
            &[
                ("s", "HLINV"),
                ("t", "ASSETS"),
                ("q", ""),
                ("fmt", fmt),
                ("hdr", "on"),
                ("dt", "ISO (YYYY-MM-DD)"),
                ("cp", "UTF-8 (1208)"),
                ("le", "LF"),
                ("copy", "Copy (download)"),
            ],
        );
        let at = r.header("Location").unwrap().to_string();
        let t = finished(&mut b, &at);
        assert!(t.contains("QAD7030") && t.contains("14 rows"), "{t}");
        let file = b.get(&format!("{at}&file=1"));
        assert_eq!(
            file.header("Content-Disposition")
                .map(|d| d.contains(&format!("assets.{ext}"))),
            Some(true)
        );
        let f = accessweb::data::files::Format::of(fmt).unwrap();
        let sheet = accessweb::data::files::read(
            f,
            file.bytes(),
            true,
            accessweb::data::files::Encoding::Utf8,
        )
        .unwrap();
        assert_eq!(sheet.rows.len(), 14, "{fmt}");
        assert_eq!(sheet.names[0], "ASSET_TAG");
    }
    // A query that changes rows is not a source.
    let r = b.post(
        "/db/copy/from",
        &[
            ("s", "HLINV"),
            ("t", "ASSETS"),
            ("q", "DELETE FROM hlinv.assets"),
            ("fmt", "CSV"),
            ("copy", "Copy (download)"),
        ],
    );
    assert!(r.text().contains("XWA0004"));
    assert_eq!(b.fx.db.rows("HLINV", "ASSETS").len(), 14);
}

#[test]
fn the_first_query_is_shown() {
    let mut b = Browser::signed_on();
    multipart(
        &mut b,
        "/sql",
        &[
            ("schema", "HLINV"),
            ("naming", "SQL"),
            ("commit", "None"),
            (
                "sql",
                "UPDATE hlinv.assets SET status = 'SPARE' WHERE asset_tag = 'QS000101'; SELECT * FROM hlinv.assets",
            ),
            ("runall", "Run all"),
        ],
        &[],
    );
    let t = b.get("/sql").text();
    assert!(
        t.contains("1: UPDATE (1 row changed)")
            && t.contains("<b>2: SELECT * (12 rows)</b>")
            && t.contains(">QS000101<"),
        "{t}"
    );
}
