// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/netsvr/sessions`: the sessions; `/netsvr/sessions/<id>` one
//! session's connections and their open files; `/<id>/confirm` End
//! session (`QZLSENSS *SESSID`).

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, List, Prop, PropGroup, actions, list_table, mono, props_table};

use super::{GROUP, Parm, answer, call, need, ns_tabs, page, span, when};
use crate::env::netsvr::Session;
use crate::routes::Rq;
use crate::routes::work::{Confirm, confirm_in};

/// Route `/netsvr/sessions/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    let [id, more @ ..] = rest else {
        return list(rq);
    };
    let s = id.parse::<u64>().ok().and_then(|n| {
        rq.env()
            .netsvr
            .sessions(rq.user())
            .into_iter()
            .find(|s| s.id == n)
    });
    let Some(s) = s else {
        return rq.not_found(&format!("NetServer session {id}"));
    };
    match more {
        [] => detail(rq, &s),
        ["confirm"] => end(rq, &s),
        _ => rq.not_found(&format!("NetServer session {id}")),
    }
}

fn kind(guest: bool) -> &'static str {
    if guest { "Guest" } else { "User" }
}

fn conn_kind(k: i32) -> &'static str {
    match k {
        0 => "Disk",
        1 => "Print",
        3 => "IPC",
        _ => "Other",
    }
}

fn idle(rq: &Rq<'_>, s: &Session) -> String {
    span(rq.env().system.now().saturating_sub(s.last_active))
}

/// `/netsvr/sessions`.
fn list(rq: &Rq<'_>) -> Response {
    let sessions = rq.env().netsvr.sessions(rq.user());
    let rows = sessions
        .iter()
        .map(|s| {
            let u = |t: &str| rq.ctx.url(&format!("/netsvr/sessions/{}{t}", s.id));
            vec![
                Cell::act(actions(&[(u(""), "Connections"), (u("/confirm"), "End")])),
                Cell::mono(&s.workstation),
                Cell::mono(&s.user),
                Cell::text(kind(s.guest)),
                Cell::num(&s.connections.len().to_string()),
                Cell::num(&s.opens().to_string()),
                Cell::text(&when(rq, s.since)),
                Cell::mono(&idle(rq, s)),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::plain("Workstation"),
            Col::plain("User"),
            Col::plain("Logon"),
            Col::plain("Connections"),
            Col::plain("Open files"),
            Col::plain("Started"),
            Col::plain("Idle"),
        ],
        rows,
        empty: "No sessions.".into(),
        keep: Vec::new(),
    };
    let body = format!("{}{}", ns_tabs(rq, 3), list_table(&list));
    page(
        rq,
        &["Sessions"],
        "NetServer sessions",
        None,
        body,
        "nssessions",
        Some("/netsvr"),
    )
}

/// `/netsvr/sessions/<id>`.
fn detail(rq: &Rq<'_>, s: &Session) -> Response {
    let props = vec![
        Prop::new("Workstation", mono(&s.workstation)),
        Prop::new("Expanded name", mono(&s.expanded)),
        Prop::new("User", mono(&s.user)),
        Prop::new("Logon", esc(kind(s.guest))),
        Prop::new("Serving job", mono(&s.job)),
        Prop::new("Started", esc(&when(rq, s.since))),
        Prop::new("Last request", esc(&when(rq, s.last_active))),
    ];
    let rows = s
        .connections
        .iter()
        .map(|c| {
            vec![
                Cell::mono(&c.share),
                Cell::text(conn_kind(c.kind)),
                Cell::num(&c.opens.to_string()),
                Cell::text(&when(rq, c.since)),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Share"),
            Col::plain("Type"),
            Col::plain("Open files"),
            Col::plain("Connected"),
        ],
        rows,
        empty: "No connections.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}{}<h2>Connections</h2>{}<p><a href=\"{}\">End session...</a></p>",
        ns_tabs(rq, 3),
        props_table(
            rq.ctx.product,
            &[PropGroup {
                title: None,
                rows: props
            }],
            false
        ),
        list_table(&list),
        esc(&rq.ctx.url(&format!("/netsvr/sessions/{}/confirm", s.id)))
    );
    let title = format!("Session {}", s.id);
    page(
        rq,
        &["Sessions", &title],
        &title,
        None,
        body,
        "nssessions",
        None,
    )
}

/// End session, confirmed.
fn end(rq: &Rq<'_>, s: &Session) -> Response {
    let id = i64::try_from(s.id).unwrap_or(i64::MAX);
    let command = call("QZLSENSS", &[Parm::Char("*SESSID")], &[Parm::Bin8(id)]);
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/netsvr/sessions");
        }
        if let Err(r) = need(rq, "End NetServer session", "QZLSENSS", "Sessions") {
            return r;
        }
        let r = rq.env().netsvr.end_session(rq.user(), s.id);
        return answer(
            rq,
            r,
            "End NetServer session",
            "QZLSENSS",
            "Sessions",
            "/netsvr/sessions",
        );
    }
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what: "End Session",
            crumb: &["NetServer", "Sessions"],
            sentence: "Ending a session disconnects the client; files it has open are closed and unsaved changes in them are lost. Press OK to end the session listed below, or Cancel to return.".into(),
            cols: &["Workstation", "User", "Connections", "Open files"],
            rows: vec![vec![
                mono(&s.workstation),
                mono(&s.user),
                s.connections.len().to_string(),
                s.opens().to_string(),
            ]],
            options: String::new(),
            command,
        },
    )
}
