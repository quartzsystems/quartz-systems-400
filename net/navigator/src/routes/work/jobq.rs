// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/jobq`: the job queues (`WRKJOBQ`) and Hold (`HLDJOBQ`), Release
//! (`RLSJOBQ`) and Clear (`CLRJOBQ`), each confirmed.

use std::cmp::Reverse;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, FilterField, List, Paging, actions, filter, list_table, paging};

use super::{Confirm, all_or, confirm_page, confirmed, keep, mono, nomax, page, strings, word};
use crate::env::work::{Choices, JobQueue};
use crate::env::{Need, Target};
use crate::routes::Rq;

/// Route `/jobq/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq),
        [lib, name, op @ ("hold" | "release" | "confirm")] => {
            let qs = rq.env().work.job_queues(rq.user());
            match qs.into_iter().find(|q| q.lib == *lib && q.name == *name) {
                Some(q) => act(rq, &q, op),
                None => rq.not_found(&format!("job queue {lib}/{name}")),
            }
        }
        _ => rq.not_found(&format!("jobq/{}", rest.join("/"))),
    }
}

fn status(held: bool) -> String {
    if held {
        word("Held", Some(false))
    } else {
        word("Released", Some(true))
    }
}

fn sort(qs: &mut [JobQueue], key: &str) {
    match key {
        "name" => qs.sort_by(|a, b| a.name.cmp(&b.name)),
        "lib" => qs.sort_by(|a, b| a.lib.cmp(&b.lib)),
        "st" => qs.sort_by_key(|q| q.held),
        "sbs" => qs.sort_by(|a, b| a.subsystem.cmp(&b.subsystem)),
        "wait" => qs.sort_by_key(|q| Reverse(q.waiting)),
        "held" => qs.sort_by_key(|q| Reverse(q.held_jobs)),
        "sch" => qs.sort_by_key(|q| Reverse(q.scheduled)),
        _ => {}
    }
}

/// `/jobq?lib=&s=&sort=&page=`.
fn list(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let lib = all_or(q.str("lib"));
    let st = match q.str("s") {
        s @ ("Released" | "Held") => s.to_string(),
        _ => "*ALL".to_string(),
    };
    let key = q.str("sort").to_string();
    let w = &rq.env().work;
    let mut qs: Vec<JobQueue> = w
        .job_queues(rq.user())
        .into_iter()
        .filter(|j| lib == "*ALL" || j.lib == lib)
        .filter(|j| st == "*ALL" || j.held == (st == "Held"))
        .collect();
    sort(&mut qs, &key);
    let filters = keep(&[("lib", &lib), ("s", &st)]);
    let mut kept = filters.clone();
    if !key.is_empty() {
        kept.push(("sort".into(), key.clone()));
    }
    let p = Paging::from_query(q.str("page"), q.str("rows"), qs.len(), kept);
    let mut libs = vec!["*ALL".to_string()];
    libs.extend(w.choices(rq.user(), Choices::JobqLibraries));
    let chosen = |v: &str| {
        if v == "*ALL" {
            String::new()
        } else {
            v.to_string()
        }
    };
    let f = filter(
        &[
            FilterField::Select("Library".into(), "lib".into(), libs, chosen(&lib)),
            FilterField::Select(
                "Status".into(),
                "s".into(),
                strings(&["*ALL", "Released", "Held"]),
                chosen(&st),
            ),
        ],
        false,
    );
    let rows = qs[p.range()]
        .iter()
        .map(|j| {
            let base = format!("/jobq/{}/{}", j.lib, j.name);
            let jobs = if j.subsystem.is_empty() {
                "/jobs".to_string()
            } else {
                format!("/jobs?sbs={}", j.subsystem)
            };
            vec![
                Cell::act(actions(&[
                    (rq.ctx.url(&format!("{base}/hold")), "Hold"),
                    (rq.ctx.url(&format!("{base}/release")), "Release"),
                    (rq.ctx.url(&format!("{base}/confirm")), "Clear"),
                    (rq.ctx.url(&jobs), "Jobs"),
                ])),
                Cell::mono(&j.name),
                Cell::mono(&j.lib),
                Cell::html(status(j.held)),
                Cell::mono(&j.subsystem),
                Cell::num(&j.waiting.to_string()),
                Cell::num(&j.held_jobs.to_string()),
                Cell::num(&j.scheduled.to_string()),
                Cell::num(&nomax(j.max_active)),
                Cell::text(&j.text),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Job queue", "name"),
            Col::sorted("Library", "lib"),
            Col::sorted("Status", "st"),
            Col::sorted("Subsystem", "sbs"),
            Col::sorted("Jobs waiting", "wait"),
            Col::sorted("Held", "held"),
            Col::sorted("Scheduled", "sch"),
            Col::plain("Max active"),
            Col::plain("Text"),
        ],
        rows,
        empty: "No job queues match the filter.".into(),
        keep: filters,
    };
    let body = format!("{f}{}{}", list_table(&list), paging(&p));
    page(
        rq,
        &["Job queues"],
        "Job queues",
        Some("Job queues and the jobs waiting on them (WRKJOBQ)."),
        body,
        "jobq",
        Some("/jobq"),
    )
}

/// Hold, Release or Clear (`confirm`) a job queue.
fn act(rq: &Rq<'_>, j: &JobQueue, op: &str) -> Response {
    let qual = format!("{}/{}", j.lib, j.name);
    let (verb, cmd) = match op {
        "hold" => ("Hold", "HLDJOBQ"),
        "release" => ("Release", "RLSJOBQ"),
        _ => ("Clear", "CLRJOBQ"),
    };
    let what = format!("{verb} Job Queue");
    let command = format!("{cmd} JOBQ({qual})");
    if rq.post() {
        let t = Target::obj(&j.lib, &j.name, "*JOBQ");
        if let Err(r) = rq.require(Need::JobCtl, &what, &t, &["Work management", "Job queues"]) {
            return r;
        }
        return confirmed(rq, &command, "/jobq");
    }
    let sentence = if op == "confirm" {
        "Clearing a job queue removes every job waiting on it; the jobs do not run. Press OK to clear the job queues listed below, or Cancel to return without clearing.".to_string()
    } else {
        format!(
            "Press OK to {} the job queues listed below.",
            verb.to_ascii_lowercase()
        )
    };
    confirm_page(
        rq,
        &Confirm {
            what: &what,
            crumb: &["Job queues"],
            sentence,
            cols: &["Job queue", "Status", "Subsystem", "Jobs waiting", "Text"],
            rows: vec![vec![
                mono(&qual),
                status(j.held),
                esc(&j.subsystem),
                j.waiting.to_string(),
                esc(&j.text),
            ]],
            options: String::new(),
            command,
        },
    )
}
