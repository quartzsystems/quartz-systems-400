// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages: `/msgq/<lib>/<name>[/<key>[/remove]|/clear]`, `/mymsgs`,
//! `/qhst`. The queues read what `DSPMSG` shows, the history log what
//! `DSPLOG` shows; Send, Reply, Remove and Clear are `SNDMSG`,
//! `SNDRPYMSG`, `RMVMSG` and `CLRMSGQ` run as the user
//! ([`crate::env::msgs::Msgs::run`]).

mod detail;
mod qhst;
mod queue;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;

use super::Rq;
use super::common::answered;

/// The tree's group.
const GROUP: &str = "Messages";
/// QSYSOPR's route, the tree's item.
const QSYSOPR: &str = "/msgq/QSYS/QSYSOPR";

/// Route `segs` (the path's segments, the area's first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["mymsgs"] => {
            let (lib, name) = rq.env().msgs.own_queue(rq.user());
            queue::page(rq, &lib, &name, true)
        }
        ["msgq", lib, name] => queue::page(rq, lib, name, false),
        ["msgq", lib, name, "clear"] => queue::clear(rq, lib, name),
        ["msgq", lib, name, key] => detail::page(rq, lib, name, key),
        ["msgq", lib, name, key, "remove"] => queue::remove(rq, lib, name, key),
        ["qhst"] => qhst::page(rq),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// Where a queue's pages return: My messages for links from there
/// (`?from=my`), else the queue's own page.
fn back(rq: &Rq<'_>, lib: &str, name: &str) -> String {
    if rq.query().get("from") == Some("my") {
        "/mymsgs".into()
    } else {
        format!("/msgq/{lib}/{name}")
    }
}

/// `?from=my` when `my`.
fn from(my: bool) -> &'static str {
    if my { "?from=my" } else { "" }
}

/// A page of the Messages group; `nav` is the tree's current item, the
/// group left as the session has it when `None`.
#[allow(clippy::too_many_arguments)]
fn page(
    rq: &Rq<'_>,
    crumb: &[&str],
    title: &str,
    h1: &str,
    desc: Option<&str>,
    body: String,
    help: &str,
    nav: Option<(&'static str, Option<&'static str>)>,
) -> Response {
    let mut c = vec![GROUP.to_string()];
    c.extend(crumb.iter().map(ToString::to_string));
    rq.page(Page {
        title: title.into(),
        crumb: c,
        h1: h1.into(),
        desc: desc.map(ToString::to_string),
        body,
        help: help.into(),
        nav,
        ..Page::default()
    })
}

/// Run `command` as the user, then `303` to `back` with its message.
fn run(rq: &Rq<'_>, command: &str, back: &str) -> Response {
    answered(rq, rq.env().msgs.run(rq.user(), command), back)
}

/// A message ID linking to its text in a new window (`""` for none).
fn idlink(rq: &Rq<'_>, id: &str) -> String {
    if id.is_empty() {
        return String::new();
    }
    format!(
        "<a href=\"{}\" target=\"_blank\">{}</a>",
        esc(&rq.ctx.product.msg(id)),
        esc(id)
    )
}

/// A job's name, linked to its page when the job is still in the
/// system.
fn job_link(rq: &Rq<'_>, job: &str) -> String {
    if !job.is_empty() && rq.env().work.job(rq.user(), job).is_some() {
        format!(
            "<a href=\"{}\">{}</a>",
            esc(&rq.ctx.url(&format!("/job/{job}/general"))),
            esc(job)
        )
    } else {
        esc(job)
    }
}
