// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Print (handoff step 15), the desktop's Printer Output:
//! `/print?u=&q=&qn=&s=&ft=&ud=&sort=&page=&rows=&auto=` lists spooled
//! files, its form holds, releases, deletes (`/print/confirm`), moves
//! and saves them; `/print/props/<id>` every attribute;
//! `/print/view/<id>` the viewer (Navigator's spooled file viewer,
//! framed for this product), `/pdf` and `/text`; `/print/printers`.
//! Everything runs as the signed-on user; the policies `print.*` come
//! first.

pub mod act;
pub mod list;
pub mod printers;
pub mod props;
pub mod view;

use qsweb::http::Response;

use super::Rq;
use crate::env::print::Splf;
use crate::policy;

/// The tree group.
pub const GROUP: &str = "Print";
/// Printer output.
pub const OUTPUT: &str = "/print";
/// Printers.
pub const PRINTERS: &str = "/print/printers";

/// Route the area's first path segments.
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    // Printer output needs one of its views.
    if !rq.allowed(policy::PRINT_TEXT)
        && !rq.allowed(policy::PRINT_PDF)
        && let Err(r) = rq.policy(policy::PRINT_TEXT, &[GROUP])
    {
        return r;
    }
    match segs {
        ["print"] => list::page(rq),
        ["print", "props", id] => props::page(rq, id),
        ["print", "view", id, rest @ ..] => view::route(rq, id, rest),
        ["print", "confirm"] => act::confirm(rq),
        ["print", "act"] => act::link(rq),
        ["print", "printers"] => printers::page(rq),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// A spooled file the user may work with: another user's needs policy
/// `print.others` (the system checks its own authority after).
pub fn mine_or_allowed(rq: &Rq<'_>, f: &Splf) -> Result<(), Response> {
    if f.user == rq.user() {
        return Ok(());
    }
    rq.policy(policy::PRINT_OTHERS, &[GROUP, "Printer output"])
}

/// The spooled file `id` names, or the page that says why not.
pub fn find(rq: &Rq<'_>, id: &str) -> Result<Splf, Response> {
    match rq.env().print.find(rq.user(), id) {
        Ok(f) => {
            mine_or_allowed(rq, &f)?;
            Ok(f)
        }
        Err(m) => {
            let m = super::files::error(rq, &m);
            Err(rq.done(OUTPUT, m))
        }
    }
}

/// The viewer's route of `f`.
pub fn view_route(f: &Splf) -> String {
    format!("/print/view/{}", f.id())
}

/// A status as the list shows it: ready and printing green, held and
/// waiting for a message red.
pub fn status(s: &str) -> String {
    let good = match s {
        "RDY" | "WTR" => Some(true),
        "HLD" | "MSGW" => Some(false),
        _ => None,
    };
    qsweb::partials::status(s, good)
}
