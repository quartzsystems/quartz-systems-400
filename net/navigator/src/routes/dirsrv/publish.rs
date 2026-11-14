// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/dirsrv/publish`: the publishing agents (users, printers), where
//! each publishes and its status; `/dirsrv/publish/<Users|Printers>`
//! one agent's configuration (`publish.conf`).

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, input, list_table, mono,
    props_table,
};

use super::{fail, need, page, top_tabs, when, yes_no};
use crate::env::dirsrv::{AgentKind, PublishAgent};
use crate::routes::Rq;
use crate::routes::common::{pick, strings, word};

fn target(a: &PublishAgent) -> String {
    let host = if a.server.is_empty() {
        "this system".to_string()
    } else {
        a.server.clone()
    };
    format!("{host}:{}{}", a.port, if a.tls { " (TLS)" } else { "" })
}

/// `/dirsrv/publish/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq),
        [name] => match AgentKind::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(name))
        {
            Some(k) => edit(rq, k),
            None => rq.not_found(&format!("publishing agent {name}")),
        },
        _ => rq.not_found(&format!("page {} of publishing", rest.join("/"))),
    }
}

fn list(rq: &Rq<'_>) -> Response {
    let d = &rq.env().dirsrv;
    let agents = match d.publishing(rq.user()) {
        Ok(a) => a,
        Err(e) => return fail(rq, &e, "Directory publishing", &[], "/dirsrv"),
    };
    let status = d.publishing_status(rq.user()).unwrap_or_default();
    let rows = agents
        .iter()
        .map(|a| {
            let s = status.iter().find(|s| s.kind == a.kind);
            let st = match s {
                Some(s) if s.active => word("Active", Some(true)),
                _ => word("Inactive", Some(false)),
            };
            vec![
                Cell::act(actions(&[(
                    rq.ctx.url(&format!("/dirsrv/publish/{}", a.kind.name())),
                    "Properties",
                )])),
                Cell::text(a.kind.name()),
                Cell::text(yes_no(a.enabled)),
                Cell::mono(&target(a)),
                Cell::mono(&a.parent_dn),
                Cell::html(st),
                Cell::mono(s.map_or("", |s| s.job.as_str())),
                Cell::num(&s.map_or(0, |s| s.published).to_string()),
                Cell::text(&when(rq, s.map_or(0, |s| s.last))),
                Cell::text(&s.map_or_else(String::new, |s| {
                    if s.msgid.is_empty() {
                        String::new()
                    } else {
                        format!("{} {}", s.msgid, s.text)
                    }
                })),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Actions",
            "Agent",
            "Publishes",
            "Directory server",
            "Parent DN",
            "Status",
            "Job",
            "Published",
            "Last",
            "Last message",
        ]
        .iter()
        .map(|l| Col::plain(l))
        .collect(),
        rows,
        empty: String::new(),
        keep: Vec::new(),
    };
    page(
        rq,
        &["Publishing"],
        "Directory publishing",
        format!("{}{}", top_tabs(rq, 1), list_table(&list)),
        "dspublish",
        true,
    )
}

#[allow(clippy::too_many_lines)]
fn edit(rq: &Rq<'_>, kind: AgentKind) -> Response {
    let d = &rq.env().dirsrv;
    let agents = match d.publishing(rq.user()) {
        Ok(a) => a,
        Err(e) => return fail(rq, &e, "Directory publishing", &[], "/dirsrv/publish"),
    };
    let Some(a) = agents.into_iter().find(|a| a.kind == kind) else {
        return rq.not_found(&format!("publishing agent {}", kind.name()));
    };
    let here = format!("/dirsrv/publish/{}", kind.name());
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/dirsrv/publish");
        }
        let function = "Change directory publishing";
        if let Err(r) = need(rq, None, function, &["Publishing"]) {
            return r;
        }
        let to = PublishAgent {
            enabled: f.str("enabled") == "*YES",
            server: f.str("server").trim().to_string(),
            port: f.str("port").trim().parse().unwrap_or(0),
            tls: f.str("tls") == "*YES",
            bind_dn: f.str("bind").trim().to_string(),
            parent_dn: f.str("parent").trim().to_string(),
            ..a.clone()
        };
        let pw = f.str("pw");
        let password = match f.str("pwop") {
            "*CHANGE" => Some(pw),
            "*REMOVE" => Some(""),
            _ => None,
        };
        return match d.set_publishing(rq.user(), &to, password) {
            Ok(()) => rq.redirect("/dirsrv/publish"),
            Err(e) => fail(rq, &e, function, &["Publishing"], &here),
        };
    }
    let yn = |n: &str, b: bool| {
        pick(
            n,
            &strings(&["*NO", "*YES"]),
            if b { "*YES" } else { "*NO" },
        )
    };
    let hint = |h: &str| format!(" <span class=\"small grey\">{}</span>", esc(h));
    let rows = vec![
        Prop::new("Agent", esc(kind.name())),
        Prop::new("Job", mono(kind.job_name())),
        Prop::new("Publish", yn("enabled", a.enabled)),
        Prop::new(
            "Directory server",
            format!(
                "{}{}",
                input("server", &a.server, 40),
                hint("host name or address; blank: this system's default instance")
            ),
        ),
        Prop::new("Port", input("port", &a.port.to_string(), 5)),
        Prop::new("Use TLS", yn("tls", a.tls)),
        Prop::new("Bind DN", input("bind", &a.bind_dn, 50)),
        Prop::new(
            "Bind password",
            format!(
                "{} {}{}",
                pick("pwop", &strings(&["*SAME", "*CHANGE", "*REMOVE"]), "*SAME"),
                "<input type=\"password\" name=\"pw\" size=\"30\">",
                hint(if a.password_set {
                    "a password is kept"
                } else {
                    "no password is kept"
                })
            ),
        ),
        Prop::new(
            "Parent DN",
            format!(
                "{}{}",
                input("parent", &a.parent_dn, 50),
                hint(&format!(
                    "the entries go below it ({}, ...)",
                    kind.default_ou()
                ))
            ),
        ),
    ];
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "{}{}",
            props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
            buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
        ),
    );
    let title = format!("Publishing of {}", kind.name().to_ascii_lowercase());
    page(
        rq,
        &["Publishing", kind.name()],
        &title,
        body,
        "dspublish",
        false,
    )
}
