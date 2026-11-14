// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/dirsrv`: the instances with their state, ports and job;
//! `/dirsrv/inst/<name>/confirm?op=start|end|restart` starts, ends or
//! restarts one (`STRTCPSVR` and `ENDTCPSVR SERVER(*DIRSRV)
//! INSTANCE(name)`), confirmed; `/dirsrv/inst/<name>/monitor` the
//! counters of `cn=monitor`.

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, Prop, PropGroup, actions, list_table, mono, props_table};

use super::{GROUP, fail, inst_route, inst_tabs, need, page, top_tabs, when, yes_no};
use crate::env::dirsrv::{InstanceInfo, InstanceState};
use crate::routes::Rq;
use crate::routes::common::{Confirm, confirm_in, word};

fn state(s: InstanceState) -> String {
    match s {
        InstanceState::Running => word("Active", Some(true)),
        InstanceState::Stopped => word("Inactive", Some(false)),
        InstanceState::Starting => word("Starting", None),
        InstanceState::Ending => word("Ending", None),
    }
}

/// `/dirsrv`.
pub fn list(rq: &Rq<'_>) -> Response {
    let list = match rq.env().dirsrv.instances(rq.user()) {
        Ok(l) => l,
        Err(e) => return fail(rq, &e, "Directory server", &[], "/home"),
    };
    let rows = list
        .iter()
        .map(|i| {
            let u = |r: &str| rq.ctx.url(&inst_route(i, r));
            let mut acts = vec![(u("props"), "Properties")];
            if i.state == InstanceState::Running {
                acts.push((u("confirm?op=end"), "End"));
                acts.push((u("confirm?op=restart"), "Restart"));
            } else {
                acts.push((u("confirm?op=start"), "Start"));
            }
            vec![
                Cell::act(actions(&acts)),
                Cell::mono(&i.name),
                Cell::text(&i.desc),
                Cell::html(state(i.state)),
                Cell::mono(&i.job),
                Cell::mono(&format!("{}, {}", i.port, i.secure_port)),
                Cell::mono(&i.security),
                Cell::text(yes_no(i.autostart)),
                Cell::text(&when(rq, i.since)),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Actions",
            "Instance",
            "Description",
            "Status",
            "Job",
            "Ports",
            "Security",
            "Autostart",
            "Started",
        ]
        .iter()
        .map(|l| Col::plain(l))
        .collect(),
        rows,
        empty: "No directory server instances are configured.".into(),
        keep: Vec::new(),
    };
    let body = format!("{}{}", top_tabs(rq, 0), list_table(&list));
    page(
        rq,
        &["Instances"],
        "Directory server instances",
        body,
        "dirsrv",
        true,
    )
}

/// Start, end or restart `i`, confirmed.
pub fn act(rq: &Rq<'_>, i: &InstanceInfo) -> Response {
    let op = rq.query().str("op").to_string();
    let (what, function, command, sentence) = match op.as_str() {
        "start" => (
            "Start Directory Server",
            "Start directory server",
            format!("STRTCPSVR SERVER(*DIRSRV) INSTANCE({})", i.name),
            "The instance's job starts in QSYSWRK and reads its configuration; a value it does not take stops the start with a message. Press OK to start the instance listed below, or Cancel to return.",
        ),
        "end" => (
            "End Directory Server",
            "End directory server",
            format!("ENDTCPSVR SERVER(*DIRSRV) INSTANCE({})", i.name),
            "The instance's job ends and its clients are disconnected. Press OK to end the instance listed below, or Cancel to return.",
        ),
        "restart" => (
            "Restart Directory Server",
            "Restart directory server",
            format!(
                "ENDTCPSVR SERVER(*DIRSRV) INSTANCE({0}) then STRTCPSVR SERVER(*DIRSRV) INSTANCE({0})",
                i.name
            ),
            "The instance ends and starts again, so it reads the changes made to its configuration. Its clients are disconnected. Press OK to restart the instance listed below, or Cancel to return.",
        ),
        _ => return rq.not_found(&format!("action {op}")),
    };
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/dirsrv");
        }
        if let Err(r) = need(rq, Some(i), function, &["Instances"]) {
            return r;
        }
        let d = &rq.env().dirsrv;
        let r = match op.as_str() {
            "start" => d.start(rq.user(), &i.name),
            "end" => d.end(rq.user(), &i.name),
            _ => d.restart(rq.user(), &i.name),
        };
        return match r {
            Ok(()) => rq.redirect("/dirsrv"),
            Err(e) => fail(rq, &e, function, &["Instances"], "/dirsrv"),
        };
    }
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what,
            crumb: &["Directory server", "Instances"],
            sentence: sentence.into(),
            cols: &["Instance", "Description", "Ports"],
            rows: vec![vec![
                mono(&i.name),
                esc(&i.desc),
                mono(&format!("{}, {}", i.port, i.secure_port)),
            ]],
            options: String::new(),
            command,
        },
    )
}

fn num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (k, c) in s.chars().enumerate() {
        if k > 0 && (s.len() - k).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `/dirsrv/inst/<name>/monitor`.
pub fn monitor(rq: &Rq<'_>, i: &InstanceInfo) -> Response {
    let body = match rq.env().dirsrv.counters(rq.user(), &i.name) {
        Ok(c) => {
            let row = |l: &str, v: u64| Prop::new(l, esc(&num(v)));
            let groups = [
                PropGroup {
                    title: Some("Connections".into()),
                    rows: vec![
                        Prop::new("Started", esc(&when(rq, c.started))),
                        row("Open now", u64::from(c.connections)),
                        row("Since started", c.total_connections),
                        row("Persistent searches", u64::from(c.persistent)),
                    ],
                },
                PropGroup {
                    title: Some("Operations".into()),
                    rows: vec![
                        row("Started", c.ops_initiated),
                        row("Completed", c.ops_completed),
                        row("Binds", c.binds),
                        row("Searches", c.searches),
                        row("Entries sent", c.entries_sent),
                        row("Compares", c.compares),
                        row("Adds", c.adds),
                        row("Modifies", c.modifies),
                        row("Deletes", c.deletes),
                        row("Modify DN", c.modrdns),
                        row("Extended operations", c.extended),
                        row("Abandons", c.abandons),
                    ],
                },
            ];
            props_table(rq.ctx.product, &groups, true)
        }
        Err(e) => format!("<p>{}</p>", esc(&format!("{} ({})", e.text, e.msgid))),
    };
    let title = format!("Monitor {}", i.name);
    page(
        rq,
        &["Instances", &i.name, "Monitor"],
        &title,
        format!("{}{body}", inst_tabs(rq, i, 5)),
        "dsmonitor",
        false,
    )
}
