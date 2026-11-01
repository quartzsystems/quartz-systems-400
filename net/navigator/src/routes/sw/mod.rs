// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Software: `/licpgm` (`GO LICPGM` 10, 11, 12 and the license keys),
//! `/ptf` (`DSPPTF`, `LODPTF`, `APYPTF`, `RMVPTF`, cover letters, the
//! PTF level) and `/fixes` (the update service). Every change is the
//! command itself run as the user ([`crate::env::sw::Software::run`]),
//! reached through a confirmation page and a POST.

mod licpgm;
mod ptf;

use qsweb::frame::Page;
use qsweb::http::Response;
use qsweb::msg::Message;

use super::Rq;
use crate::env::{Need, Target};

/// The tree's group.
const GROUP: &str = "Software";

/// Route `segs` (the path's segments, the area's first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["licpgm"] => licpgm::list(rq),
        ["licpgm", "keys"] => licpgm::keys(rq),
        ["licpgm", id, opt] => licpgm::props(rq, id, opt),
        ["licpgm", id, opt, "confirm"] => licpgm::confirm(rq, id, opt),
        ["ptf"] => ptf::list(rq),
        ["ptf", "confirm"] => ptf::confirm(rq),
        ["ptf", "level"] => ptf::level(rq),
        ["ptf" | "fixes", id, "cover"] => ptf::cover(rq, id),
        ["fixes"] => ptf::fixes(rq),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// A page of the Software group with message rows.
#[allow(clippy::too_many_arguments)]
fn page(
    rq: &Rq<'_>,
    crumb: &[&str],
    h1: &str,
    desc: Option<&str>,
    messages: Vec<Message>,
    body: String,
    help: &str,
    item: Option<&'static str>,
) -> Response {
    let mut c = vec![GROUP.to_string()];
    c.extend(crumb.iter().map(ToString::to_string));
    rq.page(Page {
        title: h1.into(),
        crumb: c,
        h1: h1.into(),
        desc: desc.map(ToString::to_string),
        messages,
        body,
        help: help.into(),
        nav: item.map(|i| (GROUP, Some(i))),
        ..Page::default()
    })
}

/// Check `*ALLOBJ` for a change to the system's software.
fn need(rq: &Rq<'_>, function: &str, target: &Target, crumb: &[&str]) -> Result<(), Response> {
    rq.require(Need::AllObj, function, target, crumb)
}

/// Run `commands` in order as the user, stopping at a failure, then
/// `303` to `back` with the last message.
fn run_all(rq: &Rq<'_>, commands: &[String], back: &str) -> Response {
    let sw = &rq.env().sw;
    let mut last = None;
    for c in commands {
        match sw.run(rq.user(), c) {
            Ok(m) => last = Some(m),
            Err(m) => return rq.done(back, m),
        }
    }
    match last {
        Some(m) => rq.done(back, m),
        None => rq.redirect(back),
    }
}
