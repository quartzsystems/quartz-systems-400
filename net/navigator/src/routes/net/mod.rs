// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Network: `/tcp` (the `CFGTCP` overview and domain), `/tcp/ifc`,
//! `/tcp/rte`, `/tcp/hosts`, `/tcp/servers`. Every change is the command
//! itself run as the user ([`crate::env::net::Net::run`]) after the
//! `*IOSYSCFG` check its command makes.

mod ifc;
mod rte;
mod servers;
mod tcp;

use qsweb::http::Response;
use qsweb::partials::tabs;

use super::Rq;
use crate::env::{Need, Target};
use crate::routes::work::quote;

/// The tree's group.
const GROUP: &str = "Network";

/// Route `segs` (the path's segments, the area's first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["tcp"] => tcp::overview(rq),
        ["tcp", "hosts"] => tcp::hosts(rq),
        ["tcp", "ifc", rest @ ..] => ifc::route(rq, rest),
        ["tcp", "rte", rest @ ..] => rte::route(rq, rest),
        ["tcp", "servers", rest @ ..] => servers::route(rq, rest),
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
    super::work::page_in(rq, GROUP, crumb, h1, desc, body, help, item)
}

/// The `CFGTCP` tabs, `current` bold.
fn cfg_tabs(rq: &Rq<'_>, current: usize) -> String {
    tabs(
        &[
            ("Overview", rq.ctx.url("/tcp")),
            ("Interfaces", rq.ctx.url("/tcp/ifc")),
            ("Routes", rq.ctx.url("/tcp/rte")),
            ("Host table", rq.ctx.url("/tcp/hosts")),
            ("Domain", rq.ctx.url("/tcp")),
        ],
        current,
    )
}

/// Check `*IOSYSCFG`, which every TCP/IP configuration command needs.
fn need(rq: &Rq<'_>, function: &str, cmd: &str, crumb: &str) -> Result<(), Response> {
    let mut t = Target::obj("QSYS", cmd, "*CMD");
    t.shown = cmd.to_string();
    rq.require(Need::IoSysCfg, function, &t, &[GROUP, crumb])
}

/// The confirmation's POST: OK runs `command`, Cancel goes `back`.
fn confirmed(rq: &Rq<'_>, command: &str, back: &str) -> Response {
    if rq.form().has("cancel") {
        return rq.redirect(back);
    }
    super::work::reply(rq, rq.env().net.run(rq.user(), command), back)
}

/// A CL value: special values as they are, the rest quoted.
fn arg(v: &str) -> String {
    if v.starts_with('*') {
        v.to_string()
    } else {
        quote(v)
    }
}

/// A status word: Active green, Inactive red.
fn active(on: bool) -> String {
    if on {
        qsweb::partials::status("Active", Some(true))
    } else {
        qsweb::partials::status("Inactive", Some(false))
    }
}

/// `*YES` or `*NO`.
fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}
