// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System: `/sysval[/<name>]`, `/disk[/<unit>]`, `/hdwrsc[/<name>/...]`,
//! `/time`, `/power`. Reads are what `WRKSYSVAL`, `WRKDSKSTS` and `WRKHDWRSC`
//! show; every change is the command itself run as the user
//! ([`crate::env::config::Config::run`]).

mod disk;
mod hdwrsc;
mod power;
mod sysval;
mod time;

use qsweb::http::Response;

use super::Rq;

/// The tree's group.
const GROUP: &str = "System";

/// Route `segs` (the path's segments, the area's first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["sysval"] => sysval::list(rq),
        ["sysval", name] => sysval::change(rq, name),
        ["disk"] => disk::list(rq),
        ["disk", unit] => disk::props(rq, unit),
        ["hdwrsc"] => hdwrsc::list(rq),
        ["hdwrsc", name, rest @ ..] => hdwrsc::route(rq, name, rest),
        ["time"] => time::page(rq),
        ["power"] => power::page(rq),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// A page of the System group.
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

/// A bar for `pct` percent, never narrower than 1 %.
fn bar(pct: u32) -> String {
    super::home::bar(pct.max(1))
}

/// Bytes as gigabytes to a tenth, without the unit (`1,862.4`).
fn gb(bytes: u64) -> String {
    crate::util::gb(bytes).trim_end_matches(" GB").to_string()
}
