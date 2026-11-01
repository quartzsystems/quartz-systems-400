// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/tcp/servers`: the TCP servers; Start (`STRTCPSVR`), End
//! (`ENDTCPSVR`) and Restart (`STRTCPSVR RESTART()`) of one, confirmed;
//! Start all autostart servers (`STRTCPSVR SERVER(*AUTOSTART)`).
//! `/tcp/servers/<name>`: its autostart attribute (`CHGxxxA
//! AUTOSTART()`). The `*HTTP` row links to HTTP Server Administration
//! on port 2001 of the same host.

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, list_table, mono, props,
};

use super::{GROUP, confirmed, need, page, yes};
use crate::env::net::Server;
use crate::routes::Rq;
use crate::routes::work::{Confirm, confirm_in, pick, reply, strings};

/// Route `/tcp/servers/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    let [name, more @ ..] = rest else {
        return list(rq);
    };
    let full = format!("*{name}");
    let Some(s) = rq
        .env()
        .net
        .servers(rq.user())
        .into_iter()
        .find(|s| s.name == full)
    else {
        return rq.not_found(&format!("server {full}"));
    };
    match more {
        [] => properties(rq, &s),
        ["start"] => act(rq, &s, "start"),
        ["confirm"] => {
            let op = if rq.query().str("op") == "restart" {
                "restart"
            } else {
                "end"
            };
            act(rq, &s, op)
        }
        _ => rq.not_found(&format!("page {} of server {full}", more.join("/"))),
    }
}

/// The server's route.
fn base(s: &Server) -> String {
    format!("/tcp/servers/{}", s.name.trim_start_matches('*'))
}

/// The first five ports, then `...`.
fn ports(s: &Server) -> String {
    let mut p: Vec<String> = s.ports.iter().take(5).map(ToString::to_string).collect();
    if s.ports.len() > 5 {
        p.push("...".into());
    }
    p.join(", ")
}

fn status(s: &Server) -> String {
    if s.started {
        qsweb::partials::status("Started", Some(true))
    } else {
        qsweb::partials::status("Ended", Some(false))
    }
}

/// HTTP Server Administration's instances on port 2001 of the host the
/// browser used.
fn admin_url(rq: &Rq<'_>) -> String {
    let h = rq.req.header("host").unwrap_or("localhost");
    let host = if h.starts_with('[') {
        h.split_inclusive(']').next().unwrap_or(h)
    } else {
        h.split(':').next().unwrap_or(h)
    };
    format!("http://{host}:2001/qs400/httpadmin/instances")
}

/// `/tcp/servers?sort=`; the buttons post here.
fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        if rq.form().has("start") {
            if let Err(r) = need(rq, "Start TCP/IP servers", "STRTCPSVR", "Servers") {
                return r;
            }
            return reply(
                rq,
                rq.env().net.run(rq.user(), "STRTCPSVR SERVER(*AUTOSTART)"),
                "/tcp/servers",
            );
        }
        return rq.redirect("/tcp/servers");
    }
    let mut ss = rq.env().net.servers(rq.user());
    match rq.query().str("sort") {
        "n" => ss.sort_by(|a, b| a.name.cmp(&b.name)),
        "s" => ss.sort_by_key(|s| !s.started),
        _ => {}
    }
    let admin = admin_url(rq);
    let rows = ss
        .iter()
        .map(|s| {
            let u = |t: &str| rq.ctx.url(&format!("{}{t}", base(s)));
            let mut acts = if s.started {
                vec![(u("/confirm?op=end"), "End"), (u("/confirm?op=restart"), "Restart")]
            } else {
                vec![(u("/start"), "Start")]
            };
            acts.push((u(""), "Properties"));
            let name = if s.name == "*HTTP" {
                format!(
                    "<a href=\"{}\">{}</a> <span class=\"small grey\">(HTTP Server Administration)</span>",
                    esc(&admin),
                    esc(&s.name)
                )
            } else {
                esc(&s.name)
            };
            vec![
                Cell::act(actions(&acts)),
                Cell::mono_html(name),
                Cell::text(&s.text),
                Cell::html(status(s)),
                Cell::mono(yes(s.autostart)),
                Cell::mono(&ports(s)),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Server", "n"),
            Col::plain("Description"),
            Col::sorted("Status", "s"),
            Col::plain("Autostart"),
            Col::plain("Ports"),
        ],
        rows,
        empty: "No TCP servers.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}{}",
        list_table(&list),
        form(
            &rq.ctx,
            "",
            &buttons(&[
                Button::new("start", "Start all autostart servers"),
                Button::new("refresh", "Refresh")
            ])
        )
    );
    page(
        rq,
        &["Servers"],
        "Servers",
        Some("TCP/IP servers (STRTCPSVR / ENDTCPSVR)."),
        body,
        "servers",
        Some("/tcp/servers"),
    )
}

/// `/tcp/servers/<name>`: Apply runs the server's `CHGxxxA AUTOSTART()`.
fn properties(rq: &Rq<'_>, s: &Server) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") || s.attr_cmd.is_empty() {
            return rq.redirect("/tcp/servers");
        }
        let auto = if f.str("auto") == "*YES" {
            "*YES"
        } else {
            "*NO"
        };
        if let Err(r) = need(rq, "Change server attributes", &s.attr_cmd, "Servers") {
            return r;
        }
        let back = if f.has("ok") {
            "/tcp/servers".to_string()
        } else {
            base(s)
        };
        return reply(
            rq,
            rq.env()
                .net
                .run(rq.user(), &format!("{} AUTOSTART({auto})", s.attr_cmd)),
            &back,
        );
    }
    let auto = if s.attr_cmd.is_empty() {
        format!(
            "{} &nbsp; <span class=\"small grey\">Set by the server's own configuration.</span>",
            mono(yes(s.autostart))
        )
    } else {
        format!(
            "{} &nbsp; <span class=\"small grey\">{}</span>",
            pick("auto", &strings(&["*YES", "*NO"]), yes(s.autostart)),
            esc(&format!("{} AUTOSTART()", s.attr_cmd))
        )
    };
    let rows = vec![
        Prop::new("Server", mono(&s.name)),
        Prop::new("Description", esc(&s.text)),
        Prop::new("Status", status(s)),
        Prop::new(
            "Ports",
            mono(
                &s.ports
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ),
        Prop::new("Start with TCP/IP", auto),
    ];
    let mut bs = vec![
        Button::new("apply", "Apply"),
        Button::new("ok", "OK"),
        Button::new("cancel", "Cancel"),
    ];
    if s.attr_cmd.is_empty() {
        bs = vec![Button::new("cancel", "Cancel")];
    }
    let body = props(&rq.ctx, &[PropGroup { title: None, rows }], &bs);
    page(
        rq,
        &["Servers", &s.name],
        &format!("Server {}", s.name),
        None,
        body,
        "servers",
        None,
    )
}

/// Start, or the confirmed End and Restart.
fn act(rq: &Rq<'_>, s: &Server, op: &str) -> Response {
    let n = &s.name;
    let (what, command, sentence) = match op {
        "start" => ("Start Server", format!("STRTCPSVR SERVER({n})"), "Press OK to start the servers listed below.".to_string()),
        "restart" => (
            "Restart Server",
            format!("STRTCPSVR SERVER({n}) RESTART({n})"),
            "Restarting a server ends its jobs and starts them again; its clients are disconnected. Press OK to restart the servers listed below, or Cancel to return.".to_string(),
        ),
        _ => (
            "End Server",
            format!("ENDTCPSVR SERVER({n})"),
            "Ending a server ends its jobs; its clients are disconnected. Press OK to end the servers listed below, or Cancel to return.".to_string(),
        ),
    };
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/tcp/servers");
        }
        let cmd = if op == "end" {
            "ENDTCPSVR"
        } else {
            "STRTCPSVR"
        };
        if let Err(r) = need(rq, what, cmd, "Servers") {
            return r;
        }
        return confirmed(rq, &command, "/tcp/servers");
    }
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what,
            crumb: &["Servers"],
            sentence,
            cols: &["Server", "Description", "Status", "Ports"],
            rows: vec![vec![mono(n), esc(&s.text), status(s), mono(&ports(s))]],
            options: String::new(),
            command,
        },
    )
}
