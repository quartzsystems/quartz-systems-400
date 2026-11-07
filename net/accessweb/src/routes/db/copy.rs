// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Data Transfer, as the desktop's: Copy data to table
//! (`/db/copy/to`, a CSV, TSV, XLSX, ODS or fixed-width file into a
//! table: append, replace every row, or update the rows whose keys
//! match; stop at the first row the system refuses or skip it) and
//! Copy data from table (`/db/copy/from`, a table's or a query's rows
//! to a file). Values move as text: decimals are exact. A copy runs in
//! a connection of its own off the request, with a progress page
//! (`?job=`) and Cancel, and ends with `QAD7030`, `QAD7031` or
//! `QAD7032`. `.qdx` requests are the desktop's.

use std::fmt::Write as _;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::{Form, Response};
use qsweb::msg::{Kind as MsgKind, Message};
use qsweb::partials::{Button, Cell, Col, List, buttons, form, list_table, upload_form};

use super::record::to_iso;
use super::state::{self, Job, UploadForm};
use super::view::keys;
use super::{GROUP, crumb, pick, picks};
use crate::data::files::{self, Encoding, Format};
use crate::data::{qdx, sqltext};
use crate::env::Env;
use crate::env::db::{Column, Commit, Db, Kind, Options, ReqType, Request, ResultColumn, Val};
use crate::routes::{Rq, shared};
use crate::{PRODUCT, msgs, policy, prefs};

/// Rows a batch sends.
const BATCH: usize = 100;
/// Rows the mapping is read from.
const SAMPLE: usize = 50;
/// Skipped rows the summary names.
const NAMED: usize = 10;

/// The Format select of Copy data to table.
const TO_FORMATS: &[&str] = &[
    "CSV",
    "TSV",
    "XLSX (first sheet)",
    "ODS (first sheet)",
    "Fixed width",
];
/// Its code pages.
const TO_PAGES: &[&str] = &["1208 (UTF-8)", "1252", "37"];
/// Copy data from table's code pages.
const FROM_PAGES: &[&str] = &["UTF-8 (1208)", "1252", "37"];
/// The skipped column.
const SKIP: &str = "(skip)";

/// The schemas the user may list.
fn user_schemas(rq: &Rq<'_>) -> Vec<String> {
    rq.env().prefs.schemas(rq.user())
}

/// A schema's tables.
fn tables_of(rq: &Rq<'_>, schema: &str) -> Vec<String> {
    rq.env()
        .db
        .tables(rq.user(), schema)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.kind == Kind::Table)
        .map(|e| e.name)
        .collect()
}

/// The columns a copy can write.
fn targets(cols: &[Column]) -> Vec<&Column> {
    cols.iter().filter(|c| !c.generated()).collect()
}

/// A file value as a column takes it: dates in ISO or the user's
/// format, numbers trimmed.
fn convert(c: &Column, v: &Val, date_fmt: &str) -> Val {
    match v {
        Val::Null => Val::Null,
        Val::Num(n) if !c.numeric() => Val::Text(n.clone()),
        v if c.numeric() => Val::Num(v.text().trim().to_string()),
        v if c.typ == "DATE" => Val::Text(to_iso(&v.text(), date_fmt)),
        v => Val::Text(v.text()),
    }
}

/// Whether a value fits its column; why not.
fn fits(c: &Column, v: &Val) -> bool {
    let t = v.text();
    match v {
        Val::Null => c.nullable,
        _ if c.numeric() => {
            let d = t.trim_start_matches(['-', '+']);
            let (int, frac) = d.split_once('.').unwrap_or((d, ""));
            let digits_ok =
                !d.is_empty() && int.chars().chain(frac.chars()).all(|x| x.is_ascii_digit());
            let scale = c.scale.unwrap_or(0) as usize;
            let room = match c.typ.as_str() {
                "DECIMAL" | "NUMERIC" => (c.length as usize).saturating_sub(scale),
                "SMALLINT" => 5,
                "INTEGER" | "INT" => 10,
                "BIGINT" => 19,
                _ => 64,
            };
            digits_ok
                && int.trim_start_matches('0').len() <= room
                && (c.scale.is_none() || frac.len() <= scale)
        }
        _ if matches!(
            c.typ.as_str(),
            "CHAR" | "VARCHAR" | "GRAPHIC" | "VARGRAPHIC"
        ) =>
        {
            t.chars().count() <= c.length as usize
        }
        _ if c.typ == "DATE" => {
            let b = t.as_bytes();
            b.len() == 10 && b[4] == b'-' && b[7] == b'-'
        }
        _ => true,
    }
}

/// Each source column's target: the column of its name, else the next
/// one in order whose type takes the sample value.
fn guess(
    names: &[String],
    sample: &[Val],
    cols: &[&Column],
    date_fmt: &str,
) -> Vec<Option<String>> {
    let mut used = vec![false; cols.len()];
    let mut out = vec![None; names.len()];
    for (i, n) in names.iter().enumerate() {
        if let Some(k) = cols
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(n.trim()))
        {
            used[k] = true;
            out[i] = Some(cols[k].name.clone());
        }
    }
    let mut next = 0;
    for (i, o) in out.iter_mut().enumerate() {
        if o.is_some() {
            continue;
        }
        let v = sample.get(i).cloned().unwrap_or(Val::Null);
        while next < cols.len() {
            let k = next;
            next += 1;
            if !used[k] && (v.is_null() || fits(cols[k], &convert(cols[k], &v, date_fmt))) {
                used[k] = true;
                *o = Some(cols[k].name.clone());
                break;
            }
        }
    }
    out
}

/// A copy's progress page, or its end.
fn progress(rq: &Rq<'_>, job: &Arc<Job>, back: &str, title: &str) -> Response {
    if rq.post() {
        if rq.form().has("cancel") {
            job.cancel.store(true, Ordering::SeqCst);
        }
        return rq.redirect(&format!("{back}?job={}", job.id));
    }
    if rq.query().has("file") {
        if let Some((name, ct, data)) = job.file.lock().expect("job").clone() {
            return Response::download(&name, ct, data);
        }
        return rq.not_found(&format!("file of copy {}", job.id));
    }
    let done = job.done.lock().expect("job").clone();
    let rows = job.rows.load(Ordering::SeqCst);
    let total = job.total.load(Ordering::SeqCst);
    let secs = rq.env().system.now().saturating_sub(job.started);
    let of = if total > 0 {
        format!(" of {total}")
    } else {
        String::new()
    };
    let mut body = format!(
        "<table class=\"props\" cellspacing=\"0\">\n<tr><th>Transfer</th><td class=\"mono\">{}</td></tr>\n<tr><th>Rows</th><td>{rows}{of}</td></tr>\n<tr><th>Time</th><td>{secs} s</td></tr>\n</table>\n",
        esc(&job.name)
    );
    let (messages, refresh) = match done {
        None => {
            body.push_str(&form(
                &rq.ctx,
                "",
                &buttons(&[Button::new("cancel", "Cancel")]),
            ));
            (Vec::new(), Some(2))
        }
        Some(ms) => {
            if job.file.lock().expect("job").is_some() {
                let _ = write!(
                    body,
                    "<p><a href=\"{}\">Download the file</a></p>",
                    esc(&PRODUCT.url(&format!(
                        "{back}{}",
                        query(&[("job", &job.id.to_string()), ("file", "1")])
                    )))
                );
            }
            let _ = write!(
                body,
                "<p><a href=\"{}\">{title}</a></p>",
                esc(&PRODUCT.url(back))
            );
            (ms, None)
        }
    };
    rq.page(Page {
        title: title.into(),
        crumb: crumb(&[title, "Progress"]),
        h1: title.into(),
        desc: Some("The copy runs in a connection of its own, as your user profile.".into()),
        messages,
        body,
        help: "db-copy".into(),
        nav: Some((GROUP, Some("/db/copy/to"))),
        refresh,
        ..Page::default()
    })
}

/// The job `?job=` names.
fn job_of(rq: &Rq<'_>, upload: bool) -> Option<Arc<Job>> {
    let id: u32 = rq.query().get("job")?.parse().ok()?;
    let w = state::work(rq);
    let w = w.lock().expect("work");
    w.jobs
        .iter()
        .find(|j| j.id == id && j.upload == upload)
        .cloned()
}

/// A new job of the session.
fn new_job(rq: &Rq<'_>, name: String, upload: bool, total: u64) -> Arc<Job> {
    let w = state::work(rq);
    let mut w = w.lock().expect("work");
    let id = w.jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
    let j = Arc::new(Job {
        id,
        name,
        upload,
        started: rq.env().system.now(),
        ..Job::default()
    });
    j.total.store(total, Ordering::SeqCst);
    w.jobs.push(j.clone());
    w.jobs
        .retain(|x| x.done.lock().is_ok_and(|d| d.is_none()) || x.id + 10 > id);
    j
}

/// How long something took, as the summary says it.
fn took(t: Instant) -> String {
    let ms = t.elapsed().as_millis();
    format!("{}.{} s", ms / 1000, ms % 1000 / 100)
}

/// What an upload runs over.
struct Plan {
    name: String,
    file: String,
    schema: String,
    table: String,
    /// Target columns and the source column of each.
    map: Vec<(Column, usize)>,
    rows: Vec<Vec<Val>>,
    /// Lines before the first row.
    header: usize,
    mode: String,
    skip: bool,
    date_fmt: String,
    keys: Vec<String>,
}

/// A source row as the target's values.
fn values(p: &Plan, row: &[Val]) -> Vec<Val> {
    p.map
        .iter()
        .map(|(c, i)| convert(c, row.get(*i).unwrap_or(&Val::Null), &p.date_fmt))
        .collect()
}

fn msg(env: &Env, kind: MsgKind, id: &str, data: &[&str]) -> Message {
    shared::message(env, kind, id, data)
}

/// Run an upload: every row, in batches, under one unit of work.
#[allow(clippy::too_many_lines)] // one loop over the batches
fn upload(env: &Env, user: &str, p: &Plan, job: &Job) -> Vec<Message> {
    let db = &env.db;
    let t0 = Instant::now();
    let c = match db.open(user) {
        Ok(c) => c,
        Err(m) => return vec![m],
    };
    let mut out = Vec::new();
    let mut opts = Options {
        commit: Commit::ReadCommitted,
        ..Options::default()
    };
    if db.options(user, c, &opts).is_err() {
        opts.commit = Commit::None;
        let _ = db.options(user, c, &opts);
    }
    let names: Vec<String> = p.map.iter().map(|(c, _)| c.name.clone()).collect();
    let insert = sqltext::insert(&p.schema, &p.table, &names);
    let fail = |row: usize, o: &crate::env::db::Outcome| {
        let why = o.message.as_ref().map_or_else(
            || o.sqlstate.clone(),
            |m| m.text.trim_end_matches('.').to_string(),
        );
        let line = row + p.header;
        msg(
            env,
            MsgKind::Error,
            msgs::TRANSFER_FAILED,
            &[&p.name, &row.to_string(), &line.to_string(), &p.file, &why],
        )
    };
    if p.mode == "replace" {
        let o = db.run(user, c, &sqltext::delete_all(&p.schema, &p.table), &[], 0);
        if o.failed {
            // A table that cannot be changed under commitment control.
            if o.sqlcode == -7008 && opts.commit != Commit::None {
                db.end_unit(user, c, false);
                opts.commit = Commit::None;
                let _ = db.options(user, c, &opts);
                out.push(msg(
                    env,
                    MsgKind::Info,
                    "QAD7034",
                    &[&format!("{}.{}", p.schema, p.table)],
                ));
                let o = db.run(user, c, &sqltext::delete_all(&p.schema, &p.table), &[], 0);
                if o.failed {
                    db.close(user, c);
                    out.push(super::failure(&o).unwrap_or_else(|| fail(0, &o)));
                    return out;
                }
            } else {
                db.close(user, c);
                out.push(super::failure(&o).unwrap_or_else(|| fail(0, &o)));
                return out;
            }
        }
    }
    let mut done = 0usize;
    let mut errors: Vec<Message> = Vec::new();
    let mut nerrors = 0usize;
    let update = (p.mode == "update").then(|| {
        let set: Vec<String> = names
            .iter()
            .filter(|n| !p.keys.contains(n))
            .cloned()
            .collect();
        (sqltext::update(&p.schema, &p.table, &set, &p.keys), set)
    });
    for (b, chunk) in p.rows.chunks(BATCH).enumerate() {
        if job.cancel.load(Ordering::SeqCst) {
            db.end_unit(user, c, false);
            db.close(user, c);
            return vec![msg(
                env,
                MsgKind::Info,
                msgs::TRANSFER_CANCELLED,
                &[&p.name],
            )];
        }
        let first = b * BATCH + 1;
        let vals: Vec<Vec<Val>> = chunk.iter().map(|r| values(p, r)).collect();
        // One row at a time when rows are updated or skipped.
        if let Some((sql, set)) = &update {
            for (k, v) in vals.iter().enumerate() {
                let mut params: Vec<Val> = set
                    .iter()
                    .map(|n| v[names.iter().position(|x| x == n).unwrap_or(0)].clone())
                    .collect();
                params.extend(
                    p.keys
                        .iter()
                        .map(|k| v[names.iter().position(|x| x == k).unwrap_or(0)].clone()),
                );
                let mut o = db.run(user, c, sql, &[params], 0);
                if !o.failed && o.changed == 0 {
                    o = db.run(user, c, &insert, std::slice::from_ref(v), 0);
                }
                if o.failed {
                    nerrors += 1;
                    if !p.skip {
                        db.end_unit(user, c, false);
                        db.close(user, c);
                        return vec![fail(first + k, &o)];
                    }
                    if errors.len() < NAMED {
                        errors.push(fail(first + k, &o));
                    }
                } else {
                    done += 1;
                }
            }
        } else {
            let o = db.run(user, c, &insert, &vals, 0);
            if o.failed {
                let at = o.row.unwrap_or(0);
                if !p.skip {
                    db.end_unit(user, c, false);
                    db.close(user, c);
                    return vec![fail(first + at, &o)];
                }
                // The rows before the failing one went in; the rest one by one.
                done += at;
                nerrors += 1;
                if errors.len() < NAMED {
                    errors.push(fail(first + at, &o));
                }
                for (k, v) in vals.iter().enumerate().skip(at + 1) {
                    let o = db.run(user, c, &insert, std::slice::from_ref(v), 0);
                    if o.failed {
                        nerrors += 1;
                        if errors.len() < NAMED {
                            errors.push(fail(first + k, &o));
                        }
                    } else {
                        done += 1;
                    }
                }
            } else {
                done += chunk.len();
            }
        }
        job.rows.store(done as u64, Ordering::SeqCst);
    }
    let end = db.end_unit(user, c, true);
    db.close(user, c);
    if let Some(m) = super::failure(&end) {
        return vec![m];
    }
    out.push(msg(
        env,
        MsgKind::Done,
        msgs::TRANSFER_DONE,
        &[&p.name, &done.to_string(), &took(t0)],
    ));
    out.push(msg(
        env,
        MsgKind::Done,
        msgs::COPY_DONE,
        &[&done.to_string(), &nerrors.to_string()],
    ));
    out.extend(errors);
    out
}

/// The form's fields into the upload's state.
fn take(rq: &Rq<'_>, f: &Form, u: &mut UploadForm) {
    if let Some(fmt) = f.get("fmt").and_then(Format::of) {
        u.fmt = fmt;
    }
    if f.has("fmt") {
        u.header = f.has("hdr");
    }
    if let Some(e) = f.get("cp").and_then(Encoding::of) {
        u.enc = e;
    }
    if let Some(s) = f.get("s") {
        if s != u.schema {
            u.table.clear();
        }
        u.schema = s.to_string();
    }
    if let Some(t) = f.get("t").filter(|t| !t.is_empty()) {
        u.table = t.to_string();
    }
    if let Some(m) = f
        .get("m")
        .filter(|m| ["append", "replace", "update"].contains(m))
    {
        u.mode = m.to_string();
    }
    if let Some(e) = f.get("e").filter(|e| ["stop", "skip"].contains(e)) {
        u.on_error = e.to_string();
    }
    for i in 0..u.map.len() {
        if let Some(v) = f.get(&format!("map{i}")) {
            u.map[i] = (v != SKIP).then(|| v.to_string());
        }
    }
    let _ = rq;
}

fn fresh_upload(rq: &Rq<'_>) -> UploadForm {
    let q = rq.query();
    let schemas = user_schemas(rq);
    let pref = rq.prefs().get(prefs::SCHEMA);
    let schema = q
        .get("s")
        .map(super::name_of)
        .filter(|s| schemas.contains(s))
        .unwrap_or(pref);
    UploadForm {
        header: true,
        table: q.get("t").map(super::name_of).unwrap_or_default(),
        schema,
        mode: "append".into(),
        on_error: "stop".into(),
        ..UploadForm::default()
    }
}

/// A `.qdx` upload request into the form.
fn from_qdx(u: &mut UploadForm, up: &qdx::Upload) {
    u.schema.clone_from(&up.schema);
    u.table.clone_from(&up.table);
    u.fmt = up.options.format.file();
    u.header = up.options.header;
    u.mode = match up.mode {
        qdx::Mode::Append => "append".into(),
        qdx::Mode::Replace => "replace".into(),
    };
    if !up.mapping.is_empty() {
        let mut map = vec![None; u.sheet.names.len().max(up.mapping.len())];
        for m in &up.mapping {
            let i = match &m.from {
                qdx::SourceColumn::Position(p) => Some(*p),
                qdx::SourceColumn::Name(n) => u.sheet.names.iter().position(|x| x == n),
            };
            if let Some(i) = i.filter(|i| *i < map.len()) {
                map[i] = Some(m.target.clone());
            }
        }
        u.map = map;
    }
    if u.file.is_empty() {
        u.file.clone_from(&up.file);
    }
}

/// The upload as a `.qdx` request.
fn to_qdx(rq: &Rq<'_>, u: &UploadForm) -> Result<String, Message> {
    let mode = match u.mode.as_str() {
        "replace" => qdx::Mode::Replace,
        "append" => qdx::Mode::Append,
        // The desktop's requests have no update mode.
        _ => {
            return Err(rq.message(
                MsgKind::Error,
                msgs::BAD_VALUE,
                &["Update matching keys", "If rows exist (in a .qdx request)"],
            ));
        }
    };
    let mapping = u
        .map
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            t.as_ref().map(|t| qdx::Map {
                target: t.clone(),
                from: if u.header {
                    qdx::SourceColumn::Name(u.sheet.names[i].clone())
                } else {
                    qdx::SourceColumn::Position(i)
                },
            })
        })
        .collect();
    Ok(qdx::to_text(&qdx::Request::Upload {
        system: rq.env().system.name(),
        upload: qdx::Upload {
            file: u.file.clone(),
            options: qdx::FileOptions {
                format: qdx::Format::of(u.fmt).unwrap_or_default(),
                header: u.header,
                ..qdx::FileOptions::default()
            },
            schema: u.schema.clone(),
            table: u.table.clone(),
            mode,
            mapping,
            ..qdx::Upload::default()
        },
    }))
}

/// `/db/copy/to`: Copy data to table.
#[allow(clippy::too_many_lines)] // one form
pub fn to(rq: &Rq<'_>) -> Response {
    const TITLE: &str = "Copy data to table";
    if let Err(r) = rq.policy(policy::DB_COPY_TO, &[GROUP, TITLE]) {
        return r;
    }
    if let Some(j) = job_of(rq, true) {
        return progress(rq, &j, "/db/copy/to", TITLE);
    }
    let work = state::work(rq);
    let date_fmt = rq.prefs().get(prefs::DATE);
    let mut messages = Vec::new();
    let mut wk = work.lock().expect("work");
    let q = rq.query();
    if wk.upload.is_none() || (!rq.post() && (q.has("s") || q.has("t") || q.has("req"))) {
        wk.upload = Some(fresh_upload(rq));
    }
    let mut u = wk.upload.clone().unwrap_or_default();
    drop(wk);
    if !rq.post()
        && let Some(id) = q.get("req").and_then(|i| i.parse::<u32>().ok())
    {
        let groups = rq.env().system.groups(rq.user());
        if let Some(r) = rq
            .env()
            .db
            .requests(rq.user(), &groups)
            .into_iter()
            .find(|r| r.id == id && r.typ == ReqType::CopyTo)
        {
            match qdx::from_text(&r.body) {
                Ok(qdx::Request::Upload { upload, .. }) => from_qdx(&mut u, &upload),
                Ok(_) => messages.push(rq.message(
                    MsgKind::Error,
                    msgs::NOT_A_REQUEST,
                    &[&r.name, "it copies from a table"],
                )),
                Err(e) => messages.push(rq.message(
                    MsgKind::Error,
                    msgs::NOT_A_REQUEST,
                    &[&r.name, &e.to_string()],
                )),
            }
        }
    }
    let mut preview = false;
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            work.lock().expect("work").upload = None;
            return rq.redirect("/db/tables");
        }
        take(rq, &f, &mut u);
        let parts = rq.req.multipart().unwrap_or_default();
        if let Some(p) = parts
            .iter()
            .find(|p| p.name == "qdx" && p.filename.is_some() && !p.data.is_empty())
        {
            let name = p.filename.clone().unwrap_or_default();
            match String::from_utf8(p.data.clone())
                .map_err(|_| qdx::Error::Parse("not text".into()))
                .and_then(|t| qdx::from_text(&t))
            {
                Ok(qdx::Request::Upload { upload, .. }) => from_qdx(&mut u, &upload),
                Ok(_) => messages.push(rq.message(
                    MsgKind::Error,
                    msgs::NOT_A_REQUEST,
                    &[&name, "it copies from a table"],
                )),
                Err(e) => messages.push(rq.message(
                    MsgKind::Error,
                    msgs::NOT_A_REQUEST,
                    &[&name, &e.to_string()],
                )),
            }
        }
        if let Some(p) = parts
            .iter()
            .find(|p| p.name == "f" && p.filename.is_some() && !p.data.is_empty())
        {
            let name = p.filename.clone().unwrap_or_default();
            if !f.has("fmt")
                && let Some(fmt) = Format::of_name(&name)
            {
                u.fmt = fmt;
            }
            match files::read(u.fmt, &p.data, u.header, u.enc) {
                Ok(sheet) => {
                    u.file = name;
                    u.sheet = sheet;
                    u.map.clear();
                }
                Err(e) => {
                    messages.push(rq.message(MsgKind::Error, msgs::FILE_UNREADABLE, &[&name, &e]));
                }
            }
        }
        if f.has("browse") || (!f.str("p").trim().is_empty() && u.sheet.names.is_empty()) {
            // BLOCKED(rel): a file on the system is read through the Files area's trait, not built at this level; refused with XWA0099.
            messages.push(rq.message(
                MsgKind::Error,
                msgs::NOT_AVAILABLE,
                &["Copy data to table from a file on this system"],
            ));
        }
        preview = f.has("preview");
    }
    let schemas = user_schemas(rq);
    let tables = tables_of(rq, &u.schema);
    if !tables.contains(&u.table) {
        u.table = tables.first().cloned().unwrap_or_default();
    }
    let cols = if u.table.is_empty() {
        Vec::new()
    } else {
        rq.env()
            .db
            .columns(rq.user(), &u.schema, &u.table)
            .unwrap_or_default()
    };
    let tcols = targets(&cols);
    if u.map.len() != u.sheet.names.len() {
        let sample = u.sheet.rows.first().cloned().unwrap_or_default();
        u.map = guess(&u.sheet.names, &sample, &tcols, &date_fmt);
    }
    work.lock().expect("work").upload = Some(u.clone());
    let f = rq.form();
    if rq.post() && messages.is_empty() && (f.has("copy") || f.has("save")) {
        if u.sheet.names.is_empty() {
            messages.push(rq.message(
                MsgKind::Error,
                msgs::FILE_UNREADABLE,
                &["(none)", "no file was sent"],
            ));
        } else if f.has("save") {
            match to_qdx(rq, &u) {
                Ok(body) => {
                    return super::requests::keep(
                        rq,
                        &Request {
                            name: format!("{} -> {}.{}", u.file, u.schema, u.table),
                            typ: ReqType::CopyTo,
                            body,
                            format: u.fmt.label().into(),
                            ..Request::default()
                        },
                    );
                }
                Err(m) => messages.push(m),
            }
        } else {
            let map: Vec<(Column, usize)> = u
                .map
                .iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    t.as_ref()
                        .and_then(|t| cols.iter().find(|c| &c.name == t))
                        .map(|c| (c.clone(), i))
                })
                .collect();
            let keys = keys(&cols);
            if map.is_empty() {
                messages.push(rq.message(
                    MsgKind::Error,
                    msgs::BAD_VALUE,
                    &[SKIP, "Target column"],
                ));
            } else if u.mode == "update"
                && (keys.is_empty() || !keys.iter().all(|k| map.iter().any(|(c, _)| &c.name == k)))
            {
                messages.push(rq.message(
                    MsgKind::Error,
                    msgs::BAD_VALUE,
                    &[
                        "Update matching keys",
                        "If rows exist (the key columns must be mapped)",
                    ],
                ));
            } else {
                let name = format!("{} -> {}.{}", u.file, u.schema, u.table);
                let plan = Plan {
                    name: name.clone(),
                    file: u.file.clone(),
                    schema: u.schema.clone(),
                    table: u.table.clone(),
                    map,
                    rows: u.sheet.rows.clone(),
                    header: usize::from(u.header),
                    mode: u.mode.clone(),
                    skip: u.on_error == "skip",
                    date_fmt: date_fmt.clone(),
                    keys,
                };
                let job = new_job(rq, name, true, plan.rows.len() as u64);
                let env = rq.env().clone();
                let user = rq.user().to_string();
                let j = job.clone();
                std::thread::spawn(move || {
                    let ms = upload(&env, &user, &plan, &j);
                    *j.done.lock().expect("job") = Some(ms);
                });
                return rq.redirect(&format!("/db/copy/to?job={}", job.id));
            }
        }
    }
    // The page.
    let fmt_label = TO_FORMATS
        .iter()
        .find(|l| Format::of(l) == Some(u.fmt))
        .copied()
        .unwrap_or("CSV");
    let page_label = match u.enc {
        Encoding::Utf8 => TO_PAGES[0],
        Encoding::Cp1252 => TO_PAGES[1],
        Encoding::Cp37 => TO_PAGES[2],
    };
    let radio = |name: &str, v: &str, label: &str, on: &str| {
        format!(
            "<input type=\"radio\" name=\"{name}\" value=\"{v}\"{}> {label}",
            if v == on { " checked" } else { "" }
        )
    };
    let mut inner = String::from(
        "<table class=\"props\" cellspacing=\"0\">\n<tr class=\"subrow\"><td colspan=\"2\">Source</td></tr>\n",
    );
    inner.push_str("<tr><th>File</th><td><input type=\"file\" name=\"f\" size=\"40\"> <span class=\"small grey\">CSV, TSV, XLSX, ODS or fixed-width text</span></td></tr>\n");
    inner.push_str("<tr><th>Or file on this system</th><td><input type=\"text\" name=\"p\" value=\"\" size=\"45\" class=\"cmd\"> <input type=\"submit\" name=\"browse\" value=\"Browse...\"></td></tr>\n");
    let _ = writeln!(
        inner,
        "<tr><th>Format</th><td>{} &nbsp; <input type=\"checkbox\" name=\"hdr\"{}> First row is column names &nbsp; Code page: {}</td></tr>",
        picks("fmt", TO_FORMATS, fmt_label),
        if u.header { " checked" } else { "" },
        picks("cp", TO_PAGES, page_label)
    );
    inner.push_str("<tr class=\"subrow\"><td colspan=\"2\">Target</td></tr>\n");
    let _ = writeln!(
        inner,
        "<tr><th>Schema</th><td>{}</td></tr>",
        pick("s", &schemas, &u.schema)
    );
    let _ = writeln!(
        inner,
        "<tr><th>Table</th><td>{}</td></tr>",
        pick("t", &tables, &u.table)
    );
    let _ = writeln!(
        inner,
        "<tr><th>If rows exist</th><td>{} {} {}</td></tr>",
        radio("m", "append", "Append", &u.mode),
        radio("m", "replace", "Replace all rows", &u.mode),
        radio("m", "update", "Update matching keys", &u.mode)
    );
    let _ = writeln!(
        inner,
        "<tr><th>On error</th><td>{} {}</td></tr>",
        radio("e", "stop", "Stop and report the row", &u.on_error),
        radio(
            "e",
            "skip",
            "Skip the row, list it in the summary",
            &u.on_error
        )
    );
    inner.push_str("<tr class=\"subrow\"><td colspan=\"2\">Request file</td></tr>\n");
    inner.push_str("<tr><th>Open a saved request</th><td><input type=\"file\" name=\"qdx\" size=\"30\"> <span class=\"small grey\">.qdx from QS/400 Access for Desktop (passwords are never in it); the same file runs here</span></td></tr>\n</table>");
    inner.push_str("<h2>Field mapping</h2>");
    if u.sheet.names.is_empty() {
        inner.push_str("<p class=\"small grey\">Choose a file and press Preview 10 rows to map its columns to the table's.</p>");
    } else {
        let _ = write!(
            inner,
            "<p class=\"small grey\">Read from the first {SAMPLE} rows of {}. Choose the target column for each source column, or (skip).</p>",
            esc(&u.file)
        );
        let options: Vec<String> = std::iter::once(SKIP.to_string())
            .chain(tcols.iter().map(|c| c.name.clone()))
            .collect();
        let sample = u.sheet.rows.first().cloned().unwrap_or_default();
        let rows = u
            .sheet
            .names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let target = u.map.get(i).cloned().flatten();
                let v = sample.get(i).cloned().unwrap_or(Val::Null);
                let typ = target
                    .as_ref()
                    .and_then(|t| cols.iter().find(|c| &c.name == t))
                    .map(Column::type_label)
                    .unwrap_or_default();
                vec![
                    Cell::mono(n),
                    Cell::html(if v.is_null() {
                        "<span class=\"grey\">-</span>".into()
                    } else {
                        qsweb::partials::mono(&v.text())
                    }),
                    Cell::html(pick(
                        &format!("map{i}"),
                        &options,
                        target.as_deref().unwrap_or(SKIP),
                    )),
                    Cell::text(&typ),
                ]
            })
            .collect();
        inner.push_str(&list_table(&List {
            cols: vec![
                Col::plain("Source column"),
                Col::plain("Sample value"),
                Col::plain("Target column"),
                Col::plain("Type"),
            ],
            rows,
            empty: String::new(),
            keep: Vec::new(),
        }));
        if preview {
            let mapped: Vec<(&Column, usize)> = u
                .map
                .iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    t.as_ref()
                        .and_then(|t| cols.iter().find(|c| &c.name == t))
                        .map(|c| (c, i))
                })
                .collect();
            let mut misfit: Vec<String> = Vec::new();
            for row in u.sheet.rows.iter().take(SAMPLE) {
                for (c, i) in &mapped {
                    let v = convert(c, row.get(*i).unwrap_or(&Val::Null), &date_fmt);
                    if !fits(c, &v) && !misfit.contains(&c.name) {
                        misfit.push(c.name.clone());
                        messages.push(rq.message(
                            MsgKind::Info,
                            msgs::MISFIT,
                            &[&c.name, &v.text(), &c.type_label()],
                        ));
                    }
                }
            }
            let prows = u
                .sheet
                .rows
                .iter()
                .take(10)
                .map(|row| {
                    mapped
                        .iter()
                        .map(|(c, i)| {
                            super::cell(
                                &convert(c, row.get(*i).unwrap_or(&Val::Null), &date_fmt),
                                c.numeric(),
                            )
                        })
                        .collect()
                })
                .collect();
            inner.push_str("<h2>Preview</h2>");
            inner.push_str(&list_table(&List {
                cols: mapped.iter().map(|(c, _)| Col::plain(&c.name)).collect(),
                rows: prows,
                empty: "The file has no rows.".into(),
                keep: Vec::new(),
            }));
        }
    }
    let mut bs = vec![
        Button::new("copy", "Copy"),
        Button::new("preview", "Preview 10 rows"),
    ];
    if rq.allowed(policy::DB_SAVE) {
        bs.push(Button::new("save", "Save request (.qdx)"));
    }
    bs.push(Button::new("cancel", "Cancel"));
    inner.push_str(&buttons(&bs));
    rq.page(Page {
        title: TITLE.into(),
        crumb: crumb(&[TITLE]),
        h1: TITLE.into(),
        desc: Some("Data Transfer to the system, as QS/400 Access for Desktop does it; decimals are moved exactly.".into()),
        messages,
        body: upload_form(&rq.ctx, "", &inner),
        help: "db-copy".into(),
        nav: Some((GROUP, Some("/db/copy/to"))),
        ..Page::default()
    })
}

/// What a download writes.
struct Down {
    name: String,
    sql: String,
    file: String,
    fmt: Format,
    write: files::Write,
}

/// Run a download: every row, fetched in batches, written at the end.
fn download(env: &Env, user: &str, d: &Down, job: &Job) -> Vec<Message> {
    let db: &Arc<dyn Db> = &env.db;
    let t0 = Instant::now();
    let c = match db.open(user) {
        Ok(c) => c,
        Err(m) => return vec![m],
    };
    let mut o = db.run(user, c, &d.sql, &[], 500);
    if let Some(m) = super::failure(&o) {
        db.close(user, c);
        return vec![m];
    }
    let cols: Vec<ResultColumn> = o.columns.clone();
    let mut rows = std::mem::take(&mut o.rows);
    let mut cursor = o.cursor;
    job.rows.store(rows.len() as u64, Ordering::SeqCst);
    while let Some(cur) = cursor {
        if job.cancel.load(Ordering::SeqCst) {
            db.close(user, c);
            return vec![msg(
                env,
                MsgKind::Info,
                msgs::TRANSFER_CANCELLED,
                &[&d.name],
            )];
        }
        let f = db.fetch(user, c, cur, 500);
        if let Some(m) = super::failure(&f) {
            db.close(user, c);
            return vec![m];
        }
        rows.extend(f.rows);
        cursor = f.cursor;
        job.rows.store(rows.len() as u64, Ordering::SeqCst);
    }
    db.close(user, c);
    let data = files::write(d.fmt, &cols, &rows, &d.write);
    *job.file.lock().expect("job") = Some((d.file.clone(), d.fmt.content_type(), data));
    vec![
        msg(
            env,
            MsgKind::Done,
            msgs::TRANSFER_DONE,
            &[&d.name, &rows.len().to_string(), &took(t0)],
        ),
        msg(
            env,
            MsgKind::Done,
            msgs::COPY_DONE,
            &[&rows.len().to_string(), "0"],
        ),
    ]
}

/// `/db/copy/from`: Copy data from table.
#[allow(clippy::too_many_lines)] // one form
pub fn from(rq: &Rq<'_>) -> Response {
    const TITLE: &str = "Copy data from table";
    if let Err(r) = rq.policy(policy::DB_COPY_FROM, &[GROUP, TITLE]) {
        return r;
    }
    if let Some(j) = job_of(rq, false) {
        return progress(rq, &j, "/db/copy/from", TITLE);
    }
    let q = rq.query();
    let p = rq.prefs();
    let f = if rq.post() { rq.form() } else { q.clone() };
    let mut messages = Vec::new();
    // A saved request fills the form.
    let mut req: Option<qdx::Download> = None;
    if let Some(id) = q.get("req").and_then(|i| i.parse::<u32>().ok()) {
        let groups = rq.env().system.groups(rq.user());
        if let Some(r) = rq
            .env()
            .db
            .requests(rq.user(), &groups)
            .into_iter()
            .find(|r| r.id == id && r.typ == ReqType::CopyFrom)
        {
            match qdx::from_text(&r.body) {
                Ok(qdx::Request::Download { download, .. }) => req = Some(download),
                Ok(_) => messages.push(rq.message(
                    MsgKind::Error,
                    msgs::NOT_A_REQUEST,
                    &[&r.name, "it copies to a table"],
                )),
                Err(e) => messages.push(rq.message(
                    MsgKind::Error,
                    msgs::NOT_A_REQUEST,
                    &[&r.name, &e.to_string()],
                )),
            }
        }
    }
    let all = user_schemas(rq);
    let schemas: Vec<String> = all
        .into_iter()
        .filter(|s| !tables_of(rq, s).is_empty())
        .collect();
    let (mut s, mut t, mut query_text) = (
        f.get("s")
            .map_or_else(|| p.get(prefs::SCHEMA), super::name_of),
        f.get("t").map(super::name_of).unwrap_or_default(),
        f.str("q").trim().to_string(),
    );
    let mut fmt = f
        .get("fmt")
        .and_then(Format::of)
        .filter(|x| Format::TRANSFER.contains(x))
        .unwrap_or(Format::Csv);
    let mut header = if rq.post() { f.has("hdr") } else { true };
    if let Some(d) = &req {
        match &d.source {
            qdx::Source::Table { schema, table, .. } => (s, t) = (schema.clone(), table.clone()),
            qdx::Source::Query { sql } => query_text.clone_from(sql),
        }
        fmt = d.options.format.file();
        header = d.options.header;
    }
    if !schemas.contains(&s) {
        s = schemas.first().cloned().unwrap_or_default();
    }
    let tables = tables_of(rq, &s);
    if !tables.contains(&t) {
        t = tables.first().cloned().unwrap_or_default();
    }
    let job_fmt = p.get(prefs::DATE);
    let dts = [
        String::from("ISO (YYYY-MM-DD)"),
        format!("Job format ({job_fmt})"),
    ];
    let dt = f
        .get("dt")
        .filter(|d| dts.iter().any(|x| x == d))
        .unwrap_or(&dts[0])
        .to_string();
    let cp = f
        .get("cp")
        .filter(|c| FROM_PAGES.contains(c))
        .unwrap_or(FROM_PAGES[0])
        .to_string();
    let les = ["CR LF (Windows)", "LF"];
    let le_default = if p.get(prefs::ENDINGS).starts_with("LF") {
        les[1]
    } else {
        les[0]
    };
    let le = f
        .get("le")
        .filter(|l| les.contains(l))
        .unwrap_or(le_default)
        .to_string();
    if rq.post() && (f.has("copy") || f.has("save")) {
        if !query_text.is_empty() && !sqltext::reads_only(&query_text) {
            messages.push(rq.message(
                MsgKind::Error,
                msgs::BAD_VALUE,
                &[&query_text, "Or a query"],
            ));
        } else if !query_text.is_empty()
            && let Err(r) = rq.policy(policy::DB_SELECT, &[GROUP, TITLE])
        {
            return r;
        } else if t.is_empty() && query_text.is_empty() {
            messages.push(rq.message(MsgKind::Error, msgs::BAD_VALUE, &["", "Schema / table"]));
        } else {
            let file = format!(
                "{}.{}",
                if query_text.is_empty() {
                    t.to_ascii_lowercase()
                } else {
                    "query".into()
                },
                fmt.extension()
            );
            if f.has("save") {
                let source = if query_text.is_empty() {
                    qdx::Source::Table {
                        schema: s.clone(),
                        table: t.clone(),
                        where_clause: String::new(),
                    }
                } else {
                    qdx::Source::Query {
                        sql: query_text.clone(),
                    }
                };
                let body = qdx::to_text(&qdx::Request::Download {
                    system: rq.env().system.name(),
                    download: qdx::Download {
                        source,
                        columns: Vec::new(),
                        limit: None,
                        file: file.clone(),
                        options: qdx::FileOptions {
                            format: qdx::Format::of(fmt).unwrap_or_default(),
                            header,
                            dates: if dt == dts[0] {
                                qdx::Dates::Iso
                            } else {
                                qdx::Dates::Job
                            },
                            ..qdx::FileOptions::default()
                        },
                    },
                });
                let from = if query_text.is_empty() {
                    format!("{s}.{t}")
                } else {
                    "query".into()
                };
                return super::requests::keep(
                    rq,
                    &Request {
                        name: format!("{from} -> {file}"),
                        typ: ReqType::CopyFrom,
                        body,
                        format: fmt.label().into(),
                        ..Request::default()
                    },
                );
            }
            let (name, sql) = if query_text.is_empty() {
                let cols: Vec<String> = rq
                    .env()
                    .db
                    .columns(rq.user(), &s, &t)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| c.name)
                    .collect();
                let order = keys(&rq.env().db.columns(rq.user(), &s, &t).unwrap_or_default());
                (
                    format!("{s}.{t} -> {file}"),
                    if cols.is_empty() {
                        format!("SELECT * FROM {}", sqltext::table(&s, &t))
                    } else {
                        sqltext::all(&s, &t, &cols, &order, None)
                    },
                )
            } else {
                (format!("query -> {file}"), query_text.clone())
            };
            let d = Down {
                name: name.clone(),
                sql,
                file,
                fmt,
                write: files::Write {
                    header,
                    crlf: le == les[0],
                    encoding: Encoding::of(&cp).unwrap_or_default(),
                    iso_dates: dt == dts[0],
                    title: t.clone(),
                },
            };
            let job = new_job(rq, name, false, 0);
            let env = rq.env().clone();
            let user = rq.user().to_string();
            let j = job.clone();
            std::thread::spawn(move || {
                let ms = download(&env, &user, &d, &j);
                *j.done.lock().expect("job") = Some(ms);
            });
            return rq.redirect(&format!("/db/copy/from?job={}", job.id));
        }
    }
    if rq.post() && f.has("cancel") {
        return rq.redirect("/db/tables");
    }
    let formats: Vec<String> = Format::TRANSFER
        .iter()
        .map(|x| x.label().to_string())
        .collect();
    let mut inner = String::from(
        "<table class=\"props\" cellspacing=\"0\">\n<tr class=\"subrow\"><td colspan=\"2\">Source</td></tr>\n",
    );
    let _ = writeln!(
        inner,
        "<tr><th>Schema / table</th><td>{} / {}</td></tr>",
        pick("s", &schemas, &s),
        pick("t", &tables, &t)
    );
    let _ = writeln!(
        inner,
        "<tr><th>Or a query</th><td><textarea name=\"q\" rows=\"3\" cols=\"80\" class=\"conf\">{}</textarea></td></tr>",
        esc(&query_text)
    );
    inner.push_str("<tr class=\"subrow\"><td colspan=\"2\">File on this computer</td></tr>\n");
    let _ = writeln!(
        inner,
        "<tr><th>Format</th><td>{}</td></tr>",
        pick("fmt", &formats, fmt.label())
    );
    let _ = writeln!(
        inner,
        "<tr><th>Column names in the first row</th><td><input type=\"checkbox\" name=\"hdr\"{}></td></tr>",
        if header { " checked" } else { "" }
    );
    inner.push_str("<tr><th>Decimal</th><td><select name=\"dec\"><option>As the column (exact, never through floating point)</option></select></td></tr>\n");
    let _ = writeln!(
        inner,
        "<tr><th>Date / time</th><td>{}</td></tr>",
        pick("dt", &dts, &dt)
    );
    let _ = writeln!(
        inner,
        "<tr><th>Code page</th><td>{}</td></tr>",
        picks("cp", FROM_PAGES, &cp)
    );
    let _ = writeln!(
        inner,
        "<tr><th>Line endings</th><td>{}</td></tr>",
        picks("le", &les, &le)
    );
    inner.push_str("</table>");
    let mut bs = vec![Button::new("copy", "Copy (download)")];
    if rq.allowed(policy::DB_SAVE) {
        bs.push(Button::new("save", "Save request (.qdx)"));
    }
    bs.push(Button::new("cancel", "Cancel"));
    inner.push_str(&buttons(&bs));
    let body = format!(
        "{}<p class=\"small grey\">Runs in a connection of its own with a progress page (rows and time, Cancel) and ends with QAD7030 complete, QAD7031 failed (with the row to resume from), QAD7032 cancelled.</p>",
        form(&rq.ctx, "", &inner)
    );
    rq.page(Page {
        title: TITLE.into(),
        crumb: crumb(&[TITLE]),
        h1: TITLE.into(),
        messages,
        body,
        help: "db-copy".into(),
        nav: Some((GROUP, Some("/db/tables"))),
        ..Page::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_follows_names_then_types() {
        let cols = crate::fixture::db::assets_columns();
        let t = targets(&cols);
        let names: Vec<String> = [
            "tag", "site", "vendor", "model", "sn", "kind", "state", "bought", "price", "comment",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let sample: Vec<Val> = [
            "QS000404",
            "QS-BR1-R1",
            "ARISTA",
            "7050SX-64",
            "JPE14210034",
            "SWITCH",
            "SPARE",
            "2023-05-02",
            "1200.00",
            "Branch spare",
        ]
        .iter()
        .map(|s| Val::Text((*s).into()))
        .collect();
        let m = guess(&names, &sample, &t, "MM/DD/YY");
        assert_eq!(m[8].as_deref(), Some("COST"));
        assert_eq!(m[9].as_deref(), Some("NOTES"));
        assert!(
            fits(&cols[8], &Val::Num("123456789.12".into()))
                && !fits(&cols[8], &Val::Num("1234567890.1".into()))
        );
        assert!(!fits(&cols[0], &Val::Text("QS0000000001".into())));
        let by_name = guess(
            &["STATUS".into()],
            &[Val::Text("SPARE".into())],
            &t,
            "MM/DD/YY",
        );
        assert_eq!(by_name[0].as_deref(), Some("STATUS"));
    }
}
