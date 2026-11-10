// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QZLSLSTI` (a list into a user space) and `QZLSOLST` (a list into a
//! receiver), with the qualifier rules the primary follows.
//!
//! `QZLSLSTI` parameters: 0 qualified user space (20), 1 format (8),
//! 2 qualifier (15), then the optional 3 session user (10), 4 session
//! identifier (8). The answer is the user space from offset 64 on.
//!
//! `QZLSOLST` parameters: 0 receiver length, 1 format, 2 qualifier,
//! then the optional 3 session user and 4 session identifier. The
//! answer is the 64-byte list information followed by the receiver's
//! bytes.

use super::error::{ApiError, list as reason, msg};
use super::host::Host;
use super::parms::Parms;
use crate::fmt::list::{self, List, SPACE_LIST};
use crate::fmt::zlsl::{self, Level};
use crate::model::session::Session;

/// The formats both APIs take.
pub const FORMATS: &[&str] = &[
    "ZLSL0100", "ZLSL0101", "ZLSL0200", "ZLSL0201", "ZLSL0300", "ZLSL0400", "ZLSL0600", "ZLSL0700",
    "ZLSL0800", "ZLSL0900", "QSLL0100",
];

/// Where the records' offsets count from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A user space: offsets from its start, the list at
    /// [`SPACE_LIST`].
    Space,
    /// A receiver: offsets from the receiver (`ZLSL0100`'s from each
    /// record).
    Receiver,
}

/// The request.
#[derive(Clone, Debug)]
pub struct Request {
    /// The format.
    pub format: String,
    /// The information qualifier.
    pub qualifier: String,
    /// The session user, when passed.
    pub user: Option<String>,
    /// The session identifier, when passed.
    pub session: Option<i64>,
}

fn all(q: &str) -> bool {
    q.is_empty() || q == "*ALL"
}

/// The sessions the request selects, or a `CPFB68F` reason.
fn sessions<'a>(
    r: &Request,
    list: &'a [Session],
    by_workstation: bool,
) -> Result<Vec<&'a Session>, i32> {
    let q_sess = r.qualifier == "*SESSID";
    let u_sess = r.user.as_deref() == Some("*SESSID");
    if q_sess != u_sess || (r.session.is_some() && !q_sess) || (q_sess && r.session.is_none()) {
        return Err(reason::SESSID);
    }
    if r.qualifier == "*EXPANDED" {
        return Err(reason::EXPANDED);
    }
    let user = r
        .user
        .as_deref()
        .filter(|u| !u.is_empty() && *u != "*ALL" && *u != "*SESSID");
    Ok(list
        .iter()
        .filter(|s| match r.session {
            Some(id) => i64::try_from(s.id).is_ok_and(|x| x == id),
            None => {
                all(&r.qualifier)
                    || !by_workstation
                    || s.workstation.eq_ignore_ascii_case(&r.qualifier)
                    || s.expanded.eq_ignore_ascii_case(&r.qualifier)
            }
        })
        .filter(|s| user.is_none_or(|u| s.user.eq_ignore_ascii_case(u)))
        .collect())
}

/// QS/400's `QSLL0100`: its own attributes, current and pending.
fn own(cfg: &crate::model::config::Config) -> List {
    List {
        records: vec![crate::fmt::qsl::l0100(
            &cfg.attrs.current,
            &cfg.attrs.pending,
        )],
        record_len: crate::fmt::qsl::L0100_SIZE,
    }
}

/// The list a request asks for, or a `CPFB68F` reason. `target` says
/// where offsets count from.
pub fn build(h: &mut dyn Host, r: &Request, target: Target) -> Result<List, i32> {
    if !FORMATS.contains(&r.format.as_str()) {
        return Err(reason::FORMAT);
    }
    let cfg = h.load().map_err(|_| reason::FORMAT)?;
    let now = h.now();
    let ccsid = h.job_ccsid();
    let q = r.qualifier.as_str();
    let shares = || {
        cfg.shares
            .values()
            .filter(|s| all(q) || s.name.eq_ignore_ascii_case(q))
            .collect::<Vec<_>>()
    };
    let mut l = List::default();
    match r.format.as_str() {
        "ZLSL0100" => {
            l.record_len = zlsl::L0100_SIZE;
            for (i, s) in shares().into_iter().enumerate() {
                let users = zlsl::current_users(h.sessions(), &s.name);
                let base = match target {
                    Target::Space => SPACE_LIST + i * zlsl::L0100_SIZE,
                    Target::Receiver => 0,
                };
                l.records.push(zlsl::l0100(s, users, ccsid, base));
            }
        }
        "ZLSL0101" => {
            let mut at = match target {
                Target::Space => SPACE_LIST,
                Target::Receiver => 0,
            };
            for s in shares() {
                let users = zlsl::current_users(h.sessions(), &s.name);
                let rec = zlsl::l0101(s, users, ccsid, at, Level::V7R5);
                at += rec.len();
                l.records.push(rec);
            }
        }
        "ZLSL0200" => {
            l.record_len = zlsl::L0200_SIZE;
            l.records.push(zlsl::l0200(&cfg));
        }
        "ZLSL0201" => {
            l.record_len = zlsl::L0201_SIZE;
            l.records.push(zlsl::l0201(&cfg, Level::V7R5));
        }
        "ZLSL0300" => {
            l.record_len = zlsl::L0300_SIZE;
            let all_s: Vec<Session> = h.sessions().sessions().cloned().collect();
            let picked = sessions(r, &all_s, true)?;
            let t = h.sessions().clone();
            l.records = picked.iter().map(|s| zlsl::l0300(&t, s, now)).collect();
        }
        "ZLSL0400" => {
            if q != "*ALL" {
                return Err(reason::QUALIFIER);
            }
            l.record_len = zlsl::L0400_SIZE;
            let st = h.sessions().stats.clone();
            let started = h.stamp(st.started);
            let reset = h.stamp(st.reset);
            let rec = zlsl::l0400(&st, cfg.attrs.current.guest_support(), &|u| {
                if u == st.started {
                    started.clone()
                } else {
                    reset.clone()
                }
            });
            l.records.push(rec);
        }
        "ZLSL0600" => {
            l.record_len = zlsl::L0600_SIZE;
            let all_s: Vec<Session> = h.sessions().sessions().cloned().collect();
            let picked = sessions(r, &all_s, true)?;
            l.records = zlsl::l0600(&picked, now);
        }
        "ZLSL0700" => {
            l.record_len = zlsl::L0700_SIZE;
            let all_s: Vec<Session> = h.sessions().sessions().cloned().collect();
            let picked = sessions(r, &all_s, false)?;
            let share = (!all(q) && q != "*SESSID").then_some(q);
            l.records = zlsl::l0700(&picked, share, now);
        }
        "QSLL0100" => l = own(&cfg),
        "ZLSL0800" => {
            l.record_len = zlsl::L0800_SIZE;
            l.records = shares().into_iter().map(zlsl::l0800).collect();
        }
        _ => {
            if !q.is_empty() {
                return Err(reason::NOT_BLANK);
            }
            l.record_len = zlsl::L0900_SIZE;
            let mut users = h.disabled_users();
            users.sort();
            l.records = users.iter().map(|u| zlsl::l0900(u)).collect();
        }
    }
    Ok(l)
}

fn request(p: &Parms<'_>) -> Request {
    Request {
        format: p.text(1),
        qualifier: p.text(2),
        user: p.has(3).then(|| p.text(3)),
        session: p.bin8(4),
    }
}

/// `CPFB68F` for user space `space` (20 bytes as passed).
#[must_use]
pub fn lsti_error(space: &[u8], r: i32, format: &str) -> ApiError {
    let mut s = space.to_vec();
    s.resize(20, 0x40);
    ApiError::of(msg::LIST, "QZLSLSTI")
        .raw(&s)
        .bin4(r)
        .text(format, 8)
        .bin4(0)
}

/// `QZLSLSTI`.
pub fn lsti(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    let r = request(p);
    let space = p.raw(0, 20);
    let l = build(h, &r, Target::Space).map_err(|x| lsti_error(&space, x, &r.format))?;
    let now = h.now();
    let created = h.stamp(now);
    Ok(list::space(&l, &r.format, &p.raw(2, 15), &space, &created))
}

/// `QZLSOLST`.
pub fn olst(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSOLST";
    let r = request(p);
    let len = p.bin4(0);
    let Ok(n) = usize::try_from(len) else {
        return Err(ApiError::of(msg::LENGTH, API)
            .bin4(2)
            .bin4(len)
            .bin4(-1)
            .bin4(i32::MAX));
    };
    let l = build(h, &r, Target::Receiver).map_err(|x| {
        let code = if x == reason::FORMAT {
            reason::FORMAT
        } else {
            reason::LSTI
        };
        ApiError::of(msg::OPEN_LIST, API)
            .bin4(code)
            .text(&r.format, 8)
    })?;
    let now = h.now();
    let created = h.stamp(now);
    let (info, data) = list::receiver(&l, n, &created);
    let mut out = info;
    out.extend(data);
    Ok(out)
}
