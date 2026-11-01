// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/sbs`: the subsystems (`WRKSBS`), Start (`STRSBS`) and End
//! (`ENDSBS`, confirmed), and a description's tabs (`DSPSBSD`): General
//! (`CHGSBSD`), Pools (`CHGSBSD POOLS`), Routing entries (`ADDRTGE`,
//! `CHGRTGE`, `RMVRTGE`), Autostart jobs (`ADDAJE`, `CHGAJE`, `RMVAJE`),
//! Workstation entries and Prestart jobs.

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, input, input_cmd, list_table,
    props, props_table, radio, tabs,
};

use super::{
    Confirm, confirm_page, confirmed, job_route, mono, nomax, page, pick, quote, run, word,
};
use crate::env::work::{Subsystem, SubsystemDesc};
use crate::env::{Need, Target};
use crate::routes::Rq;

/// The tabs: label and route segment.
const TABS: &[(&str, &str)] = &[
    ("General", "general"),
    ("Pools", "pools"),
    ("Routing entries", "routing"),
    ("Autostart jobs", "autostart"),
    ("Workstation entries", "workstation"),
    ("Prestart jobs", "prestart"),
];

/// Route `/sbs/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    let [name, more @ ..] = rest else {
        return list(rq);
    };
    let Some(d) = rq.env().work.subsystem(rq.user(), name) else {
        return rq.not_found(&format!("subsystem {name}"));
    };
    match more {
        ["start"] => start(rq, &d),
        ["end"] => end(rq, &d),
        ["general"] => general(rq, &d),
        ["pools"] => pools(rq, &d),
        ["routing"] => routing(rq, &d),
        ["autostart"] => autostart(rq, &d),
        ["routing" | "autostart", "confirm"] => remove(rq, &d, more[0]),
        ["workstation"] => workstation(rq, &d),
        ["prestart"] => prestart(rq, &d),
        _ => rq.not_found(&format!("page {} of subsystem {name}", more.join("/"))),
    }
}

fn status(active: bool) -> String {
    if active {
        word("Active", Some(true))
    } else {
        word("Inactive", Some(false))
    }
}

fn qual(s: &Subsystem) -> String {
    format!("{}/{}", s.lib, s.name)
}

/// `/sbs?sort=`; `?start=1` adds the Start subsystem form. The buttons
/// post here.
#[allow(clippy::too_many_lines)]
fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("strsbs") {
            let sbsd = f.str("sbsd").trim().to_ascii_uppercase();
            let t = Target::obj("QSYS", &sbsd, "*SBSD");
            if let Err(r) = rq.require(
                Need::JobCtl,
                "Start subsystem",
                &t,
                &["Work management", "Subsystems"],
            ) {
                return r;
            }
            return run(rq, &format!("STRSBS SBSD({sbsd})"), "/sbs");
        }
        return rq.redirect(if f.has("start") {
            "/sbs?start=1"
        } else {
            "/sbs"
        });
    }
    let q = rq.query();
    let key = q.str("sort");
    let mut all = rq.env().work.subsystems(rq.user());
    match key {
        "name" => all.sort_by(|a, b| a.name.cmp(&b.name)),
        "lib" => all.sort_by(|a, b| a.lib.cmp(&b.lib)),
        "st" => all.sort_by_key(|s| !s.active),
        "jobs" => all.sort_by_key(|s| std::cmp::Reverse(s.jobs)),
        _ => {}
    }
    let rows = all
        .iter()
        .map(|s| {
            let u = |t: &str| rq.ctx.url(&format!("/sbs/{}/{t}", s.name));
            let acts = if s.active {
                actions(&[
                    (u("end"), "End"),
                    (u("general"), "Properties"),
                    (rq.ctx.url(&format!("/jobs?sbs={}", s.name)), "Jobs"),
                ])
            } else {
                actions(&[(u("start"), "Start"), (u("general"), "Properties")])
            };
            vec![
                Cell::act(acts),
                Cell::mono(&s.name),
                Cell::mono(&s.lib),
                Cell::html(status(s.active)),
                Cell::num(&s.jobs.to_string()),
                Cell::mono(&s.pools.join(", ")),
                Cell::num(&nomax(s.max_jobs)),
                Cell::text(&s.text),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Subsystem", "name"),
            Col::sorted("Library", "lib"),
            Col::sorted("Status", "st"),
            Col::sorted("Active jobs", "jobs"),
            Col::plain("Pools"),
            Col::plain("Max jobs"),
            Col::plain("Description"),
        ],
        rows,
        empty: "No subsystem descriptions.".into(),
        keep: Vec::new(),
    };
    let mut body = list_table(&list);
    body.push_str(&form(
        &rq.ctx,
        "",
        &buttons(&[
            Button::new("start", "Start subsystem..."),
            Button::new("refresh", "Refresh"),
        ]),
    ));
    if q.has("start") {
        let rows = vec![Prop::new("Subsystem description", input_cmd("sbsd", "", 21)).required()];
        let _ = write!(
            body,
            "<h2>Start subsystem</h2>{}",
            form(
                &rq.ctx,
                "",
                &format!(
                    "{}{}",
                    props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
                    buttons(&[
                        Button::new("strsbs", "Start"),
                        Button::new("cancel", "Cancel")
                    ])
                )
            )
        );
    }
    page(
        rq,
        &["Subsystems"],
        "Subsystems",
        Some("Subsystem descriptions and their status (WRKSBS)."),
        body,
        "sbs",
        Some("/sbs"),
    )
}

fn confirm_row(d: &SubsystemDesc) -> Vec<String> {
    vec![
        mono(&qual(&d.sbs)),
        status(d.sbs.active),
        d.sbs.jobs.to_string(),
        esc(&d.sbs.text),
    ]
}

const CONFIRM_COLS: &[&str] = &["Subsystem", "Status", "Active jobs", "Description"];

/// `/sbs/<name>/start`: Confirm Start Subsystem.
fn start(rq: &Rq<'_>, d: &SubsystemDesc) -> Response {
    let command = format!("STRSBS SBSD({})", qual(&d.sbs));
    if rq.post() {
        let t = Target::obj(&d.sbs.lib, &d.sbs.name, "*SBSD");
        if let Err(r) = rq.require(
            Need::JobCtl,
            "Start subsystem",
            &t,
            &["Work management", "Subsystems"],
        ) {
            return r;
        }
        return confirmed(rq, &command, "/sbs");
    }
    confirm_page(
        rq,
        &Confirm {
            what: "Start Subsystem",
            crumb: &["Subsystems"],
            sentence: "Press OK to start the subsystems listed below.".into(),
            cols: CONFIRM_COLS,
            rows: vec![confirm_row(d)],
            options: String::new(),
            command,
        },
    )
}

fn endsbs(name: &str, f: &qsweb::http::Form) -> String {
    let opt = if f.str("opt") == "*IMMED" {
        "*IMMED"
    } else {
        "*CNTRLD"
    };
    let mut c = format!("ENDSBS SBS({name}) OPTION({opt})");
    if opt == "*CNTRLD" {
        match f.str("delay").trim().parse::<u32>() {
            Ok(d) if (1..=99_999).contains(&d) => {
                let _ = write!(c, " DELAY({d})");
            }
            _ => c.push_str(" DELAY(*NOLIMIT)"),
        }
    }
    c
}

/// `/sbs/<name>/end`: Confirm End Subsystem.
fn end(rq: &Rq<'_>, d: &SubsystemDesc) -> Response {
    let name = &d.sbs.name;
    if rq.post() {
        let t = Target::obj(&d.sbs.lib, name, "*SBSD");
        if let Err(r) = rq.require(
            Need::JobCtl,
            "End subsystem",
            &t,
            &["Work management", "Subsystems"],
        ) {
            return r;
        }
        return confirmed(rq, &endsbs(name, &rq.form()), "/sbs");
    }
    let rows = vec![
        Prop::new(
            "How to end",
            format!(
                "{} Controlled (*CNTRLD) - let the jobs end themselves<br>{} Immediate (*IMMED)",
                radio("opt", "*CNTRLD", true),
                radio("opt", "*IMMED", false)
            ),
        ),
        Prop::new(
            "Delay time",
            format!("{} seconds (blank: no limit)", input("delay", "", 6)),
        ),
    ];
    confirm_page(
        rq,
        &Confirm {
            what: "End Subsystem",
            crumb: &["Subsystems"],
            sentence: format!(
                "Ending a subsystem ends every job in it ({} active now). Press OK to end the subsystems listed below, or Cancel to return without ending.",
                d.sbs.jobs
            ),
            cols: CONFIRM_COLS,
            rows: vec![confirm_row(d)],
            options: props_table(rq.ctx.product, &[PropGroup { title: None, rows }], false),
            command: endsbs(name, &qsweb::http::Form::new(Vec::new())),
        },
    )
}

/// A tab's page.
fn tab_page(rq: &Rq<'_>, d: &SubsystemDesc, current: usize, body: &str) -> Response {
    let name = &d.sbs.name;
    let t: Vec<(&str, String)> = TABS
        .iter()
        .map(|(l, s)| (*l, rq.ctx.url(&format!("/sbs/{name}/{s}"))))
        .collect();
    let h1 = format!("Subsystem: {name}");
    let mut crumb = vec!["Subsystems", name.as_str()];
    if current > 0 {
        crumb.push(TABS[current].0);
    }
    let body = format!("{}{body}", tabs(&t, current));
    page(rq, &crumb, &h1, None, body, "sbs-props", None)
}

/// `/sbs/<name>/general`; Apply and OK run `CHGSBSD` with what changed.
fn general(rq: &Rq<'_>, d: &SubsystemDesc) -> Response {
    let s = &d.sbs;
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/sbs");
        }
        let back = if f.has("ok") {
            "/sbs".to_string()
        } else {
            format!("/sbs/{}/general", s.name)
        };
        let mut c = format!("CHGSBSD SBSD({})", qual(s));
        let base = c.len();
        let max = f.str("max").trim().to_ascii_uppercase();
        if !max.is_empty() && max != nomax(s.max_jobs) {
            let _ = write!(c, " MAXJOBS({max})");
        }
        let dsp = f.str("dsp").trim().to_ascii_uppercase();
        if !dsp.is_empty() && dsp != d.signon {
            let _ = write!(c, " SGNDSPF({dsp})");
        }
        let txt = f.str("txt").trim();
        if txt != s.text {
            let _ = write!(c, " TEXT({})", quote(txt));
        }
        if c.len() == base {
            return rq.redirect(&back);
        }
        return run(rq, &c, &back);
    }
    let since = d
        .since
        .map(|t| rq.when(t))
        .map_or_else(String::new, |w| format!(" since {w}"));
    let monitor = d.monitor.as_ref().map_or_else(String::new, |m| {
        format!(
            "<a href=\"{}\">{}</a>",
            esc(&rq.ctx.url(&format!("{}/general", job_route(m)))),
            esc(m)
        )
    });
    let queues: Vec<String> = d
        .job_queues
        .iter()
        .map(|q| {
            let lib = q.split('/').next().unwrap_or("");
            format!(
                "<a href=\"{}\">{}</a>",
                esc(&rq.ctx.url(&format!("/jobq?lib={lib}"))),
                esc(q)
            )
        })
        .collect();
    let groups = [
        PropGroup {
            title: Some("Subsystem description".into()),
            rows: vec![
                Prop::new("Name", mono(&s.name)),
                Prop::new("Library", mono(&s.lib)),
                Prop::new("Status", format!("{}{since}", status(s.active))),
                Prop::new("Monitor job", monitor),
                Prop::new("Maximum jobs", input("max", &nomax(s.max_jobs), 8)),
                Prop::new("Sign-on display file", input_cmd("dsp", &d.signon, 20)),
                Prop::new("Text", input("txt", &s.text, 40)),
            ],
        },
        PropGroup {
            title: Some("Current activity".into()),
            rows: vec![
                Prop::new(
                    "Active jobs",
                    format!(
                        "<a href=\"{}\">{}</a>",
                        esc(&rq.ctx.url(&format!("/jobs?sbs={}", s.name))),
                        s.jobs
                    ),
                ),
                Prop::new(
                    "Pools",
                    format!("{} ({})", d.pools.len(), esc(&s.pools.join(", "))),
                ),
                Prop::new("Job queues", queues.join(", ")),
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
    tab_page(rq, d, 0, &body)
}

/// `/sbs/<name>/pools?edit=add|change|remove`.
#[allow(clippy::too_many_lines)]
fn pools(rq: &Rq<'_>, d: &SubsystemDesc) -> Response {
    let s = &d.sbs;
    let here = format!("/sbs/{}/pools", s.name);
    if rq.post() {
        let f = rq.form();
        for op in ["add", "change", "remove"] {
            if f.has(op) {
                return rq.redirect(&format!("{here}?edit={op}"));
            }
        }
        if f.has("cancel") {
            return rq.redirect(&here);
        }
        let id = f.str("id").trim();
        let c = if f.has("rmvpool") {
            format!("CHGSBSD SBSD({}) POOLS(({id} *RMV))", qual(s))
        } else {
            let storage = f.str("storage").trim().to_ascii_uppercase();
            let act = f.str("act").trim();
            format!("CHGSBSD SBSD({}) POOLS(({id} {storage} {act}))", qual(s)).replace(" )", ")")
        };
        return run(rq, &c, &here);
    }
    let rows = d
        .pools
        .iter()
        .map(|p| {
            vec![
                Cell::num(&p.id.to_string()),
                Cell::mono(&p.storage),
                Cell::text(&p.activity.map(|a| a.to_string()).unwrap_or_default()),
                Cell::text(&p.system_pool.to_string()),
                Cell::num(&crate::util::thousands(p.size_mb)),
                Cell::num(&crate::util::thousands(p.reserved_mb)),
                Cell::num(&crate::util::tenths(p.faults)),
                Cell::num(&crate::util::tenths(p.pages)),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Pool ID",
            "Storage",
            "Activity level",
            "System pool",
            "Size (MB)",
            "Reserved (MB)",
            "Faults/s",
            "Pages/s",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows,
        empty: "The subsystem defines no pools.".into(),
        keep: Vec::new(),
    };
    let mut body = list_table(&list);
    body.push_str(&form(
        &rq.ctx,
        "",
        &buttons(&[
            Button::new("add", "Add pool..."),
            Button::new("change", "Change..."),
            Button::new("remove", "Remove"),
        ]),
    ));
    let ids: Vec<String> = d.pools.iter().map(|p| p.id.to_string()).collect();
    let edit = rq.query().str("edit").to_string();
    let first = d.pools.first();
    let (rows, bs, title) = match edit.as_str() {
        "add" | "change" => {
            let next = d.pools.iter().map(|p| p.id).max().unwrap_or(0) + 1;
            let (id, storage, act) = if edit == "change" {
                first.map_or((String::new(), String::new(), String::new()), |p| {
                    (
                        p.id.to_string(),
                        p.storage.clone(),
                        p.activity.map(|a| a.to_string()).unwrap_or_default(),
                    )
                })
            } else {
                (next.to_string(), "*SHRPOOL1".to_string(), String::new())
            };
            let id_field = if edit == "change" {
                pick("id", &ids, &id)
            } else {
                input("id", &id, 3)
            };
            (
                vec![
                    Prop::new("Pool ID", id_field).required(),
                    Prop::new("Storage", input_cmd("storage", &storage, 12)).required(),
                    Prop::new("Activity level", input("act", &act, 6)),
                ],
                vec![
                    Button::new("apply", "Apply"),
                    Button::new("cancel", "Cancel"),
                ],
                if edit == "add" {
                    "Add pool"
                } else {
                    "Change pool"
                },
            )
        }
        "remove" => (
            vec![Prop::new("Pool ID", pick("id", &ids, ""))],
            vec![
                Button::new("rmvpool", "Remove"),
                Button::new("cancel", "Cancel"),
            ],
            "Remove pool",
        ),
        _ => (Vec::new(), Vec::new(), ""),
    };
    if !rows.is_empty() {
        let _ = write!(
            body,
            "<h2>{title}</h2>{}",
            form(
                &rq.ctx,
                "",
                &format!(
                    "{}{}",
                    props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
                    buttons(&bs)
                )
            )
        );
    }
    tab_page(rq, d, 1, &body)
}

/// `/sbs/<name>/routing?seq=`: the entries, with the form for one when
/// `seq` names it (or `new`).
#[allow(clippy::too_many_lines)]
fn routing(rq: &Rq<'_>, d: &SubsystemDesc) -> Response {
    let s = &d.sbs;
    let here = format!("/sbs/{}/routing", s.name);
    if rq.post() {
        let f = rq.form();
        let op = f.str("op");
        if op.is_empty() {
            return rq.redirect(&format!("{here}?seq=new"));
        }
        if f.has("cancel") {
            return rq.redirect(&here);
        }
        let cmd = if op == "add" { "ADDRTGE" } else { "CHGRTGE" };
        let max = f.str("max").trim().to_ascii_uppercase();
        let c = format!(
            "{cmd} SBSD({}) SEQNBR({}) CMPVAL({} {}) PGM({}) CLS({}) MAXACT({}) POOLID({})",
            qual(s),
            f.str("seq").trim(),
            quote(f.str("cmp").trim()),
            f.str("start").trim(),
            f.str("pgm").trim().to_ascii_uppercase(),
            f.str("cls").trim().to_ascii_uppercase(),
            if max.is_empty() { "*NOMAX" } else { &max },
            f.str("pool").trim()
        );
        return run(rq, &c, &here);
    }
    let rows = d
        .routing
        .iter()
        .map(|r| {
            vec![
                Cell::act(actions(&[
                    (rq.ctx.url(&format!("{here}?seq={}", r.seq)), "Change"),
                    (
                        rq.ctx.url(&format!("{here}/confirm?seq={}", r.seq)),
                        "Remove",
                    ),
                ])),
                Cell::num(&r.seq.to_string()),
                Cell::mono(&r.compare),
                Cell::text(&r.start.to_string()),
                Cell::mono(&r.program),
                Cell::mono(&r.lib),
                Cell::mono(&r.class),
                Cell::text(&nomax(r.max_active)),
                Cell::text(&r.pool.to_string()),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Actions",
            "Seq",
            "Compare value",
            "Start position",
            "Program",
            "Library",
            "Class",
            "Max active",
            "Pool",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows,
        empty: "The subsystem has no routing entries.".into(),
        keep: Vec::new(),
    };
    let mut body = list_table(&list);
    body.push_str(&form(&rq.ctx, "", &buttons(&[Button::bare("Add...")])));
    let seq = rq.query().str("seq").to_string();
    if !seq.is_empty() {
        let found = d.routing.iter().find(|r| r.seq.to_string() == seq);
        let add = found.is_none();
        let r = found.cloned().unwrap_or_else(|| crate::env::work::Routing {
            seq: d
                .routing
                .iter()
                .map(|r| r.seq)
                .filter(|n| *n < 9999)
                .max()
                .unwrap_or(0)
                + 10,
            start: 1,
            program: "QCMD".into(),
            lib: "QSYS".into(),
            class: "QGPL/QBATCH".into(),
            pool: 1,
            ..Default::default()
        });
        let seq_field = if add {
            input("seq", &r.seq.to_string(), 4)
        } else {
            format!(
                "{}<input type=\"hidden\" name=\"seq\" value=\"{}\">",
                r.seq, r.seq
            )
        };
        let rows = vec![
            Prop::new("Sequence number", seq_field).required(),
            Prop::new("Compare value", input_cmd("cmp", &r.compare, 20)),
            Prop::new("Start position", input("start", &r.start.to_string(), 3)),
            Prop::new(
                "Program",
                input_cmd("pgm", &format!("{}/{}", r.lib, r.program), 21),
            )
            .required(),
            Prop::new("Class", input_cmd("cls", &r.class, 21)).required(),
            Prop::new("Maximum active", input("max", &nomax(r.max_active), 8)),
            Prop::new("Pool", input("pool", &r.pool.to_string(), 3)),
        ];
        let _ = write!(
            body,
            "<h2>{}</h2>{}",
            if add {
                "Add routing entry"
            } else {
                "Change routing entry"
            },
            form(
                &rq.ctx,
                "",
                &format!(
                    "<input type=\"hidden\" name=\"op\" value=\"{}\">{}{}",
                    if add { "add" } else { "chg" },
                    props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
                    buttons(&[
                        Button::new("apply", "Apply"),
                        Button::new("cancel", "Cancel")
                    ])
                )
            )
        );
    }
    tab_page(rq, d, 2, &body)
}

/// `/sbs/<name>/autostart?job=`: the entries, with the form for one.
fn autostart(rq: &Rq<'_>, d: &SubsystemDesc) -> Response {
    let s = &d.sbs;
    let here = format!("/sbs/{}/autostart", s.name);
    if rq.post() {
        let f = rq.form();
        let op = f.str("op");
        if op.is_empty() {
            return rq.redirect(&format!("{here}?job=*NEW"));
        }
        if f.has("cancel") {
            return rq.redirect(&here);
        }
        let cmd = if op == "add" { "ADDAJE" } else { "CHGAJE" };
        let c = format!(
            "{cmd} SBSD({}) JOB({}) JOBD({})",
            qual(s),
            f.str("job").trim().to_ascii_uppercase(),
            f.str("jobd").trim().to_ascii_uppercase()
        );
        return run(rq, &c, &here);
    }
    let rows = d
        .autostart
        .iter()
        .map(|a| {
            vec![
                Cell::act(actions(&[
                    (rq.ctx.url(&format!("{here}?job={}", a.job)), "Change"),
                    (
                        rq.ctx.url(&format!("{here}/confirm?job={}", a.job)),
                        "Remove",
                    ),
                ])),
                Cell::mono(&a.job),
                Cell::mono(&a.jobd),
                Cell::mono(&a.lib),
            ]
        })
        .collect();
    let list = List {
        cols: ["Actions", "Job name", "Job description", "Library"]
            .iter()
            .map(|c| Col::plain(c))
            .collect(),
        rows,
        empty: "The subsystem has no autostart job entries.".into(),
        keep: Vec::new(),
    };
    let mut body = list_table(&list);
    body.push_str(&form(&rq.ctx, "", &buttons(&[Button::bare("Add...")])));
    let job = rq.query().str("job").to_string();
    if !job.is_empty() {
        let found = d.autostart.iter().find(|a| a.job == job);
        let add = found.is_none();
        let (name, jobd) = found.map_or((String::new(), String::new()), |a| {
            (a.job.clone(), format!("{}/{}", a.lib, a.jobd))
        });
        let job_field = if add {
            input("job", "", 10)
        } else {
            format!(
                "{}<input type=\"hidden\" name=\"job\" value=\"{}\">",
                mono(&name),
                esc(&name)
            )
        };
        let rows = vec![
            Prop::new("Job name", job_field).required(),
            Prop::new("Job description", input_cmd("jobd", &jobd, 21)).required(),
        ];
        let _ = write!(
            body,
            "<h2>{}</h2>{}",
            if add {
                "Add autostart job entry"
            } else {
                "Change autostart job entry"
            },
            form(
                &rq.ctx,
                "",
                &format!(
                    "<input type=\"hidden\" name=\"op\" value=\"{}\">{}{}",
                    if add { "add" } else { "chg" },
                    props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
                    buttons(&[
                        Button::new("apply", "Apply"),
                        Button::new("cancel", "Cancel")
                    ])
                )
            )
        );
    }
    tab_page(rq, d, 3, &body)
}

/// `/sbs/<name>/{routing,autostart}/confirm?seq=|job=`: Remove an entry.
fn remove(rq: &Rq<'_>, d: &SubsystemDesc, what: &str) -> Response {
    let s = &d.sbs;
    let q = rq.query();
    let back = format!("/sbs/{}/{what}", s.name);
    let (title, command, cols, row): (&str, String, &[&str], Vec<String>) = if what == "routing" {
        let seq = q.str("seq");
        let Some(r) = d.routing.iter().find(|r| r.seq.to_string() == seq) else {
            return rq.not_found(&format!("routing entry {seq} of subsystem {}", s.name));
        };
        (
            "Remove Routing Entry",
            format!("RMVRTGE SBSD({}) SEQNBR({})", qual(s), r.seq),
            &["Subsystem", "Seq", "Compare value", "Program", "Class"],
            vec![
                mono(&qual(s)),
                r.seq.to_string(),
                mono(&r.compare),
                mono(&format!("{}/{}", r.lib, r.program)),
                mono(&r.class),
            ],
        )
    } else {
        let job = q.str("job");
        let Some(a) = d.autostart.iter().find(|a| a.job == job) else {
            return rq.not_found(&format!(
                "autostart job entry {job} of subsystem {}",
                s.name
            ));
        };
        (
            "Remove Autostart Job Entry",
            format!("RMVAJE SBSD({}) JOB({})", qual(s), a.job),
            &["Subsystem", "Job name", "Job description"],
            vec![
                mono(&qual(s)),
                mono(&a.job),
                mono(&format!("{}/{}", a.lib, a.jobd)),
            ],
        )
    };
    if rq.post() {
        return confirmed(rq, &command, &back);
    }
    let tab = if what == "routing" {
        "Routing entries"
    } else {
        "Autostart jobs"
    };
    confirm_page(
        rq,
        &Confirm {
            what: title,
            crumb: &["Subsystems", &s.name, tab],
            sentence: "Press OK to remove the entries listed below from the subsystem description."
                .into(),
            cols,
            rows: vec![row],
            options: String::new(),
            command,
        },
    )
}

/// `/sbs/<name>/workstation`.
fn workstation(rq: &Rq<'_>, d: &SubsystemDesc) -> Response {
    let rows = d
        .workstations
        .iter()
        .map(|w| {
            vec![
                Cell::mono(&w.name),
                Cell::text(if w.by_type { "Type" } else { "Name" }),
                Cell::mono(&w.jobd),
                Cell::text(&nomax(w.max_active)),
                Cell::mono(&w.at),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Workstation",
            "By",
            "Job description",
            "Max active",
            "Allocate",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows,
        empty: "The subsystem has no workstation entries.".into(),
        keep: Vec::new(),
    };
    tab_page(rq, d, 4, &list_table(&list))
}

/// `/sbs/<name>/prestart`.
fn prestart(rq: &Rq<'_>, d: &SubsystemDesc) -> Response {
    let rows = d
        .prestart
        .iter()
        .map(|p| {
            vec![
                Cell::mono(&p.program),
                Cell::mono(&p.lib),
                Cell::mono(&p.user),
                Cell::num(&p.initial.to_string()),
                Cell::num(&p.threshold.to_string()),
                Cell::num(&nomax(p.max_jobs)),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Program",
            "Library",
            "User",
            "Initial jobs",
            "Threshold",
            "Max jobs",
        ]
        .iter()
        .map(|c| Col::plain(c))
        .collect(),
        rows,
        empty: "The subsystem has no prestart job entries.".into(),
        keep: Vec::new(),
    };
    tab_page(rq, d, 5, &list_table(&list))
}
