// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the areas' pages share: filter values, selects, the page of a
//! tree group and the confirmation of a command.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::pages::confirm_body;
use qsweb::partials::{Cell, Col, List, list_table};

use super::Rq;

/// A select of `options` with `chosen` selected; the first option is
/// the default and is not marked.
pub fn pick(name: &str, options: &[String], chosen: &str) -> String {
    let mut s = format!("<select name=\"{}\">", esc(name));
    for (i, o) in options.iter().enumerate() {
        let sel = if i > 0 && o == chosen { " selected" } else { "" };
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

/// `<span class="mono">` of `s`.
pub fn mono(s: &str) -> String {
    qsweb::partials::mono(s)
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

/// A page of tree group `group`; `item` is the tree's current item for
/// a list page, `None` for pages below one.
#[allow(clippy::too_many_arguments)]
pub fn group_page(
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

/// The confirmation page in tree group `group`.
pub fn confirm_in(rq: &Rq<'_>, group: &'static str, c: &Confirm<'_>) -> Response {
    let list = List {
        cols: c.cols.iter().map(|l| Col::plain(l)).collect(),
        rows: c.rows.iter().map(|r| r.iter().map(|h| Cell::html(h.clone())).collect()).collect(),
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
    group_page(rq, group, &crumb, &title, None, body, "confirm", None)
}

/// `303` to `back` with a command's answer waiting there.
pub fn answered(rq: &Rq<'_>, answer: Result<Message, Message>, back: &str) -> Response {
    let (Ok(m) | Err(m)) = answer;
    rq.done(back, m)
}
