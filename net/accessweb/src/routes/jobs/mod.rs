// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Jobs (handoff step 17): My jobs (`WRKUSRJOB` of the signed-on user,
//! completed jobs with output included) and Server jobs (the jobs that
//! run Access for Web and serve the session). Hold and Release are
//! `HLDJOB` and `RLSJOB` run as the user after a confirmation on the
//! page; Job log, Properties and End are Navigator's pages (End there
//! asks with its End Job confirmation). Ending a server job needs the
//! "End server jobs" policy and `*JOBCTL`.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Cell, Col, FilterField, List, filter, list_table, status};

use super::Rq;
use super::msgs::navigator;
use crate::env::Need;
use crate::env::jobs::{Stage, UserJob};
use crate::{PRODUCT, msgs, policy};

/// The tree's group.
const GROUP: &str = "Jobs";

/// The Status select: label and the stage it keeps.
const STAGES: [(&str, Option<Stage>); 4] = [
    ("All", None),
    ("Active", Some(Stage::Active)),
    ("On job queue", Some(Stage::OnQueue)),
    ("Completed (output)", Some(Stage::Completed)),
];

/// Route the area's first path segments.
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["jobs"] => mine(rq),
        ["jobs", "server"] => server(rq),
        ["jobs", "server", number, user, name, "end"] => end_server(rq, number, user, name),
        _ => rq.not_found(&rq.req.path),
    }
}

/// A job status word: running green, waiting for a message red.
fn status_cell(s: &str) -> Cell {
    let good = match s {
        "RUN" => Some(true),
        "MSGW" => Some(false),
        _ => None,
    };
    Cell::mono_html(status(s, good))
}

/// An HTTP Server Administration page (port 2001, 2010 with TLS) on the
/// host the browser reached this one by; a path when it named none.
fn httpadmin(rq: &Rq<'_>, route: &str) -> String {
    let nav = navigator(rq, route);
    let path = format!("/qs400/httpadmin{route}");
    match nav.split_once("/qs400/navigator") {
        Some((origin, _)) if !origin.is_empty() => {
            let port = if rq.peer.secure { ":2010" } else { ":2001" };
            let host = origin.rsplit_once(':').map_or(origin, |(h, _)| h);
            format!("{host}{port}{path}")
        }
        _ => path,
    }
}

/// Navigator's page of job `j` (`joblog`, `general`, `end`).
fn job_page(rq: &Rq<'_>, j: &UserJob, page: &str) -> String {
    navigator(rq, &format!("/job/{}/{}/{}/{page}", j.number, j.user, j.name))
}

/// Run `command` as the user; back to My jobs with its message.
fn run(rq: &Rq<'_>, command: &str) -> Response {
    let verb = command.split_whitespace().next().unwrap_or("");
    let m = match rq.env().jobs.run(rq.user(), command) {
        Ok(m) if m.id.is_empty() => rq.message(Kind::Done, msgs::CMD_COMPLETED, &[verb]),
        Ok(m) | Err(m) => m,
    };
    rq.done("/jobs", m)
}

/// `GET /jobs`; Hold and Release, confirmed on the page, post here.
#[allow(clippy::too_many_lines)]
fn mine(rq: &Rq<'_>) -> Response {
    let crumb = &["Jobs", "My jobs"];
    if let Err(r) = rq.policy(policy::JOBS_DISPLAY, crumb) {
        return r;
    }
    let all = rq.env().jobs.user_jobs(rq.user());
    let find = |q: &str| all.iter().find(|j| j.qualified().eq_ignore_ascii_case(q.trim()));
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/jobs");
        }
        let Some(j) = find(f.str("job")) else {
            return rq.done("/jobs", rq.message(Kind::Error, "CPF1070", &[f.str("job")]));
        };
        let verb = if f.str("op") == "release" { "RLSJOB" } else { "HLDJOB" };
        return run(rq, &format!("{verb} JOB({})", j.qualified()));
    }
    let q = rq.query();
    let mut body = String::new();
    let mut messages: Vec<Message> = Vec::new();
    for (op, verb, word) in [("hold", "HLDJOB", "Hold"), ("release", "RLSJOB", "Release")] {
        let Some(asked) = q.get(op) else { continue };
        match find(asked) {
            Some(j) => {
                let _ = write!(
                    body,
                    "<form method=\"post\">{}<p>{word} job <span class=\"mono\">{}</span> (<span class=\"mono\">{verb} JOB({})</span>)? <input type=\"hidden\" name=\"job\" value=\"{}\"><input type=\"hidden\" name=\"op\" value=\"{op}\"><input type=\"submit\" name=\"ok\" value=\"{word}\"> <input type=\"submit\" name=\"cancel\" value=\"Cancel\"></p></form>",
                    rq.ctx.csrf_field(),
                    esc(&j.qualified()),
                    esc(&j.qualified()),
                    esc(&j.qualified())
                );
            }
            None => messages.push(rq.message(Kind::Error, "CPF1070", &[asked])),
        }
    }
    let chosen = q.str("s");
    let stage = STAGES.iter().find(|s| s.0 == chosen).and_then(|s| s.1);
    let labels: Vec<String> = STAGES.iter().map(|s| s.0.to_string()).collect();
    let pick = if chosen == STAGES[0].0 { String::new() } else { chosen.to_string() };
    body.push_str(&filter(&[FilterField::Select("Status".into(), "s".into(), labels, pick)], false));
    let mut jobs: Vec<UserJob> = all.iter().filter(|j| stage.is_none_or(|s| j.stage == s)).cloned().collect();
    match q.str("sort") {
        "n" => jobs.sort_by(|a, b| a.name.cmp(&b.name).then(a.number.cmp(&b.number))),
        "s" => jobs.sort_by(|a, b| a.status.cmp(&b.status).then(a.number.cmp(&b.number))),
        "e" => jobs.sort_by_key(|j| std::cmp::Reverse(j.entered)),
        _ => {}
    }
    let end_ok = rq.allowed(policy::JOBS_END);
    let rows = jobs
        .iter()
        .map(|j| {
            let with = |k: &str| PRODUCT.url(&format!("/jobs{}", query(&[(k, &j.qualified())])));
            let mut acts = vec![
                (job_page(rq, j, "joblog"), "Job log"),
                (PRODUCT.url(&format!("/print{}", query(&[("job", &j.qualified())]))), "Printer output"),
                (with("hold"), "Hold"),
                (with("release"), "Release"),
            ];
            if end_ok {
                acts.push((job_page(rq, j, "end"), "End"));
            }
            acts.push((job_page(rq, j, "general"), "Properties"));
            let (d, t) = rq.stamp(j.entered);
            vec![
                Cell::act(qsweb::partials::actions(&acts)),
                Cell::mono(&j.name),
                Cell::mono(&j.user),
                Cell::mono(&j.number),
                Cell::text(&j.kind),
                status_cell(&j.status),
                Cell::mono(&j.subsystem),
                Cell::mono(&j.function),
                Cell::text(&format!("{d} {}", t.get(..5).unwrap_or(&t))),
            ]
        })
        .collect();
    let keep: Vec<(String, String)> = q.get("s").map(|s| vec![("s".to_string(), s.to_string())]).unwrap_or_default();
    body.push_str(&list_table(&List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Job name", "n"),
            Col::plain("User"),
            Col::plain("Number"),
            Col::plain("Type"),
            Col::sorted("Status", "s"),
            Col::plain("Subsystem"),
            Col::plain("Function"),
            Col::sorted("Entered", "e"),
        ],
        rows,
        empty: "You have no jobs on the system.".into(),
        keep,
    }));
    rq.page(Page {
        title: "My jobs".into(),
        crumb: vec!["Jobs".into(), "My jobs".into()],
        h1: "My jobs".into(),
        desc: Some("Jobs that belong to your user profile, including completed jobs with output (WRKUSRJOB).".into()),
        messages,
        body,
        help: "jobs".into(),
        nav: Some((GROUP, Some("/jobs"))),
        ..Page::default()
    })
}

/// `GET /jobs/server`.
fn server(rq: &Rq<'_>) -> Response {
    let crumb = &["Jobs", "Server jobs"];
    if let Err(r) = rq.policy(policy::JOBS_SERVER, crumb) {
        return r;
    }
    let end_ok = rq.allowed(policy::JOBS_END_SERVER);
    let jobs = rq.env().jobs.server_jobs(rq.user());
    let rows = jobs
        .iter()
        .map(|j| {
            let mut acts = vec![(job_page(rq, j, "joblog"), "Job log"), (job_page(rq, j, "general"), "Properties")];
            if end_ok {
                acts.push((PRODUCT.url(&format!("/jobs/server/{}/{}/{}/end", j.number, j.user, j.name)), "End"));
            }
            vec![
                Cell::act(qsweb::partials::actions(&acts)),
                Cell::mono(&j.name),
                Cell::mono(&j.user),
                Cell::mono(&j.number),
                Cell::mono(&j.kind),
                status_cell(&j.status),
                Cell::mono(&j.subsystem),
                Cell::mono(&j.function),
                Cell::text(&j.serving),
            ]
        })
        .collect();
    let mut body = format!(
        "<p class=\"small\">Jobs that run the Access for Web server (HTTP instance <a href=\"{}\">{}</a>) and the host servers serving your session.</p>",
        esc(&httpadmin(rq, &format!("/instance/{}/general", crate::SELF_INSTANCE))),
        crate::SELF_INSTANCE
    );
    body.push_str(&list_table(&List {
        cols: ["Actions", "Job name", "User", "Number", "Type", "Status", "Subsystem", "Function", "Serving"]
            .iter()
            .map(|c| Col::plain(c))
            .collect(),
        rows,
        empty: "No server jobs are active.".into(),
        keep: Vec::new(),
    }));
    rq.page(Page {
        title: "Server jobs".into(),
        crumb: vec!["Jobs".into(), "Server jobs".into()],
        h1: "Server jobs".into(),
        body,
        help: "jobs".into(),
        nav: Some((GROUP, Some("/jobs/server"))),
        ..Page::default()
    })
}

/// `/jobs/server/<number>/<user>/<name>/end`: the policy and `*JOBCTL`
/// first, then Navigator's End Job confirmation.
fn end_server(rq: &Rq<'_>, number: &str, user: &str, name: &str) -> Response {
    let crumb = &["Jobs", "Server jobs", "End"];
    if let Err(r) = rq.policy(policy::JOBS_END_SERVER, crumb) {
        return r;
    }
    if let Err(r) = rq.require(Need::JobCtl, "End server jobs", ("QSYS", "ENDJOB", "*CMD"), crumb) {
        return r;
    }
    let jobs = rq.env().jobs.server_jobs(rq.user());
    let Some(j) = jobs.iter().find(|j| j.number == number && j.user.eq_ignore_ascii_case(user) && j.name.eq_ignore_ascii_case(name)) else {
        return rq.done("/jobs/server", rq.message(Kind::Error, "CPF1070", &[&format!("{number}/{user}/{name}")]));
    };
    Response::redirect(&job_page(rq, j, "end"))
}
