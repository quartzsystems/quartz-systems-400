// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Run SQL (`/sql`, `/sql/result/<n>`), as the desktop's: a script of
//! statements in a connection of the page's own, run at the cursor,
//! selected or all, stopping at the first error when asked; the default
//! schema, naming and commit level, Commit and Rollback; a tab per
//! result, the user's maximum rows at first and Fetch more or all; each
//! statement's message with its SQLSTATE and SQLCODE; the history;
//! `.sql` files opened and saved; a result downloaded in any format.
//! A statement that only reads needs policy "Run SQL - SELECT", any
//! other "Run SQL - any statement".

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::{Kind as MsgKind, Message};
use qsweb::partials::{Cell, Col, List, list_table, msgid, tabs, upload_form};

use super::state::{self, NEW_SCRIPT, Res, SqlWork};
use super::{GROUP, cell, crumb, name_of, options, pick, picks};
use crate::data::files::{self, Format};
use crate::data::{script, sqltext};
use crate::env::db::{Commit, Options, Outcome, ResultColumn};
use crate::routes::Rq;
use crate::{PRODUCT, msgs, policy};

const CRUMB: &[&str] = &[GROUP, "Run SQL"];

/// Whether the user may run SQL at all.
fn may(rq: &Rq<'_>) -> Result<(), Response> {
    if rq.allowed(policy::DB_SELECT) || rq.allowed(policy::DB_SQL) {
        return Ok(());
    }
    rq.policy(policy::DB_SELECT, CRUMB)
}

/// The schemas the options offer: the user's, and the catalog's.
fn schemas(rq: &Rq<'_>) -> Vec<String> {
    let mut v = rq.env().prefs.schemas(rq.user());
    if !v.iter().any(|s| s == "QSYS2") {
        v.push("QSYS2".into());
        v[1..].sort();
    }
    v
}

/// `SELECT asset_tag ...`, `SELECT location, COUNT(*)`, `UPDATE`.
fn tab_label(stmt: &str) -> String {
    let verb = script::verb(stmt);
    if verb != "SELECT" {
        return verb;
    }
    let rest = stmt.trim_start()[6..].trim_start();
    let upper = rest.to_ascii_uppercase();
    let end = upper
        .find(" FROM ")
        .or_else(|| upper.find("\nFROM "))
        .unwrap_or(rest.len());
    // The select list's items, outside parentheses.
    let list = &rest[..end];
    let mut items = Vec::new();
    let (mut depth, mut from) = (0i32, 0usize);
    for (i, c) in list.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                items.push(list[from..i].trim());
                from = i + 1;
            }
            _ => {}
        }
    }
    items.push(list[from..].trim());
    let mut label = String::from("SELECT");
    for (n, item) in items.iter().enumerate() {
        let item = item.split_whitespace().collect::<Vec<_>>().join(" ");
        let up = item.to_ascii_uppercase();
        let item = up.find(" AS ").map_or(item.as_str(), |a| &item[..a]);
        let next = if n == 0 {
            format!("{label} {item}")
        } else {
            format!("{label}, {item}")
        };
        if next.len() > 25 {
            label.push_str(" ...");
            return label;
        }
        label = next;
    }
    label
}

/// What the tab says of the rows.
fn tab_count(r: &Res) -> String {
    let plural = |n: usize, w: &str| {
        if n == 1 {
            format!("1 {w}")
        } else {
            format!("{n} {w}s")
        }
    };
    if r.failed {
        "failed".into()
    } else if r.query() {
        plural(r.rows.len(), "row")
    } else if !matches!(
        script::verb(&r.statement).as_str(),
        "INSERT" | "UPDATE" | "DELETE" | "MERGE"
    ) {
        "done".into()
    } else {
        format!(
            "{} changed",
            plural(usize::try_from(r.changed).unwrap_or(0), "row")
        )
    }
}

/// The table and schema a statement changes (`UPDATE hlinv.assets`).
fn target(stmt: &str, schema: &str) -> (String, String) {
    let words: Vec<&str> = stmt.split_whitespace().collect();
    let at = words
        .iter()
        .position(|w| matches!(w.to_ascii_uppercase().as_str(), "INTO" | "FROM" | "UPDATE"))
        .and_then(|i| words.get(i + 1))
        .copied()
        .unwrap_or("");
    let at = at.split('(').next().unwrap_or(at);
    match at.split_once(['.', '/']) {
        Some((s, t)) => (name_of(t), name_of(s)),
        None => (name_of(at), schema.to_string()),
    }
}

/// The message of a statement that ran.
fn completion(rq: &Rq<'_>, stmt: &str, o: &Outcome, opts: &Options) -> Message {
    if let Some(m) = &o.message
        && o.failed
    {
        return m.clone();
    }
    if o.query() {
        let secs = format!("{}.{:03}", o.ms / 1000, o.ms % 1000);
        return rq.message(
            MsgKind::Done,
            "SQL7958",
            &[&o.rows.len().to_string(), &secs],
        );
    }
    let (t, s) = target(stmt, &opts.schema);
    let n = o.changed.to_string();
    let verb = script::verb(stmt);
    let mut m = match verb.as_str() {
        "INSERT" | "UPDATE" | "DELETE" | "MERGE" if o.changed == 0 => {
            rq.message(MsgKind::Info, "SQL0100", &[&verb])
        }
        "INSERT" => rq.message(MsgKind::Done, "SQL7956", &[&t, &s, &n]),
        "UPDATE" | "MERGE" => rq.message(MsgKind::Done, "SQL7957", &[&t, &s, &n]),
        "DELETE" => rq.message(MsgKind::Done, "SQL7955", &[&t, &s, &n]),
        "COMMIT" => rq.message(MsgKind::Done, "SQL7960", &[]),
        "ROLLBACK" => rq.message(MsgKind::Done, "SQL7961", &[]),
        _ => rq.message(MsgKind::Done, "SQL7977", &[&verb]),
    };
    if opts.commit != Commit::None && o.changed > 0 {
        let _ = write!(
            m.text,
            " Changes are uncommitted (commit level {}).",
            opts.commit.label()
        );
    }
    m
}

/// The page's connection, opened when it has none, with `opts` set.
fn connect(rq: &Rq<'_>, w: &mut SqlWork, opts: &Options) -> Result<u32, Vec<Message>> {
    let db = &rq.env().db;
    let fresh = w.conn.is_none();
    let c = if let Some(c) = w.conn {
        c
    } else {
        let c = db.open(rq.user()).map_err(|m| vec![m])?;
        w.conn = Some(c);
        c
    };
    if fresh || w.opts != *opts {
        if let Err(m) = db.options(rq.user(), c, opts) {
            let mut v = vec![m];
            if w.pending {
                v.push(rq.message(MsgKind::Info, msgs::UNCOMMITTED, &["Run SQL"]));
            }
            return Err(v);
        }
        w.opts = opts.clone();
    }
    Ok(c)
}

/// Run `statements` in the page's connection.
fn run(
    rq: &Rq<'_>,
    w: &mut SqlWork,
    statements: &[script::Statement],
    opts: &Options,
) -> Vec<Message> {
    let c = match connect(rq, w, opts) {
        Ok(c) => c,
        Err(m) => return m,
    };
    let db = &rq.env().db;
    let max = rq.prefs().max_rows();
    // Cursors of the last run go with it.
    w.results.clear();
    for st in statements {
        let o = db.run(rq.user(), c, &st.text, &[], max);
        let message = completion(rq, &st.text, &o, opts);
        if !o.query() && o.changed > 0 && opts.commit != Commit::None {
            w.pending = true;
        }
        let failed = o.failed;
        w.results.push(Res {
            statement: st.text.clone(),
            columns: o.columns,
            rows: o.rows,
            cursor: o.cursor,
            changed: o.changed,
            message: Some(message),
            sqlstate: if o.sqlstate.is_empty() {
                "00000".into()
            } else {
                o.sqlstate
            },
            sqlcode: o.sqlcode,
            ms: o.ms,
            failed,
        });
        if failed && w.stop {
            break;
        }
    }
    db.remember(
        rq.user(),
        &statements
            .iter()
            .map(|s| s.text.clone())
            .collect::<Vec<_>>(),
    );
    Vec::new()
}

/// The tab `/sql` shows first: the first query's, else the first.
fn first_tab(w: &SqlWork) -> usize {
    w.results.iter().position(Res::query).map_or(1, |i| i + 1)
}

/// The tab shown (`?tab=`, from 1).
fn tab_of(rq: &Rq<'_>, w: &SqlWork) -> usize {
    rq.query()
        .get("tab")
        .and_then(|t| t.parse().ok())
        .filter(|t| (1..=w.results.len()).contains(t))
        .unwrap_or_else(|| first_tab(w))
}

/// Rows sorted by `?sort=` (a column's name, lower case).
fn sorted(rq: &Rq<'_>, r: &Res) -> Vec<Vec<crate::env::db::Val>> {
    let mut rows = r.rows.clone();
    let key = rq.query().str("sort").to_ascii_uppercase();
    if let Some(i) = r.columns.iter().position(|c| c.name == key) {
        let num = r.columns[i].numeric();
        rows.sort_by(|a, b| {
            if num {
                let f = |v: &crate::env::db::Val| v.text().parse::<f64>().unwrap_or(f64::MIN);
                f(&a[i]).total_cmp(&f(&b[i]))
            } else {
                a[i].text().cmp(&b[i].text())
            }
        });
    }
    rows
}

/// The result tabs; `n` the one shown.
fn result_tabs(w: &SqlWork, n: usize) -> String {
    let first = first_tab(w);
    let list: Vec<(String, String)> = w
        .results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let k = i + 1;
            let label = format!("{k}: {} ({})", tab_label(&r.statement), tab_count(r));
            let href = if k == first || !r.query() {
                PRODUCT.url(&if k == first {
                    "/sql".to_string()
                } else {
                    format!("/sql?tab={k}")
                })
            } else {
                PRODUCT.url(&format!("/sql/result/{k}"))
            };
            (label, href)
        })
        .collect();
    let refs: Vec<(&str, String)> = list.iter().map(|(l, h)| (l.as_str(), h.clone())).collect();
    tabs(&refs, n - 1)
}

/// A result's table.
fn result_table(rq: &Rq<'_>, r: &Res, keep: Vec<(String, String)>) -> String {
    let rows = sorted(rq, r);
    list_table(&List {
        cols: r
            .columns
            .iter()
            .map(|c| Col::sorted(&c.name, &c.name.to_ascii_lowercase()))
            .collect(),
        rows: rows
            .iter()
            .map(|row| {
                row.iter()
                    .zip(&r.columns)
                    .map(|(v, c)| cell(v, c.numeric()))
                    .collect()
            })
            .collect(),
        empty: "No rows.".into(),
        keep,
    })
}

/// The fetch line: how many rows, Fetch more and Fetch all.
fn fetch_line(rq: &Rq<'_>, r: &Res, n: usize, first: usize, copy: bool) -> String {
    let more = r.cursor.is_some();
    let count = if more {
        format!("{} rows fetched; more rows remain.", r.rows.len())
    } else {
        format!("{0} rows fetched of {0}.", r.rows.len())
    };
    let dis = if more { "" } else { " disabled" };
    let copy_link = if copy {
        format!(
            " &nbsp; <a href=\"{}\">Copy row</a>",
            esc(&PRODUCT.url(&format!(
                "/sql{}",
                query(&[("tab", &n.to_string()), ("copy", "1")])
            )))
        )
    } else {
        String::new()
    };
    let action = if n == first {
        String::new()
    } else {
        PRODUCT.url(&format!("/sql?tab={n}"))
    };
    qsweb::partials::form(
        &rq.ctx,
        &action,
        &format!(
            "<div class=\"small\">{count} <input type=\"submit\" name=\"fetch\" value=\"Fetch more\"{dis}> <input type=\"submit\" name=\"fetchall\" value=\"Fetch all\"{dis}>{copy_link}</div>"
        ),
    )
}

/// The messages of the last run.
fn messages_table(rq: &Rq<'_>, w: &SqlWork) -> String {
    let mut rows: Vec<(usize, &Res)> = w
        .results
        .iter()
        .enumerate()
        .map(|(i, r)| (i + 1, r))
        .collect();
    if rq.query().str("sort") == "id" {
        rows.sort_by(|a, b| {
            a.1.message
                .as_ref()
                .map(|m| &m.id)
                .cmp(&b.1.message.as_ref().map(|m| &m.id))
        });
    }
    let cells = rows
        .iter()
        .map(|(n, r)| {
            let m = r
                .message
                .clone()
                .unwrap_or_else(|| Message::new(MsgKind::Info, "", ""));
            let mut href = PRODUCT.msg(&m.id);
            for (i, d) in m.data.iter().enumerate() {
                href.push(if i == 0 { '?' } else { '&' });
                href.push_str("d=");
                href.push_str(&qsweb::http::form::encode(d));
            }
            let count = if r.query() {
                r.rows.len().to_string()
            } else {
                r.changed.to_string()
            };
            vec![
                Cell::num(&n.to_string()),
                Cell::mono_html(format!(
                    "<a href=\"{}\" target=\"_blank\">{}</a>",
                    esc(&href),
                    esc(&m.id)
                )),
                Cell::mono(&r.sqlstate),
                Cell::num(&r.sqlcode.to_string()),
                Cell::text(&m.text),
                Cell::num(&count),
                Cell::text(&format!("{}.{:03} s", r.ms / 1000, r.ms % 1000)),
            ]
        })
        .collect();
    list_table(&List {
        cols: vec![
            Col::plain("Statement"),
            Col::sorted("Message ID", "id"),
            Col::plain("SQLSTATE"),
            Col::plain("SQLCODE"),
            Col::plain("Message text"),
            Col::plain("Rows"),
            Col::plain("Time"),
        ],
        rows: cells,
        empty: "No statements have run.".into(),
        keep: Vec::new(),
    })
}

/// The main page.
#[allow(clippy::too_many_lines)]
fn show(rq: &Rq<'_>, w: &SqlWork, messages: Vec<Message>) -> Response {
    let opts = &w.opts;
    let schema = if opts.schema.is_empty() {
        rq.prefs().get(crate::prefs::SCHEMA)
    } else {
        opts.schema.clone()
    };
    let commits: Vec<String> = Commit::ALL.iter().map(|c| c.label().to_string()).collect();
    let recent = if w.recent.is_empty() {
        String::new()
    } else {
        let links: Vec<String> = w
            .recent
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                format!(
                    "<a href=\"{}\">{}</a>",
                    esc(&PRODUCT.url(&format!("/sql?recent={i}"))),
                    esc(name)
                )
            })
            .collect();
        format!(" &nbsp; Recent: {}", links.join(", "))
    };
    let check = |name: &str, on: bool| {
        format!(
            "<input type=\"checkbox\" name=\"{name}\"{}>",
            if on { " checked" } else { "" }
        )
    };
    let mut inner = format!(
        "<table class=\"props\"><tr><th>Options</th><td>Default schema {} &nbsp; Naming {} &nbsp; Commit level {} &nbsp; <input type=\"submit\" name=\"c\" value=\"Commit\"> <input type=\"submit\" name=\"r\" value=\"Rollback\"> &nbsp; {} Stop at first error &nbsp; {} Show CCSID 65535 data as text</td></tr>",
        pick("schema", &schemas(rq), &schema),
        picks(
            "naming",
            &["SQL", "System"],
            if opts.system_naming { "System" } else { "SQL" }
        ),
        pick("commit", &commits, opts.commit.label()),
        check("stop", w.stop),
        check("hex", w.hex)
    );
    let _ = write!(
        inner,
        "<tr><th>Script</th><td><textarea name=\"sql\" rows=\"10\" cols=\"100\" class=\"conf\">{}</textarea><br><span class=\"small grey\">Line 1, column 1</span></td></tr><tr><th>File</th><td><input type=\"file\" name=\"sqlfile\" size=\"30\"> <input type=\"submit\" name=\"open\" value=\"Open .sql\"> <input type=\"submit\" name=\"saveas\" value=\"Save as .sql\">{recent}</td></tr></table>",
        esc(&w.script)
    );
    let formats: Vec<String> = Format::DOWNLOAD
        .iter()
        .map(|f| f.label().to_string())
        .collect();
    let save = if rq.allowed(policy::DB_SAVE) {
        "<input type=\"submit\" name=\"save\" value=\"Save request...\">"
    } else {
        ""
    };
    let _ = write!(
        inner,
        "<div class=\"buttons\"><input type=\"submit\" name=\"run1\" value=\"Run statement (at cursor)\"><input type=\"submit\" name=\"runsel\" value=\"Run selected\"><input type=\"submit\" name=\"runall\" value=\"Run all\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\"><input type=\"submit\" name=\"output\" value=\"Download as\"> {}{save}<input type=\"submit\" name=\"clear\" value=\"Clear\"></div>",
        pick("fmt", &formats, "")
    );
    let tab = tab_of(rq, w);
    let first = first_tab(w);
    let action = if tab == first {
        String::new()
    } else {
        PRODUCT.url(&format!("/sql?tab={tab}"))
    };
    let mut body = upload_form(&rq.ctx, &action, &inner);
    if !w.results.is_empty() {
        body.push_str("<h2>Results</h2>");
        body.push_str(&result_tabs(w, tab));
        let r = &w.results[tab - 1];
        if r.query() {
            let keep = if tab == first {
                Vec::new()
            } else {
                vec![("tab".to_string(), tab.to_string())]
            };
            body.push_str(&result_table(rq, r, keep));
            let max = rq.prefs().max_rows();
            let _ = write!(
                body,
                "<div class=\"small\">{} rows returned (maximum {max}). </div>",
                r.rows.len()
            );
            body.push_str(&fetch_line(rq, r, tab, first, true));
            if rq.query().has("copy") {
                let text = crate::data::files::write(
                    Format::Tsv,
                    &r.columns,
                    &r.rows,
                    &files::Write {
                        crlf: false,
                        ..files::Write::default()
                    },
                );
                let _ = write!(
                    body,
                    "<p class=\"small grey\">The rows as tab-separated text, to copy:</p><textarea rows=\"8\" cols=\"100\" class=\"conf\" readonly>{}</textarea>",
                    esc(&String::from_utf8_lossy(&text))
                );
            }
        }
        body.push_str("<h2>Messages</h2>");
        body.push_str(&messages_table(rq, w));
        body.push_str("<p class=\"small grey\">Uncommitted changes: the level, naming and schema cannot change until Commit or Rollback (QAD7021); closing the page asks the same. A lost connection marks the result incomplete (QAD7011 / QAD7010) and nothing is run again by itself.</p>");
    }
    let history = rq.env().db.history(rq.user());
    if !history.is_empty() {
        body.push_str("<h2>History (last 100 on this system)</h2><div class=\"small mono\">");
        let links: Vec<String> = history
            .iter()
            .enumerate()
            .map(|(i, h)| {
                format!(
                    "<a href=\"{}\">{}</a>",
                    esc(&PRODUCT.url(&format!("/sql?h={i}"))),
                    esc(h)
                )
            })
            .collect();
        body.push_str(&links.join("<br>"));
        body.push_str("</div>");
    }
    rq.page(Page {
        title: "Run SQL".into(),
        crumb: crumb(&["Run SQL"]),
        h1: "Run SQL".into(),
        desc: Some("Run SQL scripts on this system in a connection of this page's own, as QS/400 Access for Desktop's Run SQL does.".into()),
        messages,
        body,
        help: "db-sql".into(),
        nav: Some((GROUP, Some("/sql"))),
        ..Page::default()
    })
}

/// Policy for each statement: SELECT-only or any.
fn allowed(rq: &Rq<'_>, statements: &[script::Statement]) -> Result<(), Response> {
    for st in statements {
        let f = if sqltext::reads_only(&st.text) {
            policy::DB_SELECT
        } else {
            policy::DB_SQL
        };
        rq.policy(f, CRUMB)?;
    }
    Ok(())
}

/// A script that ends with `;` and a new line.
fn script_of(text: &str) -> String {
    let t = text.trim_end();
    let t = t.strip_suffix(';').unwrap_or(t);
    format!("{NEW_SCRIPT}{t};\n")
}

/// `GET /sql` shows the page (`?table=`, `?h=`, `?recent=`, `?req=`
/// load a script); `POST` runs, fetches, commits, opens and saves.
#[allow(clippy::too_many_lines)] // one arm per button
pub fn page(rq: &Rq<'_>) -> Response {
    if let Err(r) = may(rq) {
        return r;
    }
    let work = state::work(rq);
    let mut wk = work.lock().expect("work");
    let w = &mut wk.sql;
    if w.conn.is_none() && w.results.is_empty() && w.opts == Options::default() {
        w.opts = options(rq, "", "", "");
    }
    let q = rq.query();
    if !rq.post() {
        let mut messages = Vec::new();
        if let Some(t) = q.get("table") {
            w.script = script_of(&format!("SELECT * FROM {t}"));
        } else if let Some(h) = q.get("h").and_then(|h| h.parse::<usize>().ok()) {
            if let Some(s) = rq.env().db.history(rq.user()).get(h) {
                w.script = script_of(s);
            }
        } else if let Some(r) = q.get("recent").and_then(|h| h.parse::<usize>().ok()) {
            if let Some((name, text)) = w.recent.get(r).cloned() {
                w.script = text;
                w.file = name;
            }
        } else if let Some(id) = q.get("req").and_then(|h| h.parse::<u32>().ok()) {
            let groups = rq.env().system.groups(rq.user());
            if let Some(r) = rq
                .env()
                .db
                .requests(rq.user(), &groups)
                .into_iter()
                .find(|r| r.id == id && r.typ == crate::env::db::ReqType::Sql)
            {
                w.script.clone_from(&r.body);
                // A link runs only what reads; anything else waits for Run.
                let sts = script::split(&w.script);
                if q.has("run")
                    && !sts.is_empty()
                    && sts.iter().all(|s| sqltext::reads_only(&s.text))
                {
                    if let Err(resp) = allowed(rq, &sts) {
                        return resp;
                    }
                    let opts = w.opts.clone();
                    messages = run(rq, w, &sts, &opts);
                    let mut saved = r;
                    saved.last_run = rq.env().system.now();
                    if saved.owner == rq.user() {
                        let _ = rq.env().db.save_request(rq.user(), &saved);
                    }
                }
            }
        }
        let snapshot = w.clone();
        drop(wk);
        return show(rq, &snapshot, messages);
    }
    let f = rq.form();
    let back = |w: &SqlWork| {
        let tab = tab_of(rq, w);
        if tab == first_tab(w) {
            "/sql".to_string()
        } else {
            format!("/sql?tab={tab}")
        }
    };
    // Every post keeps the fields typed.
    if f.has("sql") {
        w.script = f.str("sql").replace("\r\n", "\n");
        w.stop = f.has("stop");
        w.hex = f.has("hex");
    }
    let opts = if f.has("schema") {
        options(rq, f.str("schema"), f.str("naming"), f.str("commit"))
    } else {
        w.opts.clone()
    };
    let user = rq.user().to_string();
    let db = &rq.env().db;
    let mut flash: Vec<Message> = Vec::new();
    if f.has("run1") || f.has("runsel") || f.has("runall") {
        let text = w.script.clone();
        let sts = if f.has("runall") {
            script::split(&text)
        } else if f.has("runsel") {
            match (
                f.get("sel0").and_then(|a| a.parse().ok()),
                f.get("sel1").and_then(|b| b.parse().ok()),
            ) {
                (Some(a), Some(b)) if a != b => script::selected(&text, a, b),
                _ => {
                    let snapshot = w.clone();
                    drop(wk);
                    return show(
                        rq,
                        &snapshot,
                        vec![rq.message(MsgKind::Error, msgs::SELECT_STATEMENTS, &[])],
                    );
                }
            }
        } else {
            script::at(
                &text,
                f.get("pos").and_then(|p| p.parse().ok()).unwrap_or(0),
            )
            .into_iter()
            .collect()
        };
        if sts.is_empty() {
            let snapshot = w.clone();
            drop(wk);
            return show(
                rq,
                &snapshot,
                vec![rq.message(MsgKind::Error, msgs::NO_STATEMENT, &[])],
            );
        }
        if let Err(r) = allowed(rq, &sts) {
            return r;
        }
        flash = run(rq, w, &sts, &opts);
    } else if f.has("c") || f.has("r") {
        if let Err(r) = rq.policy(policy::DB_SQL, CRUMB) {
            return r;
        }
        let commit = f.has("c");
        if let Some(c) = w.conn {
            let o = db.end_unit(&user, c, commit);
            if let Some(m) = o.message.filter(|_| o.failed) {
                flash.push(m);
            } else {
                w.pending = false;
                flash.push(rq.message(
                    MsgKind::Done,
                    if commit { "SQL7960" } else { "SQL7961" },
                    &[],
                ));
            }
        } else {
            flash.push(rq.message(
                MsgKind::Done,
                if commit { "SQL7960" } else { "SQL7961" },
                &[],
            ));
        }
    } else if f.has("fetch") || f.has("fetchall") {
        let tab = tab_of(rq, w);
        let max = if f.has("fetchall") {
            usize::MAX
        } else {
            rq.prefs().max_rows()
        };
        if let (Some(c), Some(r)) = (w.conn, w.results.get_mut(tab - 1))
            && let Some(cur) = r.cursor
        {
            let o = db.fetch(&user, c, cur, max);
            if o.failed {
                flash.extend(o.message);
                r.cursor = None;
            } else {
                r.rows.extend(o.rows);
                r.cursor = o.cursor;
            }
        }
    } else if f.has("clear") {
        w.results.clear();
    } else if f.has("cancel") {
        if let Some(c) = w.conn {
            db.cancel(&user, c);
        }
    } else if f.has("open") {
        let part = rq
            .req
            .multipart()
            .unwrap_or_default()
            .into_iter()
            .find(|p| p.name == "sqlfile" && p.filename.is_some());
        match part {
            Some(p) if !p.data.is_empty() => {
                let name = p.filename.clone().unwrap_or_default();
                match String::from_utf8(p.data) {
                    Ok(text) => {
                        w.script = text.replace("\r\n", "\n");
                        w.recent.retain(|r| r.0 != name);
                        w.recent.insert(0, (name.clone(), w.script.clone()));
                        w.recent.truncate(5);
                        w.file = name;
                    }
                    Err(_) => flash.push(rq.message(
                        MsgKind::Error,
                        msgs::FILE_UNREADABLE,
                        &[&name, "it is not UTF-8 text"],
                    )),
                }
            }
            _ => flash.push(rq.message(
                MsgKind::Error,
                msgs::FILE_UNREADABLE,
                &["(none)", "no file was sent"],
            )),
        }
    } else if f.has("saveas") {
        let name = if w.file.is_empty() {
            "script.sql".to_string()
        } else {
            w.file.clone()
        };
        w.recent.retain(|r| r.0 != name);
        w.recent.insert(0, (name.clone(), w.script.clone()));
        w.recent.truncate(5);
        return Response::download(
            &name,
            "application/sql; charset=utf-8",
            w.script.clone().into_bytes(),
        );
    } else if f.has("output") {
        let tab = tab_of(rq, w);
        let fmt = Format::of(f.str("fmt")).unwrap_or(Format::Csv);
        let r = w.results.get(tab - 1).cloned();
        match (r, w.conn) {
            (Some(r), Some(c)) if r.query() && !r.failed => {
                if let Err(resp) = allowed(rq, &script::split(&r.statement)) {
                    return resp;
                }
                let o = db.run(&user, c, &r.statement, &[], usize::MAX);
                if let Some(m) = super::failure(&o) {
                    flash.push(m);
                } else {
                    let cols: Vec<ResultColumn> = o.columns;
                    let data = files::write(
                        fmt,
                        &cols,
                        &o.rows,
                        &files::Write {
                            title: format!("Result {tab}"),
                            ..files::Write::default()
                        },
                    );
                    return Response::download(
                        &format!("result-{tab}.{}", fmt.extension()),
                        fmt.content_type(),
                        data,
                    );
                }
            }
            _ => flash.push(rq.message(MsgKind::Error, msgs::NO_RESULT, &[&tab.to_string()])),
        }
    } else if f.has("save") {
        if let Err(r) = rq.policy(policy::DB_SAVE, CRUMB) {
            return r;
        }
        return rq.redirect("/requests/save?type=sql");
    } else if !f.has("sql") {
        // New SQL request... from My requests: a new script.
        *w = SqlWork {
            conn: w.conn,
            opts: w.opts.clone(),
            recent: w.recent.clone(),
            pending: w.pending,
            ..SqlWork::default()
        };
    }
    // Options typed without a run: kept for the next one.
    if f.has("schema") && w.conn.is_none() {
        w.opts = opts;
    }
    let to = back(w);
    // More than one message: the page itself, as a flash holds one.
    if flash.len() > 1 {
        let snapshot = w.clone();
        drop(wk);
        return show(rq, &snapshot, flash);
    }
    drop(wk);
    for m in flash {
        rq.flash(m);
    }
    rq.redirect(&to)
}

/// `GET /sql/result/<n>`: one result on a page of its own.
pub fn result(rq: &Rq<'_>, n: &str) -> Response {
    if let Err(r) = may(rq) {
        return r;
    }
    let work = state::work(rq);
    let w = work.lock().expect("work").sql.clone();
    let Some(k) = n
        .parse::<usize>()
        .ok()
        .filter(|k| (1..=w.results.len()).contains(k))
    else {
        return rq.not_found(&format!("result {n} of Run SQL"));
    };
    let r = &w.results[k - 1];
    let mut body = result_tabs(&w, k);
    if r.query() {
        body.push_str(&result_table(rq, r, Vec::new()));
        if r.cursor.is_some() {
            body.push_str(&fetch_line(rq, r, k, first_tab(&w), false));
        }
        let _ = write!(
            body,
            "<div class=\"small\">{} <a href=\"{}\">Back to the script</a></div>",
            if r.cursor.is_some() {
                format!("{} rows fetched; more rows remain.", r.rows.len())
            } else {
                format!("{0} rows fetched of {0}.", r.rows.len())
            },
            esc(&PRODUCT.url("/sql"))
        );
    } else if let Some(m) = &r.message {
        let _ = write!(body, "<p>{} - {}</p>", msgid(&PRODUCT, &m.id), esc(&m.text));
    }
    rq.page(Page {
        title: "Run SQL".into(),
        crumb: crumb(&["Run SQL", &format!("Result {k}")]),
        h1: "Run SQL".into(),
        body,
        help: "db-sql".into(),
        nav: Some((GROUP, Some("/sql"))),
        ..Page::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_targets() {
        assert_eq!(
            tab_label("SELECT asset_tag, location, manufacturer FROM hlinv.assets"),
            "SELECT asset_tag ..."
        );
        assert_eq!(
            tab_label("SELECT location, COUNT(*) AS n FROM hlinv.assets GROUP BY location"),
            "SELECT location, COUNT(*)"
        );
        assert_eq!(tab_label("update t set a = 1"), "UPDATE");
        assert_eq!(
            target("UPDATE hlinv.assets SET x = 1", "Q"),
            ("ASSETS".into(), "HLINV".into())
        );
        assert_eq!(
            target("insert into t values (1)", "QGPL"),
            ("T".into(), "QGPL".into())
        );
    }
}
