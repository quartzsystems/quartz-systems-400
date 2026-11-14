// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server, in the Network group: `/dirsrv` (the
//! instances: Start, End, Restart), `/dirsrv/publish` (the publishing
//! agents) and below `/dirsrv/inst/<name>/` an instance's `props`
//! (server properties and the administrator), `suffixes`, `indexes`,
//! `schema`, `access` and `monitor`. Every read and change goes through
//! [`crate::env::dirsrv::DirAdmin`] as the user, which checks the
//! authority (`*IOSYSCFG` to read, `*ALLOBJ` and `*IOSYSCFG` to change,
//! as `CHGDIRSVRA`) and validates; Navigator checks `*IOSYSCFG` first
//! before a change.

mod access;
mod instances;
mod props;
mod publish;
mod schema;
mod suffixes;

use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::tabs;

use super::Rq;
use crate::env::dirsrv::{AdminError, ErrorKind, InstanceInfo, message};
use crate::env::{Need, Target};

/// The tree's group.
const GROUP: &str = "Network";

/// The tree's item.
const ITEM: &str = "/dirsrv";

/// Route `segs` (the path's segments, `dirsrv` first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["dirsrv"] => instances::list(rq),
        ["dirsrv", "publish", rest @ ..] => publish::route(rq, rest),
        ["dirsrv", "inst", name, rest @ ..] => {
            let inst = match instance(rq, name) {
                Ok(i) => i,
                Err(r) => return r,
            };
            match rest {
                ["confirm"] => instances::act(rq, &inst),
                ["monitor"] => instances::monitor(rq, &inst),
                ["props"] => props::route(rq, &inst),
                ["admin"] => props::admin(rq, &inst),
                ["suffixes", more @ ..] => suffixes::route(rq, &inst, more),
                ["indexes", more @ ..] => suffixes::indexes(rq, &inst, more),
                ["schema", more @ ..] => schema::route(rq, &inst, more),
                ["access"] => access::route(rq, &inst),
                _ => rq.not_found(&segs.join("/")),
            }
        }
        _ => rq.not_found(&segs.join("/")),
    }
}

/// The instance `name`, or the page that says why not.
fn instance(rq: &Rq<'_>, name: &str) -> Result<InstanceInfo, Response> {
    match rq.env().dirsrv.instances(rq.user()) {
        Ok(l) => l
            .into_iter()
            .find(|i| i.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| rq.not_found(&format!("directory server instance {name}"))),
        Err(e) => Err(fail(rq, &e, "Directory server", &[], "/home")),
    }
}

/// A page of the Network group, below Directory server.
fn page(rq: &Rq<'_>, crumb: &[&str], h1: &str, body: String, help: &str, item: bool) -> Response {
    let mut c = vec!["Directory server"];
    c.extend_from_slice(crumb);
    super::common::group_page(rq, GROUP, &c, h1, None, body, help, item.then_some(ITEM))
}

/// The top tabs: Instances, Publishing.
fn top_tabs(rq: &Rq<'_>, current: usize) -> String {
    tabs(
        &[
            ("Instances", rq.ctx.url("/dirsrv")),
            ("Publishing", rq.ctx.url("/dirsrv/publish")),
        ],
        current,
    )
}

/// An instance's route `rest`.
fn inst_url(rq: &Rq<'_>, i: &InstanceInfo, rest: &str) -> String {
    rq.ctx.url(&inst_route(i, rest))
}

/// An instance's route below the prefix.
fn inst_route(i: &InstanceInfo, rest: &str) -> String {
    format!("/dirsrv/inst/{}/{rest}", i.name)
}

/// An instance's tabs.
fn inst_tabs(rq: &Rq<'_>, i: &InstanceInfo, current: usize) -> String {
    tabs(
        &[
            ("Properties", inst_url(rq, i, "props")),
            ("Suffixes", inst_url(rq, i, "suffixes")),
            ("Indexes", inst_url(rq, i, "indexes")),
            ("Schema", inst_url(rq, i, "schema")),
            ("Access", inst_url(rq, i, "access")),
            ("Monitor", inst_url(rq, i, "monitor")),
        ],
        current,
    )
}

/// What a refused change named.
fn target(i: Option<&InstanceInfo>) -> Target {
    let mut t = Target::obj("QSYS", "CHGDIRSVRA", "*CMD");
    t.shown = i.map_or_else(
        || "Directory server".into(),
        |i| format!("Directory server instance {}", i.name),
    );
    t
}

/// Check `*IOSYSCFG`, which every change needs.
fn need(
    rq: &Rq<'_>,
    i: Option<&InstanceInfo>,
    function: &str,
    crumb: &[&str],
) -> Result<(), Response> {
    let mut c = vec![GROUP, "Directory server"];
    c.extend_from_slice(crumb);
    rq.require(Need::IoSysCfg, function, &target(i), &c)
}

/// A refusal: the Not authorized page when it is about authority,
/// else `back` with its message waiting.
fn fail(rq: &Rq<'_>, e: &AdminError, function: &str, crumb: &[&str], back: &str) -> Response {
    if e.kind == ErrorKind::NotAuthorized {
        let need = match e.data.first().map(String::as_str) {
            Some("*ALLOBJ") => Need::AllObj,
            _ => Need::IoSysCfg,
        };
        let mut c = vec![GROUP, "Directory server"];
        c.extend_from_slice(crumb);
        return rq.refuse(message(e), need, function, &target(None), &c);
    }
    rq.done(back, message(e))
}

/// After a change of instance `i`: `back` with `GLD0123` (the file is
/// changed; the instance reads it at its next start).
fn changed(rq: &Rq<'_>, i: &InstanceInfo, back: &str) -> Response {
    rq.done(back, rq.message(Kind::Done, "GLD0123", &[&i.name]))
}

/// `Yes` or `No`.
fn yes_no(b: bool) -> &'static str {
    if b { "Yes" } else { "No" }
}

/// A time as the user's job shows it; blank for none.
fn when(rq: &Rq<'_>, secs: u64) -> String {
    if secs == 0 {
        String::new()
    } else {
        rq.when(secs)
    }
}

/// A query value, encoded.
fn enc(s: &str) -> String {
    qsweb::http::form::encode(s)
}

/// A confirmation of a change that is no command: what it changes, in
/// `cols` and `rows` (HTML cells).
fn confirm(
    rq: &Rq<'_>,
    what: &str,
    crumb: &[&str],
    sentence: &str,
    cols: &[&str],
    rows: Vec<Vec<String>>,
) -> Response {
    use qsweb::partials::{Cell, Col, List, list_table};
    let list = List {
        cols: cols.iter().map(|l| Col::plain(l)).collect(),
        rows: rows
            .into_iter()
            .map(|r| r.into_iter().map(Cell::html).collect())
            .collect(),
        empty: String::new(),
        keep: Vec::new(),
    };
    let body = qsweb::pages::confirm_body(&rq.ctx, sentence, &list_table(&list), "");
    let title = format!("Confirm {what}");
    let mut c = vec!["Directory server"];
    c.extend_from_slice(crumb);
    c.push(&title);
    super::common::group_page(rq, GROUP, &c, &title, None, body, "confirm", None)
}
