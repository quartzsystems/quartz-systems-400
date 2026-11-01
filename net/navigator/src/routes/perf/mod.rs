// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Performance: `/perf/sys` (`WRKSYSACT` and `WRKSYSSTS`'s pools) and
//! `/perf/disk` (`WRKDSKSTS`'s I/O rates), each over the interval since
//! the user's previous display; `?a=` reloads them with
//! `<meta http-equiv="refresh">`.

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, list_table};

use super::Rq;
use super::home::{auto_in, bar, toolbar_in};
use crate::env::perf::UnitIo;
use crate::util::{tenths, thousands};

/// The tree's group.
const GROUP: &str = "Performance";

/// The auto-refresh choices, seconds.
const AUTO: &[&str] = &["Off", "5", "10", "30", "60"];

/// A job's CPU bar spans 0 to this many tenths of a percent.
const JOB_SCALE: u32 = 200;

/// Route `segs` (the path's segments, the area's first).
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["perf", "sys"] => system(rq),
        ["perf", "disk"] => disks(rq),
        _ => rq.not_found(&segs.join("/")),
    }
}

/// `HH:MM:SS` of seconds.
fn hms(s: u32) -> String {
    format!("{:02}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
}

/// The toolbar with the elapsed interval; the refresh interval.
fn toolbar(rq: &Rq<'_>, elapsed: u32) -> (String, Option<u32>) {
    let (chosen, refresh) = auto_in(rq, "a", AUTO);
    let tail = format!(
        " &nbsp; <span class=\"small grey\">Elapsed {}</span>",
        hms(elapsed)
    );
    (toolbar_in("a", AUTO, &chosen, &tail), refresh)
}

#[allow(clippy::too_many_arguments)]
fn page(
    rq: &Rq<'_>,
    item: &'static str,
    h1: &str,
    desc: &str,
    body: String,
    help: &str,
    refresh: Option<u32>,
) -> Response {
    rq.page(qsweb::frame::Page {
        title: h1.into(),
        crumb: vec![GROUP.into(), h1.into()],
        h1: h1.into(),
        desc: Some(desc.into()),
        body,
        help: help.into(),
        nav: Some((GROUP, Some(item))),
        refresh,
        ..qsweb::frame::Page::default()
    })
}

/// `/perf/sys?a=&sort=`.
#[allow(clippy::too_many_lines)]
fn system(rq: &Rq<'_>) -> Response {
    let mut s = rq.env().perf.system(rq.user());
    let (tools, refresh) = toolbar(rq, s.elapsed);
    if rq.query().str("sort") == "io" {
        s.jobs.sort_by_key(|j| std::cmp::Reverse(j.io));
    } else {
        s.jobs.sort_by_key(|j| std::cmp::Reverse(j.cpu));
    }
    let figures = format!(
        "<table class=\"status\" style=\"width:auto\"><tr><td class=\"lbl\">CPU % total</td><td><b>{}</b> &nbsp; {}</td><td class=\"lbl\">&nbsp; CPU % database</td><td>{}</td><td class=\"lbl\">&nbsp; Interactive</td><td>{}</td><td class=\"lbl\">&nbsp; Overhead</td><td>{}</td></tr></table>",
        tenths(s.cpu),
        bar(s.cpu / 10),
        tenths(s.cpu_db),
        tenths(s.interactive),
        tenths(s.overhead)
    );
    let rows = s
        .jobs
        .iter()
        .map(|j| {
            let u = |t: &str| rq.ctx.url(&format!("/job/{}/{t}", j.qualified()));
            vec![
                Cell::act(qsweb::partials::actions(&[
                    (u("hold"), "Hold"),
                    (u("release"), "Release"),
                    (u("end"), "End"),
                    (u("joblog"), "Job log"),
                    (u("general"), "Properties"),
                    (u("splf"), "Spooled files"),
                ])),
                Cell::mono_html(format!(
                    "<a href=\"{}\">{}</a>",
                    esc(&u("general")),
                    esc(&j.name)
                )),
                Cell::mono(&j.user),
                Cell::mono(&j.number),
                Cell::mono(&j.kind),
                Cell::num(&tenths(j.cpu)),
                Cell::html(bar((j.cpu * 100 / JOB_SCALE).clamp(1, 100))),
                Cell::num(&thousands(j.io)),
                Cell::num(&thousands(j.transactions)),
            ]
        })
        .collect();
    let jobs = List {
        cols: vec![
            Col::plain("Actions"),
            Col::plain("Job name"),
            Col::plain("User"),
            Col::plain("Number"),
            Col::plain("Type"),
            Col::sorted("CPU %", "cpu"),
            Col::plain(""),
            Col::sorted("Disk I/O", "io"),
            Col::plain("Transactions"),
        ],
        rows,
        empty: "No jobs used the processor in the interval.".into(),
        keep: Vec::new(),
    };
    let pools = List {
        cols: [
            "Pool",
            "Name",
            "Size (MB)",
            "Reserved",
            "Act level",
            "DB faults/s",
            "Non-DB faults/s",
            "Act-Wait",
            "Wait-Inel",
            "Act-Inel",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows: s
            .pools
            .iter()
            .map(|p| {
                vec![
                    Cell::num(&p.number.to_string()),
                    Cell::mono(&p.name),
                    Cell::num(&thousands(p.size_mb)),
                    Cell::num(&thousands(p.reserved_mb)),
                    Cell::num(&p.activity.map_or("+++++".to_string(), |a| a.to_string())),
                    Cell::num(&tenths(p.db_faults)),
                    Cell::num(&tenths(p.other_faults)),
                    Cell::num(&thousands(p.act_wait)),
                    Cell::num(&thousands(p.wait_inel)),
                    Cell::num(&thousands(p.act_inel)),
                ]
            })
            .collect(),
        empty: "No storage pools.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{tools}{figures}<h2>Jobs by CPU</h2>{}<h2>Memory pools</h2>{}",
        list_table(&jobs),
        list_table(&pools)
    );
    page(
        rq,
        "/perf/sys",
        "System activity",
        "CPU by job and pool activity (WRKSYSACT / WRKSYSSTS).",
        body,
        "perfsys",
        refresh,
    )
}

/// `Units 7 and 8 are busy above 70% (job ...).` for the busy units.
fn busy_note(units: &[UnitIo]) -> String {
    let busy: Vec<&UnitIo> = units.iter().filter(|u| u.busy >= 700).collect();
    if busy.is_empty() {
        return String::new();
    }
    let nums: Vec<String> = busy.iter().map(|u| u.unit.to_string()).collect();
    let list = match nums.as_slice() {
        [one] => format!("Unit {one} is"),
        [rest @ .., last] => format!("Units {} and {last} are", rest.join(", ")),
        [] => String::new(),
    };
    let mut jobs: Vec<&str> = busy
        .iter()
        .map(|u| u.top_job.as_str())
        .filter(|j| !j.is_empty())
        .collect();
    jobs.dedup();
    let mut s = format!("{list} busy above 70%");
    if !jobs.is_empty() {
        let _ = write!(s, " (job {})", jobs.join(", "));
    }
    s.push_str(". ");
    esc(&s)
}

/// `/perf/disk?a=`.
fn disks(rq: &Rq<'_>) -> Response {
    let d = rq.env().perf.disks(rq.user());
    let (tools, refresh) = toolbar(rq, d.elapsed);
    let rows = d
        .units
        .iter()
        .map(|u| {
            vec![
                Cell::num(&u.unit.to_string()),
                Cell::mono(&u.resource),
                Cell::num(&u.asp.to_string()),
                Cell::num(&tenths(u.busy)),
                Cell {
                    class: Some("num"),
                    html: bar((u.busy / 10).max(1)),
                },
                Cell::num(&tenths(u.reads)),
                Cell::num(&tenths(u.writes)),
                Cell::num(&thousands(u.read_kb)),
                Cell::num(&thousands(u.write_kb)),
                Cell::num(&tenths(u.service)),
                Cell::num(&tenths(u.wait)),
                Cell::num(&u.queue.to_string()),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Unit",
            "Resource",
            "ASP",
            "% busy",
            "",
            "Read req/s",
            "Write req/s",
            "Read KB/s",
            "Write KB/s",
            "Avg service (ms)",
            "Avg wait (ms)",
            "Queue",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows,
        empty: "No disk units are configured.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{tools}{}<p class=\"small grey\">{}<a href=\"{}\">Disk status</a></p>",
        list_table(&list),
        busy_note(&d.units),
        esc(&rq.ctx.url("/disk"))
    );
    page(
        rq,
        "/perf/disk",
        "Disk activity",
        "I/O by disk unit (WRKDSKSTS, elapsed interval).",
        body,
        "perfdisk",
        refresh,
    )
}
