// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The actions on spooled files, one request each as the desktop sends
//! them: Hold (now or at the end of the page), Release, Delete (after
//! `/print/confirm`), Move (to the top of the queue or its end), Save
//! as PDF or text. One file's result is its message; several files'
//! are `QAD6006` when any was refused, with each file's result under it.

use qsweb::frame::Page;
use qsweb::html::query;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::pages::confirm_body;
use qsweb::partials::{Cell, Col, List, list_table, mono};

use super::list::{DETAILS, selected, summary};
use super::{GROUP, OUTPUT, find, status, view_route};
use crate::env::print::{Act, Splf};
use crate::msgs;
use crate::policy;
use crate::routes::Rq;
use crate::routes::files::{error, token_ok};

/// Back to the list the form was on.
fn back_to(rq: &Rq<'_>) -> String {
    let q = rq.req.query.clone();
    if q.is_empty() {
        OUTPUT.to_string()
    } else {
        format!("{OUTPUT}?{q}")
    }
}

/// `act` on each of `ids`, then back to `to` with the result.
fn run(rq: &Rq<'_>, ids: &[String], act: &Act, to: &str) -> Response {
    let function = match act {
        Act::Move { .. } => policy::PRINT_MOVE,
        _ => policy::PRINT_OWN,
    };
    if let Err(r) = rq.policy(function, &[GROUP, "Printer output"]) {
        return r;
    }
    let mut files: Vec<Splf> = Vec::new();
    for id in ids {
        match find(rq, id) {
            Ok(f) => files.push(f),
            Err(r) => return r,
        }
    }
    let mut results: Vec<(Splf, Option<Message>)> = Vec::new();
    for f in files {
        let r = rq
            .env()
            .print
            .act(rq.user(), &f, act)
            .err()
            .map(|m| error(rq, &m));
        results.push((f, r));
    }
    let failed = results.iter().filter(|r| r.1.is_some()).count();
    let done = |f: &Splf| -> Message {
        let name = f.file.as_str();
        match act {
            Act::Hold { .. } => rq.message(Kind::Done, msgs::SPLF_HELD, &[name]),
            Act::Release => rq.message(Kind::Done, msgs::SPLF_RELEASED, &[name]),
            Act::Delete => rq.message(Kind::Done, msgs::SPLF_DELETED, &[name]),
            Act::Move { outq, .. } => rq.message(Kind::Done, msgs::SPLF_MOVED, &[name, outq]),
        }
    };
    let m = match results.as_slice() {
        [(f, None)] => done(f),
        [(_, Some(m))] => m.clone(),
        many => {
            let lines: Vec<String> = many
                .iter()
                .map(|(f, r)| match r {
                    None => format!("{}\t\t", f.file),
                    Some(m) => format!(
                        "{}\t{}\t{}",
                        f.file,
                        m.id,
                        m.text.replace(['\t', '\n'], " ")
                    ),
                })
                .collect();
            rq.set(&[(DETAILS, &lines.join("\n"))]);
            if failed == 0 {
                let names: Vec<&str> = many.iter().map(|(f, _)| f.file.as_str()).collect();
                let all = Splf {
                    file: names.join(", "),
                    ..Splf::default()
                };
                done(&all)
            } else {
                summary(rq, failed, many.len())
            }
        }
    };
    rq.done(to, m)
}

/// The list form's post.
pub fn post(rq: &Rq<'_>) -> Response {
    let f = rq.form();
    let ids = selected(rq);
    let back = back_to(rq);
    if ids.is_empty() {
        return rq.done(&back, rq.message(Kind::Info, msgs::NONE_SELECTED, &[]));
    }
    if f.has("hold") {
        return run(
            rq,
            &ids,
            &Act::Hold {
                page_end: f.str("hw") == "page",
            },
            &back,
        );
    }
    if f.has("release") {
        return run(rq, &ids, &Act::Release, &back);
    }
    if f.has("move") {
        let outq = f.str("to").trim().to_ascii_uppercase();
        if !rq.env().print.queues(rq.user()).contains(&outq) {
            return rq.done(
                &back,
                rq.message(Kind::Error, msgs::BAD_VALUE, &[&outq, "Move to"]),
            );
        }
        return run(
            rq,
            &ids,
            &Act::Move {
                outq,
                top: f.has("top"),
            },
            &back,
        );
    }
    if f.has("delete") {
        let mut q: Vec<(&str, &str)> = ids.iter().map(|i| ("s", i.as_str())).collect();
        q.push(("back", &back));
        return rq.redirect(&format!("/print/confirm{}", query(&q)));
    }
    for (button, what, title) in [
        ("pdf", "pdf", "Save as PDF"),
        ("text", "text", "Save as text"),
    ] {
        if f.has(button) {
            return match ids.as_slice() {
                [one] => rq.redirect(&format!("/print/view/{one}/{what}?save=1")),
                _ => rq.done(&back, rq.message(Kind::Info, msgs::ONE_SPLF, &[title])),
            };
        }
    }
    rq.redirect(&back)
}

/// `/print/act?op=hold|release&s=<id>&t=`: the viewer's Hold and
/// Release, back to the viewer.
pub fn link(rq: &Rq<'_>) -> Response {
    if !token_ok(rq) {
        return crate::routes::files::stale(rq);
    }
    let q = rq.query();
    let id = q.str("s").to_string();
    let act = match q.str("op") {
        "hold" => Act::Hold { page_end: false },
        "release" => Act::Release,
        _ => return rq.redirect(OUTPUT),
    };
    let f = match find(rq, &id) {
        Ok(f) => f,
        Err(r) => return r,
    };
    run(rq, &[id], &act, &view_route(&f))
}

/// `/print/confirm?s=<id>...&back=`: Delete, confirmed.
pub fn confirm(rq: &Rq<'_>) -> Response {
    let crumb = [GROUP, "Printer output", "Confirm Delete"];
    if let Err(r) = rq.policy(policy::PRINT_OWN, &crumb) {
        return r;
    }
    let q = rq.query();
    let ids: Vec<String> = q.all("s").into_iter().map(ToString::to_string).collect();
    let back = q
        .get("back")
        .filter(|b| b.starts_with(OUTPUT))
        .unwrap_or(OUTPUT)
        .to_string();
    if ids.is_empty() {
        return rq.redirect(&back);
    }
    if rq.post() {
        if !rq.form().has("ok") {
            return rq.redirect(&back);
        }
        return run(rq, &ids, &Act::Delete, &back);
    }
    let mut rows = Vec::new();
    for id in &ids {
        let f = match find(rq, id) {
            Ok(f) => f,
            Err(r) => return r,
        };
        rows.push(vec![
            Cell::mono(&f.file),
            Cell::mono(&f.user),
            Cell::html(status(&f.status)),
            Cell::num(&f.pages.to_string()),
            Cell::html(mono(&f.job)),
            Cell::num(&f.number.to_string()),
        ]);
    }
    let table = list_table(&List {
        cols: ["File", "User", "Status", "Pages", "Job", "File nbr"]
            .iter()
            .map(|c| Col::plain(c))
            .collect(),
        rows,
        empty: String::new(),
        keep: Vec::new(),
    });
    let body = confirm_body(
        &rq.ctx,
        "Deleting a spooled file removes it and its data from the system (DLTSPLF). Press OK to delete the files listed below, or Cancel to return without deleting.",
        &table,
        "",
    );
    rq.page(Page {
        title: "Confirm Delete".into(),
        crumb: crumb.iter().map(ToString::to_string).collect(),
        h1: "Confirm Delete".into(),
        body,
        help: "confirm".into(),
        nav: Some((GROUP, Some(OUTPUT))),
        ..Page::default()
    })
}
