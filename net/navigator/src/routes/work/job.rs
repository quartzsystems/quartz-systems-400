// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/job/<number>/<user>/<name>/...`: a job's properties (`DSPJOB`) by
//! tab, General's Apply as `CHGJOB`, and its actions: Hold (`HLDJOB`),
//! Release (`RLSJOB`) and End (`ENDJOB`), each confirmed.

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, buttons, checkbox, form, input, list_table, props,
    props_table, radio, tabs,
};

use super::jobs::status;
use super::{Confirm, confirm_page, confirmed, job_route, mono, need, page, pick, strings};
use crate::env::work::{JobProps, LogEntry};
use crate::env::{Need, Target};
use crate::routes::Rq;
use crate::util::{tenths, thousands};

/// The tabs: label and the route's last segment.
const TABS: &[(&str, &str)] = &[
    ("General", "general"),
    ("Performance", "performance"),
    ("Library list", "liblist"),
    ("Job log", "joblog"),
    ("Spooled files", "splf"),
    ("Locks", "locks"),
    ("Call stack", "callstack"),
];

/// The message logging levels General offers.
const LOGGING: &[&str] = &["4 00 *SECLVL", "4 00 *NOLIST", "0 99 *MSG"];
/// The inquiry replies General offers.
const INQUIRY: &[&str] = &["*RQD", "*DFT", "*SYSRPYL"];

/// Route a job's page: `q` is `number/user/name`.
pub fn route(rq: &Rq<'_>, q: &str, rest: &[&str]) -> Response {
    let Some(p) = rq.env().work.job(rq.user(), q) else {
        return rq.not_found(&format!("job {q}"));
    };
    match rest {
        ["hold"] => act(rq, &p, "Hold", "HLDJOB"),
        ["release"] => act(rq, &p, "Release", "RLSJOB"),
        ["end"] => end(rq, &p),
        ["general"] => general(rq, &p),
        ["performance"] => performance(rq, &p),
        ["liblist"] => liblist(rq, &p),
        ["joblog"] => joblog(rq, &p),
        ["locks"] => locks(rq, &p),
        ["callstack"] => callstack(rq, &p),
        ["splf"] => super::outq::job_files(rq, &p.job.qualified()),
        _ => rq.not_found(&format!("page {} of job {q}", rest.join("/"))),
    }
}

/// The function's authority check: another user's job needs `*JOBCTL`.
fn allowed(rq: &Rq<'_>, p: &JobProps, function: &str, crumb: &[&str]) -> Result<(), Response> {
    let q = p.job.qualified();
    need(
        rq,
        p.job.user == rq.user(),
        Need::JobCtl,
        function,
        &Target::job(&q),
        crumb,
    )
}

/// The confirmation's table row.
fn row(p: &JobProps) -> Vec<String> {
    let j = &p.job;
    vec![
        mono(&j.qualified()),
        esc(&j.user),
        status(&j.status),
        esc(&j.subsystem),
        mono(&j.function),
    ]
}

const COLS: &[&str] = &["Job", "User", "Status", "Subsystem", "Function"];

/// Hold or Release: the confirmation, then the command.
fn act(rq: &Rq<'_>, p: &JobProps, verb: &str, cmd: &str) -> Response {
    let q = p.job.qualified();
    let command = format!("{cmd} JOB({q})");
    let what = format!("{verb} Job");
    if rq.post() {
        if let Err(r) = allowed(rq, p, &what, &["Work management", "Active jobs"]) {
            return r;
        }
        return confirmed(rq, &command, "/jobs");
    }
    confirm_page(
        rq,
        &Confirm {
            what: &what,
            crumb: &["Active jobs"],
            sentence: format!(
                "Press OK to {} the jobs listed below.",
                verb.to_ascii_lowercase()
            ),
            cols: COLS,
            rows: vec![row(p)],
            options: String::new(),
            command,
        },
    )
}

/// `ENDJOB`'s command from the confirmation's fields.
fn endjob(q: &str, f: &qsweb::http::Form) -> String {
    let opt = if f.str("opt") == "*IMMED" {
        "*IMMED"
    } else {
        "*CNTRLD"
    };
    let mut c = format!("ENDJOB JOB({q}) OPTION({opt})");
    if opt == "*CNTRLD" {
        let d: u32 = f
            .str("delay")
            .trim()
            .parse()
            .ok()
            .filter(|d| (1..=99_999).contains(d))
            .unwrap_or(30);
        let _ = write!(c, " DELAY({d})");
    }
    let spl = if f.str("spl") == "*YES" {
        "*YES"
    } else {
        "*NO"
    };
    let _ = write!(c, " SPLFILE({spl})");
    let log = f.str("log").trim().to_ascii_uppercase();
    if !log.is_empty() && log != "*SAME" {
        let _ = write!(c, " LOGLMT({log})");
    }
    let adl = f.str("adl");
    if matches!(adl, "*GRPJOB" | "*ALL") {
        let _ = write!(c, " ADLINTJOBS({adl})");
    }
    c
}

/// `/job/.../end`: Confirm End Job.
fn end(rq: &Rq<'_>, p: &JobProps) -> Response {
    let q = p.job.qualified();
    if rq.post() {
        if let Err(r) = allowed(rq, p, "End job", &["Work management", "Active jobs"]) {
            return r;
        }
        return confirmed(rq, &endjob(&q, &rq.form()), "/jobs");
    }
    let rows = vec![
        Prop::new(
            "How to end",
            format!(
                "{} Controlled (*CNTRLD) - let the job clean up<br>{} Immediate (*IMMED)",
                radio("opt", "*CNTRLD", true),
                radio("opt", "*IMMED", false)
            ),
        )
        .help("endjob"),
        Prop::new(
            "Delay time",
            format!("{} seconds, then end immediately", input("delay", "30", 4)),
        ),
        Prop::new(
            "Delete spooled files",
            format!(
                "{} No (*NO) {} Yes (*YES)",
                radio("spl", "*NO", true),
                radio("spl", "*YES", false)
            ),
        ),
        Prop::new("Maximum log entries", input("log", "*SAME", 8)),
        Prop::new(
            "Additional interactive jobs",
            pick("adl", &strings(&["*NONE", "*GRPJOB", "*ALL"]), "*NONE"),
        ),
    ];
    let options = props_table(rq.ctx.product, &[PropGroup { title: None, rows }], false);
    confirm_page(
        rq,
        &Confirm {
            what: "End Job",
            crumb: &["Active jobs"],
            sentence: "Press OK to end the jobs listed below.".into(),
            cols: COLS,
            rows: vec![row(p)],
            options,
            command: endjob(&q, &qsweb::http::Form::new(Vec::new())),
        },
    )
}

/// The tab row with `current` bold.
fn tab_row(rq: &Rq<'_>, q: &str, current: usize) -> String {
    let r = job_route(q);
    let t: Vec<(&str, String)> = TABS
        .iter()
        .map(|(l, s)| (*l, rq.ctx.url(&format!("{r}/{s}"))))
        .collect();
    tabs(&t, current)
}

/// A tab's page.
fn tab_page(rq: &Rq<'_>, p: &JobProps, current: usize, body: &str) -> Response {
    let q = p.job.qualified();
    let h1 = format!("Job: {q}");
    let mut crumb = vec!["Active jobs", q.as_str()];
    if current > 0 {
        crumb.push(TABS[current].0);
    }
    let body = format!("{}{body}", tab_row(rq, &q, current));
    page(rq, &crumb, &h1, None, body, "job", None)
}

/// A job type's name.
fn kind(k: &str) -> String {
    let name = match k {
        "BCH" => "Batch",
        "BCI" => "Batch immediate",
        "INT" => "Interactive",
        "PJ" => "Prestart",
        "SBS" => "Subsystem monitor",
        "SYS" => "System",
        "ASJ" => "Autostart",
        "WTR" => "Writer",
        "RDR" => "Reader",
        "SPLW" => "Spool writer",
        _ => return esc(k),
    };
    format!("{name} ({k})")
}

/// A select whose current value is offered even when not among `opts`.
fn choice(name: &str, opts: &[&str], current: &str) -> String {
    let mut v = strings(opts);
    if !v.iter().any(|o| o == current) && !current.is_empty() {
        v.push(current.to_string());
    }
    pick(name, &v, current)
}

/// `/job/.../general`; Apply and OK run `CHGJOB` with what changed.
#[allow(clippy::too_many_lines)]
fn general(rq: &Rq<'_>, p: &JobProps) -> Response {
    let q = p.job.qualified();
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/jobs");
        }
        let back = if f.has("ok") {
            "/jobs".to_string()
        } else {
            format!("{}/general", job_route(&q))
        };
        let mut c = format!("CHGJOB JOB({q})");
        let pty = f.str("pty").trim();
        if !pty.is_empty() && pty != p.run_priority.to_string() {
            let _ = write!(c, " RUNPTY({pty})");
        }
        let ts = f.str("ts").trim();
        if !ts.is_empty() && ts != p.time_slice.to_string() {
            let _ = write!(c, " TIMESLICE({ts})");
        }
        let lvl = f.str("lvl");
        if !lvl.is_empty() && lvl != p.logging {
            let _ = write!(c, " LOG({lvl})");
        }
        if f.has("logcl") != p.log_cl {
            let _ = write!(
                c,
                " LOGCLPGM({})",
                if f.has("logcl") { "*YES" } else { "*NO" }
            );
        }
        let inq = f.str("inq");
        if !inq.is_empty() && inq != p.inquiry {
            let _ = write!(c, " INQMSGRPY({inq})");
        }
        if !c.contains(") ") {
            return rq.redirect(&back);
        }
        if let Err(r) = allowed(rq, p, "Change job", &["Work management", "Active jobs", &q]) {
            return r;
        }
        return super::run(rq, &c, &back);
    }
    let j = &p.job;
    let (ed, et) = rq.stamp(p.entered);
    let (sd, st) = rq.stamp(p.started);
    let submitted = if p.submitted_by.is_empty() {
        String::new()
    } else if p.submitted_how.is_empty() {
        mono(&p.submitted_by)
    } else {
        format!("{} ({})", mono(&p.submitted_by), esc(&p.submitted_how))
    };
    let sbs = if j.subsystem.is_empty() {
        String::new()
    } else {
        format!(
            "<a href=\"{}\">{}</a>",
            esc(&rq.ctx.url("/sbs")),
            esc(&j.subsystem)
        )
    };
    let groups = [
        PropGroup {
            title: Some("Job".into()),
            rows: vec![
                Prop::new("Job name", mono(&j.name)),
                Prop::new("User", mono(&j.user)),
                Prop::new("Number", mono(&j.number)),
                Prop::new("Type", kind(&j.kind)),
                Prop::new("Status", status(&j.status)),
                Prop::new("Subsystem", sbs),
                Prop::new("Function", mono(&j.function)),
                Prop::new("Current user", mono(&j.current_user)),
            ],
        },
        PropGroup {
            title: Some("Entered system".into()),
            rows: vec![
                Prop::new("Date/time entered", format!("{ed} {et}")),
                Prop::new("Date/time started", format!("{sd} {st}")),
                Prop::new("Submitted by", submitted),
            ],
        },
        PropGroup {
            title: Some("Attributes".into()),
            rows: vec![
                Prop::new("Job description", mono(&p.jobd)),
                Prop::new("Job queue", mono(&p.jobq)),
                Prop::new("Output queue", mono(&p.outq)),
                Prop::new("Run priority", input("pty", &p.run_priority.to_string(), 3)),
                Prop::new(
                    "Time slice",
                    format!("{} ms", input("ts", &p.time_slice.to_string(), 6)),
                ),
                Prop::new("Message logging", choice("lvl", LOGGING, &p.logging)),
                Prop::new("Log CL program commands", checkbox("logcl", p.log_cl)),
                Prop::new("Inquiry message reply", choice("inq", INQUIRY, &p.inquiry)),
                Prop::new("CCSID", esc(&p.ccsid)),
                Prop::new(
                    "Language / country",
                    format!("{} / {}", esc(&p.language), esc(&p.country)),
                ),
            ],
        },
    ];
    let body = props(
        &rq.ctx,
        &groups,
        &[
            Button::new("apply", "Apply"),
            Button::new("ok", "OK"),
            Button::new("cancel", "Cancel"),
        ],
    );
    tab_page(rq, p, 0, &body)
}

/// `H:MM:SS` of a span of seconds.
fn hms(s: u64) -> String {
    format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
}

/// `/job/.../performance`; Refresh starts a new interval.
fn performance(rq: &Rq<'_>, p: &JobProps) -> Response {
    let q = p.job.qualified();
    if rq.post() {
        if rq.form().has("ok") {
            return rq.redirect("/jobs");
        }
        rq.env().work.active_jobs(rq.user(), true);
        return rq.redirect(&format!("{}/performance", job_route(&q)));
    }
    let f = &p.perf;
    let n = |v: u64| thousands(v);
    let groups = [
        PropGroup {
            title: Some("Since job started".into()),
            rows: vec![
                Prop::new(
                    "CPU time",
                    format!("{}.{:03} s", f.cpu_ms / 1000, f.cpu_ms % 1000),
                ),
                Prop::new("Elapsed time", hms(f.elapsed)),
                Prop::new(
                    "Disk I/O (async / sync)",
                    format!("{} / {}", n(f.io_async), n(f.io_sync)),
                ),
                Prop::new("Page faults", n(f.faults)),
                Prop::new("Threads", f.threads.to_string()),
                Prop::new(
                    "Temporary storage",
                    format!("{} MB (peak {} MB)", n(f.temp_mb), n(f.temp_peak_mb)),
                ),
            ],
        },
        PropGroup {
            title: Some(format!("Elapsed interval ({} s)", f.interval)),
            rows: vec![
                Prop::new("CPU %", tenths(f.interval_cpu)),
                Prop::new("Disk I/O", n(f.interval_io)),
                Prop::new("Transactions", n(f.transactions)),
                Prop::new(
                    "Response time",
                    f.response
                        .map_or("n/a".into(), |r| format!("{} s", tenths(r))),
                ),
            ],
        },
        PropGroup {
            title: Some("Pool".into()),
            rows: vec![
                Prop::new("Memory pool", format!("{} ({})", f.pool, esc(&f.pool_name))),
                Prop::new("Activity level", f.activity.to_string()),
                Prop::new("Pool size", format!("{} MB", n(f.pool_mb))),
            ],
        },
    ];
    let body = props(
        &rq.ctx,
        &groups,
        &[Button::new("refresh", "Refresh"), Button::new("ok", "OK")],
    );
    tab_page(rq, p, 1, &body)
}

/// `/job/.../liblist`.
fn liblist(rq: &Rq<'_>, p: &JobProps) -> Response {
    let libs = rq.env().work.library_list(rq.user(), &p.job.qualified());
    let list = List {
        cols: ["Sequence", "Library", "Type", "Text"]
            .iter()
            .map(|c| Col::plain(c))
            .collect(),
        rows: libs
            .iter()
            .map(|l| {
                vec![
                    Cell::num(&l.seq.map(|s| s.to_string()).unwrap_or_default()),
                    Cell::mono(&l.lib),
                    Cell::text(&l.kind),
                    Cell::text(&l.text),
                ]
            })
            .collect(),
        empty: "The job has no library list.".into(),
        keep: Vec::new(),
    };
    // Only the job itself changes its library list (CHGLIBL); Navigator
    // runs in another job.
    let bs = buttons(&[
        Button::bare("Add library...").disabled(),
        Button::bare("Remove").disabled(),
        Button::bare("Change current library...").disabled(),
    ]);
    let body = format!("{}{}", list_table(&list), form(&rq.ctx, "", &bs));
    tab_page(rq, p, 2, &body)
}

/// What the job log's Show select keeps.
fn shown(e: &LogEntry, lvl: &str) -> bool {
    match lvl {
        "Severity 10 and above" => e.severity >= 10,
        "Severity 20 and above" => e.severity >= 20,
        "Escape and diagnostic only" => matches!(e.kind.as_str(), "Escape" | "Diagnostic"),
        _ => true,
    }
}

fn pad(s: &str, w: usize) -> String {
    format!("{s:<w$}")
}

/// `/job/.../joblog?lvl=&n=&print=&dsp=`: the job log as `DSPJOBLOG`
/// prints it, message IDs linked.
fn joblog(rq: &Rq<'_>, p: &JobProps) -> Response {
    const LEVELS: &[&str] = &[
        "All messages",
        "Severity 10 and above",
        "Severity 20 and above",
        "Escape and diagnostic only",
    ];
    const LAST: &[&str] = &["100", "200", "500", "All"];
    let q = rq.query();
    let qual = p.job.qualified();
    let lvl = q
        .get("lvl")
        .filter(|l| LEVELS.contains(l))
        .unwrap_or(LEVELS[0])
        .to_string();
    let n = q
        .get("n")
        .filter(|l| LAST.contains(l))
        .unwrap_or("200")
        .to_string();
    let second = q.has("dsp");
    let mut printed = None;
    if q.has("print") {
        let m = match rq
            .env()
            .work
            .run(rq.user(), &format!("DSPJOBLOG JOB({qual}) OUTPUT(*PRINT)"))
        {
            Ok(m) | Err(m) => m,
        };
        printed = Some(m);
    }
    let all = rq.env().work.job_log(rq.user(), &qual);
    let mut entries: Vec<&LogEntry> = all.iter().filter(|e| shown(e, &lvl)).collect();
    if let Ok(last) = n.parse::<usize>() {
        let skip = entries.len().saturating_sub(last);
        entries.drain(..skip);
    }
    let (jl, jn) = p.jobd.split_once('/').unwrap_or(("", &p.jobd));
    let mut out = format!(
        "Job name . . . . . . . . . . :   {}User  . . . . . . :   {}Number . . . . . . . . . . . :   {}\nJob description  . . . . . . :   {}Library . . . . . :   {jl}\n\nMSGID      TYPE                    SEV  DATE      TIME             FROM PGM     LIBRARY     INST     TO PGM      LIBRARY     INST\n",
        pad(&p.job.name, 16),
        pad(&p.job.user, 13),
        p.job.number,
        pad(jn, 16),
    );
    let indent = " ".repeat(37);
    for e in &entries {
        let id = format!(
            "<a href=\"{}\" target=\"_blank\">{}</a>",
            esc(&rq.ctx.product.msg(&e.id)),
            esc(&e.id)
        );
        let line = format!(
            "{}{}{:02}   {}{}{}{}{}{}{}{}",
            " ".repeat(11usize.saturating_sub(e.id.len())),
            pad(&e.kind, 24),
            e.severity,
            pad(&e.date, 10),
            pad(&e.time, 17),
            pad(&e.from.0, 13),
            pad(&e.from.1, 12),
            pad(&e.from.2, 9),
            pad(&e.to.0, 12),
            pad(&e.to.1, 12),
            e.to.2
        );
        let _ = writeln!(out, "{id}{}", esc(&line));
        let _ = writeln!(out, "{indent}Message . . . . :   {}", esc(&e.text));
        if second && let Some(sl) = rq.env().system.message(&e.id, &[]) {
            let _ = writeln!(out, "{indent}Cause . . . . . :   {}", esc(&sl.cause));
            let _ = writeln!(out, "{indent}Recovery  . . . :   {}", esc(&sl.recovery));
        }
    }
    let count = entries.len();
    let form = format!(
        "<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr><td>Show:</td><td>{}</td><td>Last:</td><td>{}</td><td><input type=\"submit\" value=\"Refresh\"> <input type=\"submit\" name=\"print\" value=\"Print job log\"> <input type=\"submit\" name=\"dsp\" value=\"Display second level\"></td></tr></table></form>",
        pick("lvl", &strings(LEVELS), &lvl),
        select_any("n", LAST, &n),
    );
    let msgs = printed
        .map(|m| qsweb::partials::message_row(rq.ctx.product, &m))
        .unwrap_or_default();
    let body = format!(
        "{msgs}{form}<pre class=\"out\">{out}</pre><p class=\"small grey\">{count} {}. Message IDs link to second-level text.</p>",
        if count == 1 { "message" } else { "messages" }
    );
    tab_page(rq, p, 3, &body)
}

/// A select with `chosen` marked wherever it is.
fn select_any(name: &str, opts: &[&str], chosen: &str) -> String {
    qsweb::partials::select(name, opts, chosen)
}

/// `/job/.../locks`.
fn locks(rq: &Rq<'_>, p: &JobProps) -> Response {
    let locks = rq.env().work.locks(rq.user(), &p.job.qualified());
    let list = List {
        cols: [
            "Object", "Library", "Type", "Lock", "Status", "Scope", "Thread",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows: locks
            .iter()
            .map(|l| {
                vec![
                    Cell::mono(&l.object),
                    Cell::mono(&l.lib),
                    Cell::mono(&l.typ),
                    Cell::mono(&l.lock),
                    Cell::text(&l.status),
                    Cell::text(&l.scope),
                    Cell::text(&l.thread),
                ]
            })
            .collect(),
        empty: "The job holds no locks.".into(),
        keep: Vec::new(),
    };
    tab_page(rq, p, 5, &list_table(&list))
}

/// `/job/.../callstack?thr=`: one thread's call stack.
fn callstack(rq: &Rq<'_>, p: &JobProps) -> Response {
    let threads = rq.env().work.call_stack(rq.user(), &p.job.qualified());
    let q = rq.query();
    let labels: Vec<String> = threads
        .iter()
        .map(|t| {
            if t.initial {
                format!("{} (initial)", t.id)
            } else {
                t.id.clone()
            }
        })
        .collect();
    let want = q.str("thr");
    let at = labels.iter().position(|l| l == want).unwrap_or(0);
    let frames = threads
        .get(at)
        .map(|t| t.frames.clone())
        .unwrap_or_default();
    let chosen = labels.get(at).cloned().unwrap_or_default();
    let sel = format!(
        "<form method=\"get\"><p class=\"small\">Thread: {} <input type=\"submit\" value=\"Go\"></p></form>",
        pick("thr", &labels, &chosen)
    );
    let list = List {
        cols: [
            "Level",
            "Program",
            "Library",
            "Module",
            "Procedure",
            "Statement",
            "Activation group",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows: frames
            .iter()
            .enumerate()
            .map(|(i, f)| {
                vec![
                    Cell::num(&(i + 1).to_string()),
                    Cell::mono(&f.program),
                    Cell::mono(&f.lib),
                    Cell::mono(&f.module),
                    Cell::mono(&f.procedure),
                    Cell::mono(&f.statement),
                    Cell::mono(&f.actgrp),
                ]
            })
            .collect(),
        empty: "The thread has no call stack entries.".into(),
        keep: Vec::new(),
    };
    tab_page(rq, p, 6, &format!("{sel}{}", list_table(&list)))
}
