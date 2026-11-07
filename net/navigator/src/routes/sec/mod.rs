// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Security: `/audit?t=&u=&f=&to=&page=`, the audit journal as
//! `DSPAUDJRNE` prints it, Courier, newest first; `?csv=1` downloads the
//! entries that match.

use std::fmt::Write as _;

use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::partials::{FilterField, Paging, filter, link, paging};

use super::Rq;
use super::common::group_page;
use crate::env::sec::{AuditEntry, AuditFilter};
use crate::env::{Need, Target};

/// The tree's group.
const GROUP: &str = "Security";

/// The entry types the filter offers.
const TYPES: [&str; 9] = [
    "*ALL",
    "AF - Authority failure",
    "PW - Password",
    "CP - User profile change",
    "SV - System value",
    "CO - Create object",
    "DO - Delete object",
    "ZC - Object change",
    "JS - Job start",
];

/// The heading of the printed entries.
pub const HEADING: &str =
    "TIME      TYPE  USER       JOB                              OBJECT / DETAIL";

/// Route `segs` (the path's segments, the area's first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["audit"] => audit(rq),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// One entry as printed, its time `time`.
pub fn line(time: &str, e: &AuditEntry) -> String {
    format!(
        "{time:<10}{:<6}{:<11}{:<33}{}",
        e.kind, e.user, e.job, e.detail
    )
}

/// Days since 1970 of `MM/DD/YY`.
#[allow(clippy::many_single_char_names)]
fn day_of(s: &str) -> Option<i64> {
    let mut p = s.trim().split('/').map(|x| x.parse::<u32>().ok());
    let (m, d, y) = (p.next()??, p.next()??, p.next()??);
    if p.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if y < 100 {
        2000 + i64::from(y)
    } else {
        i64::from(y)
    };
    Some(crate::util::days(y, m, d))
}

/// Seconds the user's clock is ahead of UTC.
fn offset(rq: &Rq<'_>) -> i64 {
    let now = rq.env().system.now();
    let (d, t) = rq.stamp(now);
    let mut hms = t.split(':').map(|x| x.parse::<i64>().unwrap_or(0));
    let secs =
        hms.next().unwrap_or(0) * 3600 + hms.next().unwrap_or(0) * 60 + hms.next().unwrap_or(0);
    let local = day_of(&d).unwrap_or(0) * 86_400 + secs;
    local - i64::try_from(now).unwrap_or(0)
}

fn audit(rq: &Rq<'_>) -> Response {
    let journal = Target::obj("QSYS", "QAUDJRN", "*JRN");
    if let Err(r) = rq.require(
        Need::Audit,
        "Display Audit Journal Entries",
        &journal,
        &[GROUP, "Audit journal"],
    ) {
        return r;
    }
    let q = rq.query();
    let code = q.str("t").get(..2).unwrap_or("").to_ascii_uppercase();
    let chosen = TYPES
        .iter()
        .find(|t| t.starts_with(&code) && !code.is_empty() && **t != "*ALL");
    let kind = if code.is_empty() || code == "*A" {
        String::new()
    } else {
        code.clone()
    };
    let user = match q.str("u").trim().to_ascii_uppercase() {
        u if u.is_empty() => "*ALL".to_string(),
        u => u,
    };
    let today = rq.stamp(rq.env().system.now()).0;
    let from_s = if day_of(q.str("f")).is_some() {
        q.str("f").trim().to_string()
    } else {
        today.clone()
    };
    let to_s = if day_of(q.str("to")).is_some() {
        q.str("to").trim().to_string()
    } else {
        today.clone()
    };
    let off = offset(rq);
    let at =
        |s: &str, end: i64| u64::try_from(day_of(s).unwrap_or(0) * 86_400 + end - off).unwrap_or(0);
    let f = AuditFilter {
        kind: kind.clone(),
        user: user.clone(),
        from: at(&from_s, 0),
        to: at(&to_s, 86_399),
    };
    let entries = match rq.env().sec.audit_entries(rq.user(), &f) {
        Ok(e) => e,
        Err(m) => {
            rq.flash(m);
            Vec::new()
        }
    };
    let mut kept: Vec<(String, String)> = Vec::new();
    if !kind.is_empty() {
        kept.push(("t".into(), kind.clone()));
    }
    if user != "*ALL" {
        kept.push(("u".into(), user.clone()));
    }
    if from_s != today {
        kept.push(("f".into(), from_s.clone()));
    }
    if to_s != today {
        kept.push(("to".into(), to_s.clone()));
    }
    if q.has("csv") {
        let rows: Vec<Vec<Option<String>>> = entries
            .iter()
            .map(|e| {
                let (d, t) = rq.stamp(e.at);
                [
                    d,
                    t,
                    e.kind.clone(),
                    e.user.clone(),
                    e.job.clone(),
                    e.detail.clone(),
                ]
                .into_iter()
                .map(Some)
                .collect()
            })
            .collect();
        let cols: Vec<String> = ["Date", "Time", "Type", "User", "Job", "Detail"]
            .iter()
            .map(ToString::to_string)
            .collect();
        return Response::download(
            "audit.csv",
            "text/csv; charset=utf-8",
            super::db::csv(&cols, &rows),
        );
    }
    let p = Paging::from_query(q.str("page"), q.str("rows"), entries.len(), kept.clone());
    let mut out = String::from(HEADING);
    out.push('\n');
    for e in &entries[p.range()] {
        let _ = writeln!(out, "{}", line(&rq.stamp(e.at).1, e));
    }
    let form = filter(
        &[
            FilterField::Select(
                "Entry type".into(),
                "t".into(),
                TYPES.iter().map(ToString::to_string).collect(),
                chosen.map(ToString::to_string).unwrap_or_default(),
            ),
            FilterField::Text("User".into(), "u".into(), user, 10),
            FilterField::Text("From".into(), "f".into(), from_s, 8),
            FilterField::Text("To".into(), "to".into(), to_s, 8),
        ],
        false,
    );
    let rcv = rq.env().sec.audit_receiver(rq.user());
    let mut csv_q: Vec<(&str, &str)> = kept.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    csv_q.push(("csv", "1"));
    let body = format!(
        "{form}<pre class=\"out\">{}</pre>{}<p class=\"small grey\">Journal {}, receiver {} (attached {}). {} &nbsp; {}</p>",
        esc(&out),
        paging(&p),
        esc(&rcv.journal),
        esc(&rcv.receiver),
        esc(&rq.stamp(rcv.attached).0),
        link(
            &rq.ctx.url(&format!("/audit{}", query(&csv_q))),
            "Download as CSV"
        ),
        link(
            &rq.ctx.url("/sysval?cat=security"),
            "Auditing system values"
        ),
    );
    group_page(
        rq,
        GROUP,
        &["Audit journal"],
        "Audit journal",
        Some("Security audit journal entries (DSPAUDJRNE)."),
        body,
        "audit",
        Some("/audit"),
    )
}
