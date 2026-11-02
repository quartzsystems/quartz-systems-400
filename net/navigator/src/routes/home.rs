// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System status: the home page (`WRKSYSSTS` and the counts of the
//! other pages). Every value links to its detail page; `?auto=` reloads
//! it with `<meta http-equiv="refresh">`.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{mono, select};

use super::Rq;
use crate::env::Status;
use crate::util::{gb, mb, tenths, thousands, uptime};

/// The auto-refresh choices, seconds.
const AUTO: &[&str] = &["Off", "30", "60", "120", "300"];

/// A utilisation bar for `pct` percent (red from 70).
pub fn bar(pct: u32) -> String {
    let pct = pct.min(100);
    let class = if pct >= 70 { "hot" } else { "fill" };
    format!(
        "<table class=\"bar\" cellspacing=\"0\" cellpadding=\"0\"><tr><td class=\"{class}\" width=\"{pct}%\"></td><td width=\"{}%\"></td></tr></table>",
        100 - pct
    )
}

/// A `table.status` of label and value (HTML) rows.
fn status_table(rows: &[(&str, String)]) -> String {
    let mut s = String::from("<table class=\"status\" cellspacing=\"0\">");
    for (l, v) in rows {
        let _ = write!(s, "<tr><td class=\"lbl\">{}</td><td>{v}</td></tr>", esc(l));
    }
    s.push_str("</table>");
    s
}

/// The auto-refresh choice of the query: seconds, or `None` for Off.
pub fn auto(rq: &Rq<'_>) -> (String, Option<u32>) {
    auto_in(rq, "auto", AUTO)
}

/// The auto-refresh choice of query field `name` among `choices`.
pub fn auto_in(rq: &Rq<'_>, name: &str, choices: &[&str]) -> (String, Option<u32>) {
    let q = rq.query();
    let v = q.str(name);
    match v.parse::<u32>() {
        Ok(n) if choices.contains(&v) => (v.to_string(), Some(n)),
        _ => ("Off".into(), None),
    }
}

/// The toolbar: Refresh and the auto-refresh select.
pub fn toolbar(chosen: &str) -> String {
    toolbar_in("auto", AUTO, chosen, "")
}

/// A toolbar with select `name` of `choices`, `tail` (HTML) after Apply.
pub fn toolbar_in(name: &str, choices: &[&str], chosen: &str, tail: &str) -> String {
    format!(
        "<form method=\"get\"><div class=\"toolbar\"><input type=\"submit\" value=\"Refresh\"> &nbsp; Auto-refresh every {} seconds <input type=\"submit\" value=\"Apply\">{tail}</div></form>",
        select(name, choices, chosen)
    )
}

fn link(rq: &Rq<'_>, route: &str, text: &str) -> String {
    format!("<a href=\"{}\">{}</a>", esc(&rq.ctx.url(route)), text)
}

fn system_rows(rq: &Rq<'_>, s: &Status, now: u64) -> Vec<(&'static str, String)> {
    let (id, it) = rq.stamp(s.ipl);
    let (nd, nt) = rq.stamp(now);
    let ptf = if s.ptf_level.is_empty() {
        String::new()
    } else {
        format!(" &nbsp; {}", link(rq, "/ptf", &format!("PTF level {}", esc(&s.ptf_level))))
    };
    vec![
        ("System name", mono(&rq.ctx.system)),
        ("Serial number", mono(&s.serial)),
        ("Model", model(s)),
        ("Release", format!("{}{ptf}", mono(&s.release))),
        ("Processors", esc(&s.processors)),
        ("Memory", mb(s.main_storage)),
        (
            "IPL date/time",
            format!("{id} {it} &nbsp; <span class=\"grey\">(up {})</span>", uptime(now.saturating_sub(s.ipl))),
        ),
        ("IPL type / source", esc(&s.ipl_kind)),
        ("Time", format!("{nd} {nt} {} &nbsp; {}", esc(&s.zone), link(rq, "/time", "Time management"))),
    ]
}

/// The hardware's vendor and product as the firmware names them.
/// `QMODEL` and `QPRCFEAT` stay system values for programs; no page
/// shows them.
fn model(s: &Status) -> String {
    if s.hardware.is_empty() {
        "<span class=\"grey\">Not reported</span>".to_string()
    } else {
        esc(&s.hardware)
    }
}

fn storage_rows(rq: &Rq<'_>, s: &Status) -> Vec<(&'static str, String)> {
    let units = if s.units_inactive == 0 {
        format!("{} units, all active", s.units)
    } else {
        format!("{} units, {} not active", s.units, s.units_inactive)
    };
    vec![
        ("System ASP capacity", gb(s.asp_capacity)),
        ("System ASP used", format!("{}% &nbsp; {}", tenths(s.asp_used), bar(s.asp_used / 10))),
        ("Temporary storage used", format!("{} (peak {} since IPL)", mb(s.temp_used), mb(s.temp_peak))),
        (
            "Main storage pools",
            format!("{}, machine pool {}", link(rq, "/sbs", &format!("{} pools", s.pools)), mb(s.machine_pool)),
        ),
        ("Disk units", link(rq, "/disk", &units)),
    ]
}

fn activity_rows(rq: &Rq<'_>, s: &Status) -> Vec<(&'static str, String)> {
    let n = |v: u32| thousands(u64::from(v));
    let msgw = if s.msgw == 0 {
        link(rq, "/jobs?sts=MSGW", "0")
    } else {
        format!(
            "<a class=\"err\" href=\"{}\">{} (MSGW)</a>",
            esc(&rq.ctx.url("/jobs?sts=MSGW")),
            n(s.msgw)
        )
    };
    vec![
        (
            "CPU used",
            format!(
                "<b>{}%</b> &nbsp; {} &nbsp; {}",
                tenths(s.cpu),
                bar(s.cpu / 10),
                link(rq, "/perf/sys", "System activity")
            ),
        ),
        ("Jobs in system", link(rq, "/jobs", &n(s.jobs))),
        ("Active jobs", link(rq, "/jobs", &n(s.active))),
        ("Interactive jobs", n(s.interactive)),
        ("Jobs in job queues", link(rq, "/jobq", &n(s.queued))),
        ("Active subsystems", link(rq, "/sbs", &n(s.subsystems))),
        ("Jobs waiting on messages", msgw),
        ("Spooled files waiting", link(rq, "/outq", &n(s.spooled))),
    ]
}

fn attention_rows(rq: &Rq<'_>, s: &Status) -> Vec<(&'static str, String)> {
    let inq = if s.inquiries == 0 {
        String::new()
    } else {
        format!(", <span class=\"err\">{} inquiry unanswered</span>", s.inquiries)
    };
    let errors = if s.units_failed == 0 {
        "<span class=\"ok\">0</span>".to_string()
    } else {
        format!("<a class=\"err\" href=\"{}\">{}</a>", esc(&rq.ctx.url("/disk")), s.units_failed)
    };
    let fixes = match s.ptfs_not_applied {
        0 => link(rq, "/ptf", "None"),
        1 => link(rq, "/ptf", "1 PTF not applied"),
        n => link(rq, "/ptf", &format!("{n} PTFs not applied")),
    };
    let down = if s.servers_down.is_empty() {
        link(rq, "/tcp/servers", "None")
    } else {
        link(rq, "/tcp/servers", &esc(&s.servers_down.join(", ")))
    };
    vec![
        (
            "QSYSOPR messages",
            format!(
                "<a href=\"{}\">{} messages{inq}</a>",
                esc(&rq.ctx.url("/msgq/QSYS/QSYSOPR")),
                thousands(u64::from(s.qsysopr))
            ),
        ),
        ("Disk units with errors", errors),
        ("Failed sign-on attempts (24 h)", link(rq, "/audit?t=PW", &s.failed_signons.to_string())),
        ("Fixes available from Quartz", fixes),
        ("TCP servers not started", down),
    ]
}

/// `/home`.
pub fn page(rq: &Rq<'_>) -> Response {
    let (chosen, refresh) = auto(rq);
    let sys = &rq.env().system;
    let s = sys.status(rq.user(), true);
    let now = sys.now();
    let mut activity = activity_rows(rq, &s);
    let cpu_label = format!("CPU used (elapsed {} s)", s.elapsed);
    activity[0].0 = "";
    let activity_html = status_table(&activity).replacen("<td class=\"lbl\"></td>", &format!("<td class=\"lbl\">{}</td>", esc(&cpu_label)), 1);
    let (_, t) = rq.stamp(now);
    let body = format!(
        "{}\n<table class=\"home\" width=\"100%\"><tr>\n<td width=\"50%\">\n<h3>System</h3>{}\n<h3>Storage</h3>{}\n</td>\n<td width=\"50%\">\n<h3>Activity</h3>{activity_html}\n<h3>Attention</h3>{}\n</td></tr></table>\n<p class=\"small grey\">Values are as of {t}. Elapsed-interval values are measured since the last Refresh.</p>",
        toolbar(&chosen),
        status_table(&system_rows(rq, &s, now)),
        status_table(&storage_rows(rq, &s)),
        status_table(&attention_rows(rq, &s)),
    );
    rq.page(Page {
        title: "System status".into(),
        crumb: vec!["System".into(), "System status".into()],
        h1: "System status".into(),
        desc: Some(format!("Overview of system {}.", rq.ctx.system)),
        body,
        help: "home".into(),
        nav: Some(("System", Some("/home"))),
        refresh,
        ..Page::default()
    })
}
