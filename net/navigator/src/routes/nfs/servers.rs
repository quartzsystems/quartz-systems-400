// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/nfs/servers`: the NFS daemons by type, in the order `STRNFSSVR
//! SERVER(*ALL)` starts them; `/nfs/servers/<type>/confirm?op=start|end`
//! starts (`STRNFSSVR SERVER(*type)`) or ends (`ENDNFSSVR`) one type,
//! `ALL` every type.

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Button, Cell, Col, List, actions, buttons, form, list_table, mono};

use super::{GROUP, need, nfs_tabs, page, run, when};
use crate::env::nfs::Daemon;
use crate::routes::Rq;
use crate::routes::common::{Confirm, confirm_in, word};

/// The types, in `STRNFSSVR SERVER(*ALL)`'s order, with what each does.
pub const TYPES: [(&str, &str); 7] = [
    ("*RPC", "RPC binder"),
    ("*BIO", "Block I/O for NFS mounts"),
    ("*RGY", "NFSv4 identity mapping"),
    ("*SVR", "NFS server"),
    ("*MNT", "Mount"),
    ("*NSM", "Network status monitor"),
    ("*NLM", "Network lock manager"),
];

/// Route `/nfs/servers/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq),
        [kind, "confirm"] => {
            let server = if kind.eq_ignore_ascii_case("ALL") {
                "*ALL".to_string()
            } else {
                format!("*{}", kind.to_ascii_uppercase())
            };
            if server != "*ALL" && !TYPES.iter().any(|(k, _)| *k == server) {
                return rq.not_found(&format!("NFS server type {server}"));
            }
            let start = rq.query().str("op") == "start";
            act(rq, &server, start)
        }
        _ => rq.not_found(&format!("page {} of NFS servers", rest.join("/"))),
    }
}

fn status(jobs: &[&Daemon]) -> String {
    if jobs.is_empty() {
        word("Inactive", Some(false))
    } else if jobs.iter().all(|j| j.ending) {
        word("Ending", None)
    } else if jobs.iter().any(|j| j.ready && !j.ending) {
        word("Active", Some(true))
    } else {
        word("Starting", None)
    }
}

/// `/nfs/servers`; the buttons post here.
fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("start") {
            return rq.redirect("/nfs/servers/ALL/confirm?op=start");
        }
        if f.has("end") {
            return rq.redirect("/nfs/servers/ALL/confirm?op=end");
        }
        return rq.redirect("/nfs/servers");
    }
    let all = rq.env().nfs.daemons(rq.user());
    let rows = TYPES
        .iter()
        .map(|(kind, text)| {
            let jobs: Vec<&Daemon> = all.iter().filter(|d| d.server == *kind).collect();
            let base = format!("/nfs/servers/{}/confirm", kind.trim_start_matches('*'));
            let mut acts = Vec::new();
            if !jobs.iter().any(|j| j.ready && !j.ending) {
                acts.push((rq.ctx.url(&format!("{base}?op=start")), "Start"));
            }
            if !jobs.is_empty() {
                acts.push((rq.ctx.url(&format!("{base}?op=end")), "End"));
            }
            let mut ports: Vec<String> = jobs.iter().flat_map(|j| j.ports.clone()).collect();
            ports.dedup();
            vec![
                Cell::act(actions(&acts)),
                Cell::mono(kind),
                Cell::text(text),
                Cell::html(status(&jobs)),
                Cell::mono(
                    &jobs
                        .iter()
                        .map(|j| j.job.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                Cell::mono(&ports.join(", ")),
                Cell::text(&when(rq, jobs.iter().map(|j| j.since).min().unwrap_or(0))),
            ]
        })
        .collect();
    let list = List {
        cols: [
            "Actions",
            "Server",
            "Description",
            "Status",
            "Jobs",
            "Ports",
            "Started",
        ]
        .iter()
        .map(|l| Col::plain(l))
        .collect(),
        rows,
        empty: String::new(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}{}{}",
        nfs_tabs(rq, 1),
        list_table(&list),
        form(
            &rq.ctx,
            "",
            &buttons(&[
                Button::new("start", "Start all..."),
                Button::new("end", "End all..."),
                Button::new("refresh", "Refresh"),
            ])
        )
    );
    page(
        rq,
        &["Servers"],
        "NFS servers",
        body,
        "nfsservers",
        Some("/nfs"),
    )
}

/// Start or end `server` (`*ALL` or a type), confirmed.
fn act(rq: &Rq<'_>, server: &str, start: bool) -> Response {
    let (cmd, what, function) = if start {
        ("STRNFSSVR", "Start NFS Servers", "Start NFS servers")
    } else {
        ("ENDNFSSVR", "End NFS Servers", "End NFS servers")
    };
    let command = format!("{cmd} SERVER({server})");
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/nfs/servers");
        }
        if let Err(r) = need(rq, function, cmd, "Servers") {
            return r;
        }
        return run(rq, &command, "/nfs/servers");
    }
    let rows = TYPES
        .iter()
        .filter(|(k, _)| server == "*ALL" || *k == server)
        .map(|(k, t)| vec![mono(k), esc(t)])
        .collect();
    let sentence = if start {
        "The daemon jobs are submitted to QSYSWRK and the command waits until they are ready; the RPC binder is started first when a server needs it. Press OK to start the servers listed below, or Cancel to return."
    } else {
        "The daemon jobs end and NFS clients can no longer reach the exports through them. Press OK to end the servers listed below, or Cancel to return."
    };
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what,
            crumb: &["NFS", "Servers"],
            sentence: sentence.into(),
            cols: &["Server", "Description"],
            rows,
            options: String::new(),
            command,
        },
    )
}
