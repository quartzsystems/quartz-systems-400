// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Insert record (`/db/<schema>/<table>/insert`) and Edit record
//! (`/edit?key=`): a form made from the catalog, one field per column,
//! its type in grey, required columns marked, a NULL box for nullable
//! columns; dates as the user's preferences write them. Values a
//! column's default makes (`CURRENT TIMESTAMP`, identities) are left to
//! the system. A long text field left empty is NULL when the column
//! allows it.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::{Form, Response};
use qsweb::msg::{Kind as MsgKind, Message};
use qsweb::partials::{Button, buttons, form};

use super::view::{columns, key_values, keys};
use super::{GROUP, crumb, failure, name_of, pick, with_conn};
use crate::data::sqltext;
use crate::env::db::{Column, Val};
use crate::routes::Rq;
use crate::{policy, prefs, util};

/// Text columns longer than this get a text area.
const LONG: u32 = 100;

/// The width of a column's field.
fn size(c: &Column) -> u32 {
    match c.typ.as_str() {
        "DATE" => 10,
        "TIME" => 8,
        "TIMESTAMP" => 26,
        "DECIMAL" | "NUMERIC" => c.length + 1,
        "SMALLINT" | "INTEGER" | "INT" | "BIGINT" => c.length * 2,
        _ => c.length.clamp(1, 60),
    }
}

fn long(c: &Column) -> bool {
    matches!(c.typ.as_str(), "VARCHAR" | "VARGRAPHIC" | "CLOB" | "DBCLOB") && c.length > LONG
}

/// An ISO date as the user's date format writes it.
pub fn from_iso(iso: &str, fmt: &str) -> String {
    let b = iso.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return iso.to_string();
    }
    let (y, m, d) = (&iso[..4], &iso[5..7], &iso[8..10]);
    match fmt {
        "MM/DD/YY" => format!("{m}/{d}/{}", &y[2..]),
        "DD/MM/YY" => format!("{d}/{m}/{}", &y[2..]),
        "DD.MM.YYYY" => format!("{d}.{m}.{y}"),
        _ => iso.to_string(),
    }
}

/// A date typed in the user's format as ISO (two-digit years 1940 to
/// 2039); what is not one is left for the database to refuse.
pub fn to_iso(s: &str, fmt: &str) -> String {
    let s = s.trim();
    let parts: Vec<&str> = s.split(['/', '.', '-']).collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
    {
        return s.to_string();
    }
    let year = |y: &str| -> String {
        if y.len() == 2 {
            let n: u32 = y.parse().unwrap_or(0);
            format!("{}", if n < 40 { 2000 + n } else { 1900 + n })
        } else {
            y.to_string()
        }
    };
    let (y, m, d) = match fmt {
        "MM/DD/YY" if parts[0].len() <= 2 => (year(parts[2]), parts[0], parts[1]),
        "DD/MM/YY" | "DD.MM.YYYY" if parts[0].len() <= 2 => (year(parts[2]), parts[1], parts[0]),
        _ => (year(parts[0]), parts[1], parts[2]),
    };
    format!("{y:0>4}-{m:0>2}-{d:0>2}")
}

/// A value as the field shows it.
fn shown(c: &Column, v: &Val, date_fmt: &str) -> String {
    match v {
        Val::Null => String::new(),
        v if c.typ == "DATE" => from_iso(&v.text(), date_fmt),
        v => v.text().trim_end().to_string(),
    }
}

/// The form's rows: `values` the fields' text and NULL boxes.
fn fields(cols: &[Column], values: &[(String, bool)], edit: bool) -> String {
    let mut s = String::from("<table class=\"props\" cellspacing=\"0\">\n");
    for (i, (c, (v, null))) in cols.iter().zip(values).enumerate() {
        let n = i + 1;
        let req = if c.required() {
            " <span class=\"req\">*</span>"
        } else {
            ""
        };
        let _ = write!(
            s,
            "<tr><th>{}{req} <span class=\"grey\">{}</span></th><td>",
            esc(&c.name),
            esc(&c.type_label())
        );
        if c.generated() {
            let _ = write!(
                s,
                "<span class=\"grey\">default: {}</span>",
                esc(&c.default)
            );
        } else if edit && c.key {
            let _ = write!(s, "<span class=\"mono\">{}</span>", esc(v));
        } else if !c.choices.is_empty() {
            let mut opts = c.choices.clone();
            if !v.is_empty() && !opts.contains(v) {
                opts.insert(0, v.clone());
            }
            s.push_str(&pick(&format!("c{n}"), &opts, v));
        } else if long(c) {
            let _ = write!(
                s,
                "<textarea name=\"c{n}\" rows=\"3\" cols=\"60\">{}</textarea>",
                esc(v)
            );
        } else {
            let class = if c.key { " class=\"cmd\"" } else { "" };
            let _ = write!(
                s,
                "<input type=\"text\" name=\"c{n}\" value=\"{}\" size=\"{}\"{class}>",
                esc(v),
                size(c)
            );
            if c.nullable {
                let _ = write!(
                    s,
                    " <input type=\"checkbox\" name=\"n{n}\"{}> NULL",
                    if *null { " checked" } else { "" }
                );
            }
        }
        s.push_str("</td></tr>\n");
    }
    s.push_str("</table>\n");
    s
}

/// The next value after the highest of a key ending in digits
/// (`QS000403` gives `QS000404`).
fn next_key(max: &str) -> Option<String> {
    let max = max.trim_end();
    let digits = max.chars().rev().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let (head, tail) = max.split_at(max.len() - digits);
    let n: u128 = tail.parse().ok()?;
    let next = format!("{:0>width$}", n + 1, width = digits);
    (next.len() == digits).then(|| format!("{head}{next}"))
}

/// What a new record's fields start as: the next key, today's date,
/// NULL for numbers and dates nobody gave.
fn fresh(rq: &Rq<'_>, s: &str, t: &str, cols: &[Column]) -> Vec<(String, bool)> {
    let p = rq.prefs();
    let date_fmt = p.get(prefs::DATE);
    let now = rq.env().system.now();
    let local = i64::try_from(now).unwrap_or(0) + rq.env().system.offset(now);
    let (y, m, d) = util::civil(local.div_euclid(86_400));
    let today = from_iso(&format!("{y:04}-{m:02}-{d:02}"), &date_fmt);
    let key: Vec<&Column> = cols.iter().filter(|c| c.key).collect();
    let suggest = match key.as_slice() {
        [k] if matches!(k.typ.as_str(), "CHAR" | "VARCHAR") => with_conn(rq, |c| {
            let o = rq
                .env()
                .db
                .run(rq.user(), c, &sqltext::max(s, t, &k.name), &[], 1);
            o.rows
                .first()
                .and_then(|r| r.first())
                .and_then(|v| next_key(&v.text()))
        })
        .ok()
        .flatten(),
        _ => None,
    };
    cols.iter()
        .map(|c| {
            if c.key
                && let Some(k) = &suggest
            {
                return (k.clone(), false);
            }
            if c.typ == "DATE" {
                return (today.clone(), false);
            }
            let text = matches!(
                c.typ.as_str(),
                "CHAR" | "VARCHAR" | "GRAPHIC" | "VARGRAPHIC" | "CLOB"
            );
            (String::new(), c.nullable && !text)
        })
        .collect()
}

/// The posted fields as the form shows them again.
fn posted(f: &Form, cols: &[Column]) -> Vec<(String, bool)> {
    (1..=cols.len())
        .map(|n| (f.str(&format!("c{n}")).to_string(), f.has(&format!("n{n}"))))
        .collect()
}

/// The values to write: the columns the form asks for and theirs.
fn values(f: &Form, cols: &[Column], date_fmt: &str, skip_keys: bool) -> (Vec<String>, Vec<Val>) {
    let mut names = Vec::new();
    let mut vals = Vec::new();
    for (i, c) in cols.iter().enumerate() {
        let n = i + 1;
        if c.generated() || (skip_keys && c.key) {
            continue;
        }
        let v = f.str(&format!("c{n}"));
        let val = if c.nullable
            && (f.has(&format!("n{n}"))
                || (long(c) && v.is_empty())
                || (v.is_empty() && !matches!(c.typ.as_str(), "CHAR" | "VARCHAR" | "CLOB")))
        {
            Val::Null
        } else if c.typ == "DATE" {
            Val::Text(to_iso(v, date_fmt))
        } else if c.numeric() {
            Val::Num(v.trim().to_string())
        } else {
            Val::Text(v.to_string())
        };
        names.push(c.name.clone());
        vals.push(val);
    }
    (names, vals)
}

fn show(
    rq: &Rq<'_>,
    s: &str,
    t: &str,
    cols: &[Column],
    v: &[(String, bool)],
    edit: bool,
    messages: Vec<Message>,
) -> Response {
    let fmt = rq.prefs().get(prefs::DATE);
    let what = if edit { "Edit record" } else { "Insert record" };
    let bs = if edit {
        vec![
            Button::new("update", "Update"),
            Button::new("cancel", "Cancel"),
        ]
    } else {
        vec![
            Button::new("insert", "Insert"),
            Button::new("insert_and_add_another", "Insert and add another"),
            Button::new("cancel", "Cancel"),
        ]
    };
    let body = format!(
        "<p class=\"small grey\">One field per column. Required columns are marked. Dates are entered in the format set in your preferences ({}).</p>{}",
        esc(&fmt),
        form(
            &rq.ctx,
            "",
            &format!("{}{}", fields(cols, v, edit), buttons(&bs))
        )
    );
    rq.page(Page {
        title: format!("{what}: {s}.{t}"),
        crumb: crumb(&["Tables", t, what]),
        h1: format!("{what}: {s}.{t}"),
        messages,
        body,
        help: "db-record".into(),
        ..Page::default()
    })
}

/// `GET /db/<schema>/<table>/insert`; `POST` inserts.
pub fn insert(rq: &Rq<'_>, s: &str, t: &str) -> Response {
    let (s, t) = (name_of(s), name_of(t));
    if let Err(r) = rq.policy(policy::DB_WRITE, &[GROUP, "Tables", &t, "Insert record"]) {
        return r;
    }
    let cols = match columns(rq, &s, &t) {
        Ok(c) => c,
        Err(r) => return r,
    };
    if !rq.post() {
        let v = fresh(rq, &s, &t, &cols);
        return show(rq, &s, &t, &cols, &v, false, Vec::new());
    }
    let f = rq.form();
    let back = format!("/db/{s}/{t}");
    if f.has("cancel") {
        return rq.redirect(&back);
    }
    let (names, vals) = values(&f, &cols, &rq.prefs().get(prefs::DATE), false);
    let r = with_conn(rq, |c| {
        rq.env()
            .db
            .run(rq.user(), c, &sqltext::insert(&s, &t, &names), &[vals], 0)
    });
    match r.map(|o| failure(&o).map_or(Ok(o.changed), Err)) {
        Ok(Ok(n)) => {
            let m = rq.message(MsgKind::Done, "SQL7956", &[&t, &s, &n.to_string()]);
            if f.has("insert_and_add_another") {
                rq.done(&format!("{back}/insert"), m)
            } else {
                rq.done(&back, m)
            }
        }
        Ok(Err(m)) | Err(m) => show(rq, &s, &t, &cols, &posted(&f, &cols), false, vec![m]),
    }
}

/// `GET /db/<schema>/<table>/edit?key=`; `POST` updates the record.
pub fn edit(rq: &Rq<'_>, s: &str, t: &str) -> Response {
    let (s, t) = (name_of(s), name_of(t));
    if let Err(r) = rq.policy(policy::DB_WRITE, &[GROUP, "Tables", &t, "Edit record"]) {
        return r;
    }
    let cols = match columns(rq, &s, &t) {
        Ok(c) => c,
        Err(r) => return r,
    };
    let keys = keys(&cols);
    let Some(kv) = key_values(rq, &keys) else {
        return rq.not_found(&format!("record of {s}.{t} with that key"));
    };
    let back = format!("/db/{s}/{t}");
    let fmt = rq.prefs().get(prefs::DATE);
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect(&back);
        }
        let (names, mut vals) = values(&f, &cols, &fmt, true);
        vals.extend(kv.iter().cloned());
        let r = with_conn(rq, |c| {
            rq.env().db.run(
                rq.user(),
                c,
                &sqltext::update(&s, &t, &names, &keys),
                &[vals],
                0,
            )
        });
        return match r.map(|o| failure(&o).map_or(Ok(o.changed), Err)) {
            Ok(Ok(0)) => rq.done(&back, rq.message(MsgKind::Info, "SQL0100", &["UPDATE"])),
            Ok(Ok(n)) => rq.done(
                &back,
                rq.message(MsgKind::Done, "SQL7957", &[&t, &s, &n.to_string()]),
            ),
            Ok(Err(m)) | Err(m) => {
                let mut v = posted(&f, &cols);
                for (i, c) in cols.iter().enumerate() {
                    if c.key {
                        v[i].0 = kv[keys.iter().position(|k| *k == c.name).unwrap_or(0)].text();
                    }
                }
                show(rq, &s, &t, &cols, &v, true, vec![m])
            }
        };
    }
    let names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
    let found = with_conn(rq, |c| {
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
    let v: Vec<(String, bool)> = cols
        .iter()
        .zip(&row)
        .map(|(c, v)| (shown(c, v, &fmt), v.is_null()))
        .collect();
    show(rq, &s, &t, &cols, &v, true, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_keys() {
        assert_eq!(from_iso("2026-10-29", "MM/DD/YY"), "10/29/26");
        assert_eq!(to_iso("10/29/26", "MM/DD/YY"), "2026-10-29");
        assert_eq!(to_iso("29.10.1999", "DD.MM.YYYY"), "1999-10-29");
        assert_eq!(to_iso("2026-10-29", "MM/DD/YY"), "2026-10-29");
        assert_eq!(to_iso("01/02/45", "DD/MM/YY"), "1945-02-01");
        assert_eq!(next_key("QS000403  "), Some("QS000404".into()));
        assert_eq!(next_key("Q99"), None);
        assert_eq!(next_key("ABC"), None);
    }
}
