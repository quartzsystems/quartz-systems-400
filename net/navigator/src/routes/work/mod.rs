// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management: `/jobs`, `/job/<number>/<user>/<name>/...`, `/jobq`,
//! `/sbs`, `/outq`, `/splf`, `/writers`, `/jobscde`. Lists read what the
//! `WRK*` commands show; every change is the command itself run as the
//! user ([`crate::env::work::Work::run`]), reached through a page that
//! names it and a POST.

mod job;
mod jobq;
mod jobs;
mod jobscde;
mod outq;
mod sbs;
mod writers;

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::pages::confirm_body;
use qsweb::partials::{Cell, Col, List, list_table};

use super::Rq;
use crate::env::{Need, Target};

/// The tree's group.
const GROUP: &str = "Work management";

/// Route `segs` (the path's segments, the area's first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["jobs"] => jobs::list(rq),
        ["job", n, u, j, rest @ ..] => job::route(rq, &format!("{n}/{u}/{j}"), rest),
        ["jobq", rest @ ..] => jobq::route(rq, rest),
        ["sbs", rest @ ..] => sbs::route(rq, rest),
        ["outq", rest @ ..] => outq::route(rq, rest),
        ["splf", n, u, j, file, nbr, rest @ ..] => {
            outq::splf(rq, &format!("{n}/{u}/{j}"), file, nbr, rest)
        }
        ["writers", rest @ ..] => writers::route(rq, rest),
        ["jobscde", rest @ ..] => jobscde::route(rq, rest),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// A select of `options` with `chosen` selected; the first option is
/// the default and is not marked.
pub fn pick(name: &str, options: &[String], chosen: &str) -> String {
    let mut s = format!("<select name=\"{}\">", esc(name));
    for (i, o) in options.iter().enumerate() {
        let sel = if i > 0 && o == chosen {
            " selected"
        } else {
            ""
        };
        let _ = write!(s, "<option{sel}>{}</option>", esc(o));
    }
    s.push_str("</select>");
    s
}

/// `opts` as owned strings.
pub fn strings(opts: &[&str]) -> Vec<String> {
    opts.iter().map(ToString::to_string).collect()
}

/// Whether `value` matches a name filter: blank or `*ALL` matches all,
/// `ABC*` is generic.
pub fn matches(pattern: &str, value: &str) -> bool {
    let p = pattern.trim().to_ascii_uppercase();
    if p.is_empty() || p == "*ALL" {
        return true;
    }
    match p.strip_suffix('*') {
        Some(prefix) => value.starts_with(prefix),
        None => value == p,
    }
}

/// A filter field's value, `*ALL` when blank.
pub fn all_or(v: &str) -> String {
    let v = v.trim().to_ascii_uppercase();
    if v.is_empty() { "*ALL".into() } else { v }
}

/// The `(name, value)` pairs links keep: those not `*ALL` or blank.
pub fn keep(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .filter(|(_, v)| !v.is_empty() && *v != "*ALL")
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

/// A status word: green, red or black.
pub fn word(w: &str, good: Option<bool>) -> String {
    qsweb::partials::status(w, good)
}

/// A job's route: `/job/<number>/<user>/<name>`.
pub fn job_route(qualified: &str) -> String {
    format!("/job/{qualified}")
}

/// `*NOMAX` or the number.
pub fn nomax(v: Option<u32>) -> String {
    v.map_or("*NOMAX".into(), |n| n.to_string())
}

/// A page of the work management group; `item` is the tree's current
/// item for a list page, `None` for pages below one.
pub fn page(
    rq: &Rq<'_>,
    crumb: &[&str],
    h1: &str,
    desc: Option<&str>,
    body: String,
    help: &str,
    item: Option<&'static str>,
) -> Response {
    page_in(rq, GROUP, crumb, h1, desc, body, help, item)
}

/// A page of tree group `group`; `item` is the tree's current item
/// for a list page, `None` for pages below one.
#[allow(clippy::too_many_arguments)]
pub fn page_in(
    rq: &Rq<'_>,
    group: &'static str,
    crumb: &[&str],
    h1: &str,
    desc: Option<&str>,
    body: String,
    help: &str,
    item: Option<&'static str>,
) -> Response {
    let mut c = vec![group.to_string()];
    c.extend(crumb.iter().map(ToString::to_string));
    rq.page(Page {
        title: h1.into(),
        crumb: c,
        h1: h1.into(),
        desc: desc.map(ToString::to_string),
        body,
        help: help.into(),
        nav: item.map(|i| (group, Some(i))),
        ..Page::default()
    })
}

/// A confirmation of a command: what it acts on and the command itself.
pub struct Confirm<'a> {
    /// `Hold Job`: the title is `Confirm Hold Job`.
    pub what: &'a str,
    /// The breadcrumb before the title.
    pub crumb: &'a [&'a str],
    /// The sentence.
    pub sentence: String,
    /// The affected objects' table.
    pub cols: &'a [&'a str],
    /// Its rows, HTML cells.
    pub rows: Vec<Vec<String>>,
    /// Options inside the form, HTML.
    pub options: String,
    /// The command OK runs, shown.
    pub command: String,
}

/// The confirmation page.
pub fn confirm_page(rq: &Rq<'_>, c: &Confirm<'_>) -> Response {
    confirm_in(rq, GROUP, c)
}

/// The confirmation page of a command in tree group `group`.
pub fn confirm_in(rq: &Rq<'_>, group: &'static str, c: &Confirm<'_>) -> Response {
    let list = List {
        cols: c.cols.iter().map(|l| Col::plain(l)).collect(),
        rows: c
            .rows
            .iter()
            .map(|r| r.iter().map(|h| Cell::html(h.clone())).collect())
            .collect(),
        empty: String::new(),
        keep: Vec::new(),
    };
    let options = format!(
        "{}<p class=\"small grey\">Equivalent command: <span class=\"mono\">{}</span></p>",
        c.options,
        esc(&c.command)
    );
    let body = confirm_body(&rq.ctx, &c.sentence, &list_table(&list), &options);
    let title = format!("Confirm {}", c.what);
    let mut crumb: Vec<&str> = c.crumb.to_vec();
    crumb.push(&title);
    page_in(rq, group, &crumb, &title, None, body, "confirm", None)
}

/// `<span class="mono">` of `s`.
pub fn mono(s: &str) -> String {
    qsweb::partials::mono(s)
}

/// Check `need` unless `own` (the user's own object).
pub fn need(
    rq: &Rq<'_>,
    own: bool,
    need: Need,
    function: &str,
    target: &Target,
    crumb: &[&str],
) -> Result<(), Response> {
    if own {
        return Ok(());
    }
    rq.require(need, function, target, crumb)
}

/// Run `command` as the user, then `303` to `back` with its message.
pub fn run(rq: &Rq<'_>, command: &str, back: &str) -> Response {
    reply(rq, rq.env().work.run(rq.user(), command), back)
}

/// `303` to `back` with a command's message, completion or failure.
pub fn reply(rq: &Rq<'_>, r: Result<Message, Message>, back: &str) -> Response {
    let m = match r {
        Ok(m) | Err(m) => m,
    };
    rq.done(back, m)
}

/// The confirmation's POST: OK runs `command`, Cancel goes `back`.
pub fn confirmed(rq: &Rq<'_>, command: &str, back: &str) -> Response {
    if rq.form().has("cancel") {
        return rq.redirect(back);
    }
    run(rq, command, back)
}

/// A CL string literal.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Whether the session's waiting message is an error (it is shown by
/// [`Rq::page`]).
pub fn error_waiting(rq: &Rq<'_>) -> bool {
    rq.nav
        .sessions
        .get(&rq.session.id)
        .and_then(|s| s.flash.clone())
        .is_some_and(|m| m.kind == Kind::Error)
}
