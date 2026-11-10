// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NetServer, in the Network group: `/netsvr` (the server, its
//! attributes current and pending, start and end), `/netsvr/shares`
//! and `/netsvr/prtshr` (file and print shares), `/netsvr/sessions`
//! (sessions and their connections), `/netsvr/disabled` (users
//! disabled for NetServer). Every read is a `QZLSOLST` list and every
//! change the `QZLS*` API a program would call, as the user
//! ([`crate::env::netsvr::NetServer`]): the API checks the authority
//! and validates; Navigator only checks `*IOSYSCFG` first where the API
//! always needs it.

#![allow(clippy::doc_markdown)] // NetServer

mod disabled;
mod server;
mod sessions;
mod shares;

use std::fmt::Write as _;

use qsweb::http::Response;
use qsweb::msg::Message;
use qsweb::partials::tabs;

use super::Rq;
use crate::env::{Need, Target};

/// The tree's group.
const GROUP: &str = "Network";

/// Route `segs` (the path's segments, `netsvr` first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["netsvr"] => server::page_route(rq),
        ["netsvr", "confirm"] => server::end(rq),
        ["netsvr", "shares", rest @ ..] => shares::route(rq, false, rest),
        ["netsvr", "prtshr", rest @ ..] => shares::route(rq, true, rest),
        ["netsvr", "sessions", rest @ ..] => sessions::route(rq, rest),
        ["netsvr", "disabled", rest @ ..] => disabled::route(rq, rest),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// A page of the Network group.
fn page(
    rq: &Rq<'_>,
    crumb: &[&str],
    h1: &str,
    desc: Option<&str>,
    body: String,
    help: &str,
    item: Option<&'static str>,
) -> Response {
    let mut c = vec!["NetServer"];
    c.extend_from_slice(crumb);
    super::common::group_page(rq, GROUP, &c, h1, desc, body, help, item)
}

/// The NetServer tabs, `current` bold.
fn ns_tabs(rq: &Rq<'_>, current: usize) -> String {
    tabs(
        &[
            ("Server", rq.ctx.url("/netsvr")),
            ("File shares", rq.ctx.url("/netsvr/shares")),
            ("Print shares", rq.ctx.url("/netsvr/prtshr")),
            ("Sessions", rq.ctx.url("/netsvr/sessions")),
            ("Disabled users", rq.ctx.url("/netsvr/disabled")),
        ],
        current,
    )
}

/// The target of a refused API call.
fn api_target(api: &str) -> Target {
    let mut t = Target::obj("QSYS", api, "*PGM");
    t.shown = format!("QSYS/{api}");
    t
}

/// Check `*IOSYSCFG`, which `api` always needs.
fn need(rq: &Rq<'_>, function: &str, api: &str, crumb: &str) -> Result<(), Response> {
    rq.require(
        Need::IoSysCfg,
        function,
        &api_target(api),
        &["NetServer", crumb],
    )
}

/// An API's refusal: the Not authorized page when it is about
/// authority (with the `AF` entry), else `None`.
fn refused(rq: &Rq<'_>, m: &Message, function: &str, api: &str, crumb: &str) -> Option<Response> {
    if !matches!(m.id.as_str(), "CPFB684" | "CPDB680") {
        return None;
    }
    Some(rq.refuse(
        m.clone(),
        Need::IoSysCfg,
        function,
        &api_target(api),
        &["NetServer", crumb],
    ))
}

/// After a change: `back` on success, the refusal page, or `back` with
/// the API's message waiting.
fn answer(
    rq: &Rq<'_>,
    r: Result<(), Message>,
    function: &str,
    api: &str,
    crumb: &str,
    back: &str,
) -> Response {
    match r {
        Ok(()) => rq.redirect(back),
        Err(m) => refused(rq, &m, function, api, crumb).unwrap_or_else(|| rq.done(back, m)),
    }
}

/// A parameter of the CL `CALL` that does what a page does.
enum Parm<'a> {
    /// A character value.
    Char(&'a str),
    /// An 8-byte binary.
    Bin8(i64),
}

/// `CALL PGM(QSYS/<api>) PARM(...)`, the error code last (8 bytes of
/// zeros: an error is an escape message), then `after` (optional
/// parameters that follow the error code).
fn call(api: &str, parms: &[Parm<'_>], after: &[Parm<'_>]) -> String {
    let one = |p: &Parm<'_>| match p {
        Parm::Char(s) => super::common::quote(s),
        Parm::Bin8(v) => format!("X'{v:016X}'"),
    };
    let mut s = format!("CALL PGM(QSYS/{api}) PARM(");
    for p in parms {
        let _ = write!(s, "{} ", one(p));
    }
    s.push_str("X'0000000000000000'");
    for p in after {
        let _ = write!(s, " {}", one(p));
    }
    s.push(')');
    s
}

/// `*YES` or `*NO`.
fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

/// A time as the user's job shows it.
fn when(rq: &Rq<'_>, secs: u64) -> String {
    if secs == 0 {
        String::new()
    } else {
        rq.when(secs)
    }
}

/// `h:mm:ss` of a number of seconds.
fn span(secs: u64) -> String {
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_read_as_cl() {
        assert_eq!(
            call("QZLSRMS", &[Parm::Char("DOCS")], &[]),
            "CALL PGM(QSYS/QZLSRMS) PARM('DOCS' X'0000000000000000')"
        );
        assert_eq!(
            call("QZLSENSS", &[Parm::Char("*SESSID")], &[Parm::Bin8(7)]),
            "CALL PGM(QSYS/QZLSENSS) PARM('*SESSID' X'0000000000000000' X'0000000000000007')"
        );
        assert_eq!(span(3725), "1:02:05");
    }
}
