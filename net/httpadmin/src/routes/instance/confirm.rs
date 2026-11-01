// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/instance/<name>/confirm?op=start|stop|restart|delete`: the
//! confirmation pages (`confirm-stop.html`, `shared/confirm.html`) and
//! what OK does: `STRTCPSVR`, `ENDTCPSVR *CNTRLD` or `*IMMED`, both for
//! a restart, deleting a stopped instance.

use httpd::instance;
use httpd::registry::Registry;
use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Button, Cell, Col, List, buttons, form, list_table};

use super::Rq;
use crate::env::EndHow;
use crate::routes::instances::{View, job_url, status_cell};
use crate::{SELF_INSTANCE, msgs, util};

/// The operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Start,
    Stop,
    Restart,
    Delete,
}

impl Op {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "start" => Some(Op::Start),
            "stop" => Some(Op::Stop),
            "restart" => Some(Op::Restart),
            "delete" => Some(Op::Delete),
            _ => None,
        }
    }

    fn verb(self) -> &'static str {
        match self {
            Op::Start => "Start",
            Op::Stop => "Stop",
            Op::Restart => "Restart",
            Op::Delete => "Delete",
        }
    }

    fn sentence(self) -> &'static str {
        match self {
            Op::Start => "Starting an instance submits its job to subsystem QHTTPSVR, which reads the configuration file and listens on its ports. Press OK to start the instances listed below.",
            Op::Stop => "Stopping an instance ends its job in subsystem QHTTPSVR and closes all client connections. Press OK to stop the instances listed below.",
            Op::Restart => "Restarting an instance ends its job in subsystem QHTTPSVR, closing all client connections, then starts it again with the current configuration file. Press OK to restart the instances listed below.",
            Op::Delete => "Deleting an instance removes its configuration directory and all log files. Press OK to delete the instances listed below, or Cancel to return without deleting.",
        }
    }
}

/// The job cell: a link to Navigator's job page, or `(not active)`.
fn job_cell(v: &View) -> String {
    match &v.job {
        Some(j) => format!("<a href=\"{}\">{}</a>", esc(&job_url(&j.qualified())), esc(&j.qualified())),
        None => "<span class=\"mono\">(not active)</span>".into(),
    }
}

/// The log files under the server root: how many, and their size.
fn log_files(rq: &Rq<'_>, v: &View) -> String {
    let dir = format!("{}/logs", v.entry.root.trim_end_matches('/'));
    let files: Vec<_> = rq.env().files.list(&dir).unwrap_or_default().into_iter().filter(|e| !e.dir).collect();
    let bytes: u64 = files.iter().map(|e| e.size).sum();
    let n = files.len();
    format!("{n} {}, {}", if n == 1 { "file" } else { "files" }, util::size(bytes))
}

/// `GET` shows the confirmation; `POST` with OK acts.
pub fn page(rq: &Rq<'_>, v: &View) -> Response {
    let Some(op) = Op::parse(rq.query().str("op")) else {
        return rq.not_found(&format!("operation {} on an instance", rq.query().str("op")));
    };
    if rq.post() {
        return act(rq, v, op);
    }
    let n = v.name();
    let (cols, row, note, options) = if op == Op::Delete {
        (
            vec!["Instance name", "Status", "Server root", "Job", "Log files"],
            vec![
                Cell::text(n),
                Cell::html(status_cell(v)),
                Cell::html(format!("<span class=\"mono\">{}</span>", esc(&v.entry.root))),
                Cell::html(job_cell(v)),
                Cell::text(&log_files(rq, v)),
            ],
            "<p class=\"small grey\">The instance must be stopped before it can be deleted. Running instances are skipped and message HTP8103 is issued.</p>",
            "",
        )
    } else {
        let conns = v.job.as_ref().map_or(0, |j| j.connections);
        let how = if op == Op::Start {
            ""
        } else {
            "<p class=\"small grey\">How to end: <input type=\"radio\" name=\"how\" value=\"*CNTRLD\" checked> Controlled, wait up to <input type=\"text\" name=\"delay\" value=\"30\" size=\"3\"> seconds for requests to finish &nbsp; <input type=\"radio\" name=\"how\" value=\"*IMMED\"> Immediate</p>"
        };
        (
            vec!["Instance name", "Status", "Job", "Ports", "Active connections"],
            vec![
                Cell::text(n),
                Cell::html(status_cell(v)),
                Cell::html(job_cell(v)),
                Cell::text(&v.port_list()),
                Cell::text(&conns.to_string()),
            ],
            "",
            how,
        )
    };
    let table = list_table(&List {
        cols: cols.into_iter().map(Col::plain).collect(),
        rows: vec![row],
        ..List::default()
    });
    let body = format!(
        "<p>{}</p>{table}{note}{}",
        esc(op.sentence()),
        form(
            &rq.ctx,
            "",
            &format!("{options}{}", buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")]))
        )
    );
    let title = format!("Confirm {} Instance", op.verb());
    rq.page(Page {
        title: title.clone(),
        crumb: vec!["Manage".into(), "Server instances".into(), title.clone()],
        h1: title,
        body,
        help: if op == Op::Delete { "confirm-delete".into() } else { "confirm-instance".into() },
        generic: op == Op::Delete,
        ..Page::default()
    })
}

fn act(rq: &Rq<'_>, v: &View, op: Op) -> Response {
    let form = rq.form();
    if !form.has("ok") {
        return rq.redirect("/instances");
    }
    if let Err(r) = super::may_change(rq, v, &format!("{} HTTP server instance", op.verb())) {
        return r;
    }
    let name = v.name();
    let user = rq.user().to_string();
    let jobs = &rq.env().jobs;
    let msg = match op {
        Op::Delete => delete(rq, v),
        Op::Start => {
            let r = jobs.start(&user, name);
            if r.is_ok() {
                started(rq, name);
            }
            r.unwrap_or_else(|e| e)
        }
        Op::Stop | Op::Restart if name.eq_ignore_ascii_case(SELF_INSTANCE) => {
            msgs::message(Kind::Error, "HTP8104", &[name])
        }
        Op::Stop | Op::Restart => {
            let how = if form.str("how") == "*IMMED" {
                EndHow::Immediate
            } else {
                match form.str("delay").trim().parse::<u32>() {
                    Ok(d) if d <= 3600 => EndHow::Controlled(d),
                    _ => {
                        return rq.done(
                            &format!("/instance/{name}/confirm?op={}", op.verb().to_ascii_lowercase()),
                            super::bad_value(form.str("delay"), "Seconds to wait", "a number of seconds from 0 to 3600"),
                        );
                    }
                }
            };
            match jobs.end(&user, name, how) {
                Ok(m) => {
                    ended(rq, name, how);
                    if op == Op::Restart {
                        let r = jobs.start(&user, name);
                        if r.is_ok() {
                            started(rq, name);
                        }
                        r.unwrap_or_else(|e| e)
                    } else {
                        m
                    }
                }
                Err(e) => e,
            }
        }
    };
    if msg.kind != Kind::Error {
        rq.audit_change(name, &format!("{} instance", op.verb().to_ascii_lowercase()));
    }
    rq.done("/instances", msg)
}

fn started(rq: &Rq<'_>, name: &str) {
    let now = rq.env().system.now();
    let _ = Registry::update(rq.env().files.as_ref(), name, |e| {
        e.last_started = Some(now);
        e.restart_required = false;
    });
}

fn ended(rq: &Rq<'_>, name: &str, how: EndHow) {
    let now = rq.env().system.now();
    let _ = Registry::update(rq.env().files.as_ref(), name, |e| {
        e.last_ended = Some(now);
        e.ended_how = match how {
            EndHow::Controlled(_) => "ENDTCPSVR *HTTP, controlled".into(),
            EndHow::Immediate => "ENDTCPSVR *HTTP, immediate".into(),
        };
    });
}

fn delete(rq: &Rq<'_>, v: &View) -> Message {
    if v.running() {
        return msgs::message(Kind::Error, "HTP8103", &[v.name()]);
    }
    match instance::delete(rq.env().files.as_ref(), v.name()) {
        Ok(()) => msgs::message(Kind::Done, "HTP8021", &[v.name(), &v.entry.root]),
        Err(e) => super::io_error(v, &e.to_string()),
    }
}
