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

use qsweb::http::Response;

pub use super::common::{
    Confirm, all_or, answered as reply, confirm_in, error_waiting, group_page as page_in, keep, matches, mono, pick, quote,
    strings, word,
};
use super::common::{answered, group_page};
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
    group_page(rq, GROUP, crumb, h1, desc, body, help, item)
}

/// The confirmation page.
pub fn confirm_page(rq: &Rq<'_>, c: &Confirm<'_>) -> Response {
    confirm_in(rq, GROUP, c)
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
    answered(rq, rq.env().work.run(rq.user(), command), back)
}

/// The confirmation's POST: OK runs `command`, Cancel goes `back`.
pub fn confirmed(rq: &Rq<'_>, command: &str, back: &str) -> Response {
    if rq.form().has("cancel") {
        return rq.redirect(back);
    }
    run(rq, command, back)
}
