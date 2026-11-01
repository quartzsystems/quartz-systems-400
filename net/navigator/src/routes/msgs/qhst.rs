// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/qhst?f=&ft=&t=&tt=&id=&sev=&job=&page=`: the history log
//! (`DSPLOG`), newest first, as column-exact Courier text with the
//! message IDs linked.

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Paging, paging};

use super::idlink;
use crate::env::msgs::HistEntry;
use crate::routes::Rq;
use crate::routes::common::matches;
use crate::util::{parse_hms, parse_mdy};

/// The column headings.
const HEADING: &str = "TIME      MSGID     SEV  JOB                                 MESSAGE";

/// Whether entry `e` is from job filter `job`: `*ALL`, a qualified
/// job, or a (generic) job name.
fn of_job(job: &str, e: &HistEntry) -> bool {
    let j = job.trim().to_ascii_uppercase();
    if j.contains('/') {
        return e.job == j;
    }
    matches(&j, e.job.rsplit('/').next().unwrap_or_default())
}

/// The page.
pub fn page(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let (today, _) = rq.stamp(rq.env().system.now());
    let val = |k: &str, dft: &str| q.get(k).map_or_else(|| dft.to_string(), |v| v.trim().to_ascii_uppercase());
    let f = val("f", &today);
    let ft = val("ft", "00:00:00");
    let t = val("t", &today);
    let tt = val("tt", "23:59:59");
    let id = val("id", "*ALL");
    let sev = val("sev", "0");
    let job = val("job", "*ALL");
    let from = parse_mdy(&f).map(|d| d + parse_hms(&ft).unwrap_or(0));
    let to = parse_mdy(&t).map(|d| d + parse_hms(&tt).unwrap_or(86_399));
    let min: u8 = sev.parse().unwrap_or(0);
    let mut es: Vec<HistEntry> = rq
        .env()
        .msgs
        .history(rq.user())
        .into_iter()
        .filter(|e| from.is_none_or(|s| e.sent >= s) && to.is_none_or(|s| e.sent <= s))
        .filter(|e| matches(&id, &e.id) && e.severity >= min && of_job(&job, e))
        .collect();
    es.reverse();
    let defaults = [
        ("f", f.as_str(), today.as_str()),
        ("ft", &ft, "00:00:00"),
        ("t", &t, &today),
        ("tt", &tt, "23:59:59"),
        ("id", &id, "*ALL"),
        ("sev", &sev, "0"),
        ("job", &job, "*ALL"),
    ];
    let kept: Vec<(String, String)> = defaults
        .iter()
        .filter(|(_, v, d)| v != d)
        .map(|(k, v, _)| ((*k).to_string(), (*v).to_string()))
        .collect();
    let p = Paging::from_query(q.str("page"), q.str("rows"), es.len(), kept);
    let input = |name: &str, v: &str, size: u32| {
        format!("<input type=\"text\" name=\"{name}\" value=\"{}\" size=\"{size}\">", esc(v))
    };
    let filter = format!(
        "<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr><td>From:</td><td>{} {}</td><td>To:</td><td>{} {}</td><td>Message ID:</td><td>{}</td><td>Severity &ge;:</td><td>{}</td><td>Job:</td><td>{}</td><td><input type=\"submit\" value=\"Go\"> </td></tr></table></form>",
        input("f", &f, 8),
        input("ft", &ft, 8),
        input("t", &t, 8),
        input("tt", &tt, 8),
        input("id", &id, 8),
        input("sev", &sev, 2),
        input("job", &job, 10),
    );
    let mut pre = format!("{HEADING}\n");
    for e in &es[p.range()] {
        let (_, time) = rq.stamp(e.sent);
        let pad = " ".repeat(10usize.saturating_sub(e.id.len()));
        let _ = writeln!(
            pre,
            "{time:<10}{}{pad}{:02}   {}{}",
            idlink(rq, &e.id),
            e.severity,
            esc(&format!("{:<36}", e.job)),
            esc(&e.text)
        );
    }
    let body = format!("{filter}<pre class=\"out\">{pre}</pre>{}", paging(&p));
    super::page(
        rq,
        &["History log"],
        "History log",
        "History log",
        Some("System history log QHST (DSPLOG). Newest first."),
        body,
        "qhst",
        Some((super::GROUP, Some("/qhst"))),
    )
}
