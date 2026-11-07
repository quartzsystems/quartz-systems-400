// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Watching (the desktop's Messages > Options, by polling): the queues
//! watched (the user's own, or it and `QSYSOPR`) are read on each
//! request; messages that arrived since the user last looked at the
//! queue make the header's `Messages waiting: N`, and the first one the
//! Arrived option names opens Message Arrived on the next page asked
//! for. The marks are the highest message key seen, kept in the session
//! from its first request on.

use qsweb::html::esc;
use qsweb::http::Response;

use super::super::{EXPIRED, Rq};
use crate::PRODUCT;
use crate::env::msgs::QueuedMessage;
use crate::policy;

/// The session's Watch choice: `own` (the default), `both`, `off`.
pub const WATCH: &str = "_mw";
/// The session's Arrived choice: `inq` (the default), `all`, `none`.
pub const ARRIVED: &str = "_ma";

/// The Watch select's options and their codes.
pub const WATCH_CHOICES: [(&str, &str); 3] = [("My queue", "own"), ("My queue and QSYSOPR", "both"), ("Off", "off")];
/// The Arrived select's options and their codes.
pub const ARRIVED_CHOICES: [(&str, &str); 3] = [("Inquiries only", "inq"), ("All", "all"), ("None", "none")];

/// The operator's queue.
pub const QSYSOPR: (&str, &str) = ("QSYS", "QSYSOPR");

fn seen_key(q: &str) -> String {
    format!("_mseen:{q}")
}

fn arrived_key(q: &str) -> String {
    format!("_marr:{q}")
}

/// The queues watched, as `(lib, name)`.
pub fn watched(rq: &Rq<'_>) -> Vec<(String, String)> {
    let w = rq.get(WATCH);
    if w == "off" || !rq.allowed(policy::MSGS_DISPLAY) {
        return Vec::new();
    }
    let mut v = vec![rq.env().msgs.own_queue(rq.user())];
    if w == "both" && v[0] != (QSYSOPR.0.to_string(), QSYSOPR.1.to_string()) {
        v.push((QSYSOPR.0.into(), QSYSOPR.1.into()));
    }
    v
}

/// The messages of each watched queue newer than its seen and arrived
/// marks (the marks set on the first look).
fn newer(rq: &Rq<'_>) -> Vec<(String, Vec<QueuedMessage>, u64, u64)> {
    let mut out = Vec::new();
    for (lib, name) in watched(rq) {
        let Ok(ms) = rq.env().msgs.messages(rq.user(), &lib, &name) else {
            continue;
        };
        let q = format!("{lib}/{name}");
        let top = ms.iter().map(QueuedMessage::order).max().unwrap_or(0);
        let mark = |k: &str| rq.get(k).parse::<u64>().ok();
        let (seen, arrived) = if let (Some(s), Some(a)) = (mark(&seen_key(&q)), mark(&arrived_key(&q))) {
            (s, a)
        } else {
            let t = top.to_string();
            rq.set(&[(&seen_key(&q), &t), (&arrived_key(&q), &t)]);
            (top, top)
        };
        out.push((q, ms, seen, arrived));
    }
    out
}

/// The user looked at queue `q` (`lib/name`) up to key `top`.
pub fn seen(rq: &Rq<'_>, q: &str, top: u64) {
    let t = top.to_string();
    let a = rq.get(&arrived_key(q)).parse::<u64>().unwrap_or(0).max(top).to_string();
    rq.set(&[(&seen_key(q), &t), (&arrived_key(q), &a)]);
}

/// Message Arrived was shown for message `key` of queue `q`.
pub fn acknowledged(rq: &Rq<'_>, q: &str, key: u64) {
    let a = rq.get(&arrived_key(q)).parse::<u64>().unwrap_or(0).max(key).to_string();
    rq.set(&[(&arrived_key(q), &a)]);
}

/// The header's `Messages waiting: N`, when messages arrived on the
/// watched queues since the user last looked.
pub fn note(rq: &Rq<'_>) -> Option<String> {
    if !rq.get(EXPIRED).is_empty() {
        return None;
    }
    let n: usize = newer(rq).iter().map(|(_, ms, seen, _)| ms.iter().filter(|m| m.order() > *seen).count()).sum();
    (n > 0).then(|| {
        format!(
            "<a href=\"{}\">Messages waiting: <b>{n}</b></a>",
            esc(&PRODUCT.url("/messages"))
        )
    })
}

/// The first message Message Arrived is to show: its queue and itself.
pub fn first_arrived(rq: &Rq<'_>) -> Option<(String, QueuedMessage)> {
    let what = rq.get(ARRIVED);
    if what == "none" {
        return None;
    }
    for (q, ms, _, arrived) in newer(rq) {
        let mut fresh: Vec<&QueuedMessage> = ms
            .iter()
            .filter(|m| m.order() > arrived && (what == "all" || m.waiting()))
            .collect();
        fresh.sort_by_key(|m| m.order());
        if let Some(m) = fresh.first() {
            return Some((q, (*m).clone()));
        }
    }
    None
}

/// On a page asked for by GET, Message Arrived first when a message
/// arrived that the Arrived option names.
pub fn arrived(rq: &Rq<'_>, segs: &[&str]) -> Option<Response> {
    if rq.post() || !rq.get(EXPIRED).is_empty() {
        return None;
    }
    if matches!(segs.first(), Some(&("messages" | "msg" | "help" | "nav" | "password" | "signoff")) | None) {
        return None;
    }
    first_arrived(rq)?;
    let back = qsweb::html::query(&[("back", &rq.ctx.here)]);
    Some(rq.redirect(&format!("/messages/arrived{back}")))
}
