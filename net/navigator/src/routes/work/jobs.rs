// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/jobs`: the active jobs (`WRKACTJOB`), filtered by subsystem, user,
//! job name and status, sorted and paged. Refresh starts a new elapsed
//! interval.

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, FilterField, List, Paging, filter, list_table, paging};

use super::{GROUP, all_or, error_waiting, job_route, keep, matches, strings, word};
use crate::env::work::{Choices, Job};
use crate::routes::Rq;
use crate::util::tenths;

/// The statuses the filter offers.
const STATUSES: &[&str] = &[
    "*ALL", "RUN", "DEQW", "MSGW", "LCKW", "TIMW", "PSRW", "SELW", "DSPW", "EVTW",
];

/// A job's status word: `RUN` green, `MSGW` red.
pub fn status(s: &str) -> String {
    let good = match s {
        "RUN" => Some(true),
        "MSGW" | "HLD" => Some(false),
        _ => None,
    };
    word(s, good)
}

fn sort(jobs: &mut [Job], key: &str) {
    match key {
        "name" => jobs.sort_by(|a, b| a.name.cmp(&b.name)),
        "user" => jobs.sort_by(|a, b| a.user.cmp(&b.user)),
        "number" => jobs.sort_by(|a, b| a.number.cmp(&b.number)),
        "type" => jobs.sort_by(|a, b| a.kind.cmp(&b.kind)),
        "sbs" => jobs.sort_by(|a, b| a.subsystem.cmp(&b.subsystem)),
        "status" => jobs.sort_by(|a, b| a.status.cmp(&b.status)),
        "cpu" => jobs.sort_by_key(|j| std::cmp::Reverse(j.cpu)),
        "func" => jobs.sort_by(|a, b| a.function.cmp(&b.function)),
        "curuser" => jobs.sort_by(|a, b| a.current_user.cmp(&b.current_user)),
        _ => {}
    }
}

fn row(rq: &Rq<'_>, j: &Job) -> Vec<Cell> {
    let r = job_route(&j.qualified());
    let u = |tail: &str| rq.ctx.url(&format!("{r}/{tail}"));
    let acts = qsweb::partials::actions(&[
        (u("hold"), "Hold"),
        (u("release"), "Release"),
        (u("end"), "End"),
        (u("joblog"), "Job log"),
        (u("general"), "Properties"),
        (u("splf"), "Spooled files"),
    ]);
    vec![
        Cell::act(acts),
        Cell::mono_html(format!(
            "<a href=\"{}\">{}</a>",
            esc(&u("general")),
            esc(&j.name)
        )),
        Cell::mono(&j.user),
        Cell::mono(&j.number),
        Cell::mono(&j.kind),
        Cell::mono(&j.subsystem),
        Cell::mono_html(status(&j.status)),
        Cell::num(&tenths(j.cpu)),
        Cell::mono(&j.function),
        Cell::mono(&j.current_user),
    ]
}

/// `/jobs?sbs=&user=&job=&sts=&sort=&page=&rows=`.
#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
pub fn list(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let sbs = all_or(q.str("sbs"));
    let user = all_or(q.str("user"));
    let name = all_or(q.str("job"));
    let sts = all_or(q.str("sts"));
    let key = q.str("sort").to_string();
    let w = &rq.env().work;
    let data = w.active_jobs(rq.user(), q.has("refresh"));
    let mut jobs: Vec<Job> = data
        .jobs
        .iter()
        .filter(|j| {
            (sbs == "*ALL" || j.subsystem == sbs)
                && matches(&user, &j.user)
                && matches(&name, &j.name)
        })
        .filter(|j| sts == "*ALL" || j.status == sts)
        .cloned()
        .collect();
    sort(&mut jobs, &key);
    let filters = keep(&[
        ("sbs", &sbs),
        ("user", &user),
        ("job", &name),
        ("sts", &sts),
    ]);
    let mut kept = filters.clone();
    if !key.is_empty() {
        kept.push(("sort".into(), key.clone()));
    }
    let p = Paging::from_query(q.str("page"), q.str("rows"), jobs.len(), kept);
    let mut subsystems = vec!["*ALL".to_string()];
    subsystems.extend(w.choices(rq.user(), Choices::Subsystems));
    let chosen = |v: &str| {
        if v == "*ALL" {
            String::new()
        } else {
            v.to_string()
        }
    };
    let f = filter(
        &[
            FilterField::Select("Subsystem".into(), "sbs".into(), subsystems, chosen(&sbs)),
            FilterField::Text("User".into(), "user".into(), user.clone(), 10),
            FilterField::Text("Job name".into(), "job".into(), name.clone(), 10),
            FilterField::Select(
                "Status".into(),
                "sts".into(),
                strings(STATUSES),
                chosen(&sts),
            ),
        ],
        true,
    );
    let summary = if jobs.is_empty() || error_waiting(rq) {
        String::new()
    } else {
        let msgw = data.jobs.iter().filter(|j| j.status == "MSGW").count();
        let msgw = if msgw == 0 {
            "0".to_string()
        } else {
            format!("<span class=\"err\">{msgw}</span>")
        };
        let e = data.elapsed;
        format!(
            "<div class=\"small grey\">Elapsed time: {:02}:{:02}:{:02} &nbsp; CPU %: {} &nbsp; Active jobs: {} &nbsp; Jobs with MSGW: {msgw}</div>",
            e / 3600,
            e % 3600 / 60,
            e % 60,
            tenths(data.cpu),
            data.jobs.len()
        )
    };
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Job name", "name"),
            Col::sorted("User", "user"),
            Col::sorted("Number", "number"),
            Col::sorted("Type", "type"),
            Col::sorted("Subsystem", "sbs"),
            Col::sorted("Status", "status"),
            Col::sorted("CPU %", "cpu"),
            Col::sorted("Function", "func"),
            Col::sorted("Current user", "curuser"),
        ],
        rows: jobs[p.range()].iter().map(|j| row(rq, j)).collect(),
        empty: "No active jobs match the filter.".into(),
        keep: filters,
    };
    let body = format!("{f}{summary}{}{}", list_table(&list), paging(&p));
    rq.page(Page {
        title: "Active jobs".into(),
        crumb: vec![GROUP.into(), "Active jobs".into()],
        h1: "Active jobs".into(),
        desc: Some("Jobs currently active on the system (WRKACTJOB).".into()),
        messages: data.notes,
        body,
        help: "jobs".into(),
        nav: Some((GROUP, Some("/jobs"))),
        ..Page::default()
    })
}
