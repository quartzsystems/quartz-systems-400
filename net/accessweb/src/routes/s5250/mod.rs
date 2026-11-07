// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! 5250 session (handoff steps 5-6b): `/5250` the saved sessions and
//! Start 5250 display, `/5250/<id>/properties|keymap|macros`,
//! `/5250/verify`, `/5250/session/<id>` the display (its picture, keys,
//! Print Screen and the WebSocket), `/5250/script.js` the page's script.

mod config;
mod keymap;
mod macros;
mod props;
mod script;
mod session;
mod verify;

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;

use super::Rq;
use crate::env::session5250::Saved;
use crate::policy;

/// The tree group.
pub const GROUP: &str = "5250 session";
/// The tree item.
pub const ITEM: &str = "/5250";

/// Route the area's paths.
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    let id = |s: &str| s.parse::<u32>().ok();
    match segs {
        ["5250"] => config::page(rq),
        ["5250", "verify"] => verify::page(rq),
        ["5250", "script.js"] => script::js(),
        ["5250", "session", "new"] => config::start(rq),
        ["5250", "session", n] => with_id(rq, id(n), session::page),
        ["5250", "session", n, rest] => with_id(rq, id(n), |rq, n| session::part(rq, n, rest)),
        ["5250", "ws", n] => with_id(rq, id(n), session::ws),
        ["5250", n, "properties"] => with_saved(rq, id(n), props::page),
        ["5250", n, "download"] => with_saved(rq, id(n), config::download),
        ["5250", n, "delete", "confirm"] => with_saved(rq, id(n), config::delete),
        ["5250", n, "keymap"] => with_saved(rq, id(n), keymap::page),
        ["5250", n, "keymap", "export"] => with_saved(rq, id(n), keymap::export),
        ["5250", n, "keymap", "json"] => with_saved(rq, id(n), |_, s| Response::new(200, "application/json", keymap::json(&s.file.keymap()).into_bytes())),
        ["5250", n, "macros"] => with_saved(rq, id(n), macros::page),
        ["5250", n, "macros", m, rest @ ..] => with_saved(rq, id(n), |rq, s| macros::one(rq, s, m, rest)),
        _ => rq.not_found(&format!("page {}", esc(&segs.join("/")))),
    }
}

fn with_id(rq: &Rq<'_>, id: Option<u32>, f: impl FnOnce(&Rq<'_>, u32) -> Response) -> Response {
    match id {
        Some(n) => f(rq, n),
        None => rq.not_found("5250 session"),
    }
}

fn with_saved(rq: &Rq<'_>, id: Option<u32>, f: impl FnOnce(&Rq<'_>, Saved) -> Response) -> Response {
    match id.and_then(|n| saved(rq, n)) {
        Some(s) => f(rq, s),
        None => rq.not_found("saved 5250 session"),
    }
}

/// Saved session `id` of the user; 0 is the one started without saving.
pub fn saved(rq: &Rq<'_>, id: u32) -> Option<Saved> {
    if id == 0 {
        return config::unsaved(rq);
    }
    rq.env().session5250.sessions(rq.user()).into_iter().find(|s| s.id == id)
}

/// The policy check every page of the area makes first.
pub fn start_policy(rq: &Rq<'_>, crumb: &[&str]) -> Result<(), Response> {
    rq.policy(policy::S5250_START, crumb)
}

/// `secs` as the lists show it: the user's date, hours and minutes.
pub fn when_short(rq: &Rq<'_>, secs: u64) -> String {
    if secs == 0 {
        return String::new();
    }
    let (d, t) = rq.stamp(secs);
    // HH:MM:SS or HH:MM:SS AM: drop the seconds.
    let hm = if t.len() > 8 { format!("{}{}", &t[..5], &t[8..]) } else { t[..5.min(t.len())].to_string() };
    format!("{d} {hm}")
}

/// A select whose first option is the default: only another choice is
/// marked `selected`.
pub fn select(name: &str, options: &[String], chosen: usize) -> String {
    let mut s = format!("<select name=\"{}\">", esc(name));
    for (i, o) in options.iter().enumerate() {
        let sel = if i == chosen && i > 0 { " selected" } else { "" };
        let _ = write!(s, "<option{sel}>{}</option>", esc(o));
    }
    s.push_str("</select>");
    s
}

/// `Yes` or `No`.
pub fn yes(b: bool) -> &'static str {
    if b { "Yes" } else { "No" }
}
