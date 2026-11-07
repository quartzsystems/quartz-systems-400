// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/5250/<id>/macros`: the session's macros (the desktop's `.qsm`
//! files): Play, Edit, Download, Delete; Record (keys, typing and the
//! waits after each key that goes to the system, until Stop); a `.qsm`
//! opened. A field that does not display is recorded as a prompt, never
//! as what was typed.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Cell, Col, List, list_table};

use super::session::{recording, start_recording, stop_recording};
use super::{GROUP, ITEM, Rq, start_policy, when_short};
use crate::env::session5250::{Saved, SavedMacro};
use crate::policy;
use crate::s5250::files::qsm::MacroFile;

/// `GET/POST /5250/<id>/macros` (`?edit=N` edits macro N).
pub fn page(rq: &Rq<'_>, mut s: Saved) -> Response {
    let title = s.file.display_title();
    let crumb = ["5250 session", title.as_str(), "Macros"];
    if let Err(r) = start_policy(rq, &crumb) {
        return r;
    }
    let editing = edited(rq, &s);
    if !rq.post() {
        return show(rq, &s, editing, Vec::new());
    }
    let f = rq.form();
    let display = format!("/5250/session/{}", s.id);
    if f.has("cancel") {
        return rq.redirect(&display);
    }
    if f.has("record") {
        let name = f.str("n").trim().to_ascii_uppercase();
        if name.is_empty() {
            return show(
                rq,
                &s,
                editing,
                vec![rq.message(Kind::Error, crate::msgs::BAD_VALUE, &["", "Name"])],
            );
        }
        start_recording(rq, s.id, &name);
        return rq.redirect(&display);
    }
    if f.has("play") {
        let Some(i) = editing else {
            return rq.redirect(&display);
        };
        let name = s.macros[i].file.display_name();
        return rq.redirect(&format!("{display}{}", query(&[("play", &name)])));
    }
    if let Err(r) = rq.policy(policy::S5250_SAVE, &crumb) {
        return r;
    }
    let size = s.file.session.screen_size;
    let now = rq.env().system.now();
    let mut done = None;
    if f.has("stop") {
        let Some((name, lines)) = stop_recording(rq, s.id) else {
            return rq.redirect(&format!("/5250/{}/macros", s.id));
        };
        let mut m = MacroFile::new(&name, size);
        m.steps = MacroFile::from_lines(&lines, size).unwrap_or_default();
        let n = m.steps.len().to_string();
        s.macros.retain(|x| x.file.display_name() != name);
        s.macros.push(SavedMacro {
            file: m,
            recorded: now,
            shared: String::new(),
        });
        done = Some(rq.message(Kind::Info, "QAD2021", &[&name, &n]));
    } else if f.has("save") {
        let parts = rq.req.multipart().unwrap_or_default();
        if let Some(p) = parts
            .iter()
            .find(|p| p.name == "qsm" && p.filename.as_deref().is_some_and(|n| !n.is_empty()))
        {
            let fname = p.filename.clone().unwrap_or_default();
            match MacroFile::parse(&String::from_utf8_lossy(&p.data)) {
                Ok(m) => {
                    let name = m.display_name();
                    s.macros.retain(|x| x.file.display_name() != name);
                    s.macros.push(SavedMacro {
                        file: m,
                        recorded: now,
                        shared: String::new(),
                    });
                    done = Some(rq.message(Kind::Info, crate::msgs::MACRO_SAVED, &[&name]));
                }
                Err(e) => {
                    let (id, data) = e.message(&fname);
                    let data: Vec<&str> = data.iter().map(String::as_str).collect();
                    return show(rq, &s, editing, vec![rq.message(Kind::Error, id, &data)]);
                }
            }
        } else if let Some(i) = editing {
            match MacroFile::from_lines(f.str("steps"), size) {
                Ok(steps) => {
                    s.macros[i].file.steps = steps;
                    done = Some(rq.message(
                        Kind::Info,
                        crate::msgs::MACRO_SAVED,
                        &[&s.macros[i].file.display_name()],
                    ));
                }
                Err(e) => {
                    let name = s.macros[i].file.display_name();
                    let n = e
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .trim_start_matches("line ")
                        .to_string();
                    return show(
                        rq,
                        &s,
                        editing,
                        vec![rq.message(Kind::Error, "QAD2020", &[&name, &n, &e])],
                    );
                }
            }
        }
    }
    let id = s.id;
    match (done, rq.env().session5250.put(rq.user(), s)) {
        (Some(m), Ok(_)) | (_, Err(m)) => rq.done(&format!("/5250/{id}/macros"), m),
        (None, Ok(_)) => rq.redirect(&format!("/5250/{id}/macros")),
    }
}

/// The macro the editor shows: `?edit=N`, else the first.
fn edited(rq: &Rq<'_>, s: &Saved) -> Option<usize> {
    let n = rq
        .query()
        .get("edit")
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(1);
    (n >= 1 && n <= s.macros.len())
        .then(|| n - 1)
        .or_else(|| (!s.macros.is_empty()).then_some(0))
}

fn show(rq: &Rq<'_>, s: &Saved, editing: Option<usize>, messages: Vec<Message>) -> Response {
    let title = s.file.display_title();
    let id = s.id;
    let mut macros: Vec<(usize, &SavedMacro)> = s.macros.iter().enumerate().collect();
    if rq.query().get("sort") == Some("n") {
        macros.sort_by_key(|(_, m)| m.file.display_name());
    }
    let save = rq.allowed(policy::S5250_SAVE);
    let rows = macros
        .iter()
        .map(|(i, m)| {
            let n = i + 1;
            let name = m.file.display_name();
            let mut acts = vec![
                format!(
                    "<a href=\"{}{}\">Play</a>",
                    esc(&rq.ctx.url(&format!("/5250/session/{id}"))),
                    esc(&query(&[("play", &name)]))
                ),
                format!(
                    "<a href=\"{}{}\">Edit</a>",
                    esc(&rq.ctx.url(&format!("/5250/{id}/macros"))),
                    esc(&query(&[("edit", &n.to_string())]))
                ),
                format!(
                    "<a href=\"{}\">Download .qsm</a>",
                    esc(&rq.ctx.url(&format!("/5250/{id}/macros/{n}/download")))
                ),
            ];
            if save {
                acts.push(format!(
                    "<a href=\"{}\">Delete</a>",
                    esc(&rq.ctx.url(&format!("/5250/{id}/macros/{n}/delete/confirm")))
                ));
            }
            vec![
                Cell::act(acts.join(" | ")),
                Cell::mono(&name),
                Cell::text(&m.file.steps.len().to_string()),
                Cell::text(&m.file.waits()),
                Cell::text(&when_short(rq, m.recorded)),
                Cell::text(if m.shared.is_empty() { "No" } else { &m.shared }),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Macro", "n"),
            Col::plain("Steps"),
            Col::plain("Waits for"),
            Col::plain("Recorded"),
            Col::plain("Shared"),
        ],
        rows,
        empty: "This session has no macros.".into(),
        keep: Vec::new(),
    };
    let rec = match recording(rq, id) {
        Some(n) => format!("Recording <span class=\"mono\">{}</span> <input type=\"submit\" name=\"stop\" value=\"Stop\">", esc(&n)),
        None => "<input type=\"submit\" name=\"record\" value=\"Record...\"> <span class=\"small grey\">keys and waits are recorded in the session until Stop; passwords typed in a nondisplay field are never kept</span>".into(),
    };
    let mut t = format!(
        "<table class=\"props\" cellspacing=\"0\">\n<tr class=\"subrow\"><td colspan=\"2\">Record</td></tr>\n<tr><th>Name</th><td><input type=\"text\" name=\"n\" value=\"\" size=\"20\"></td></tr>\n<tr><th>Record</th><td>{rec}</td></tr>\n"
    );
    if let Some(i) = editing {
        let m = &s.macros[i].file;
        let _ = write!(
            t,
            "<tr class=\"subrow\"><td colspan=\"2\">Edit {}</td></tr>\n<tr><th>Steps</th><td><textarea name=\"steps\" rows=\"8\" cols=\"70\" class=\"conf\">{}</textarea></td></tr>\n",
            esc(&m.display_name()),
            esc(&m.to_lines())
        );
    }
    t.push_str("<tr class=\"subrow\"><td colspan=\"2\">Macro file</td></tr>\n<tr><th>Open a macro file</th><td><input type=\"file\" name=\"qsm\" size=\"30\"> <span class=\"small grey\">.qsm from QS/400 Access for Desktop</span></td></tr>\n</table>");
    let save_b = if save {
        "<input type=\"submit\" name=\"save\" value=\"Save\">"
    } else {
        "<input type=\"submit\" name=\"save\" value=\"Save\" disabled>"
    };
    let body = format!(
        "{}<form method=\"post\" enctype=\"multipart/form-data\">{}{t}\n<div class=\"buttons\">{save_b}<input type=\"submit\" name=\"play\" value=\"Play\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\"></div></form>",
        list_table(&list),
        rq.ctx.csrf_field()
    );
    rq.page(Page {
        title: format!("Macros: {title}"),
        crumb: vec!["5250 session".into(), title.clone(), "Macros".into()],
        h1: format!("Macros: {title}"),
        messages,
        body,
        help: "5250macros".into(),
        nav: Some((GROUP, Some(ITEM))),
        ..Page::default()
    })
}

/// `/5250/<id>/macros/<n>/download` and `/.../<n>/delete/confirm`.
pub fn one(rq: &Rq<'_>, mut s: Saved, n: &str, rest: &[&str]) -> Response {
    let title = s.file.display_title();
    let Some(i) = n
        .parse::<usize>()
        .ok()
        .filter(|i| *i >= 1 && *i <= s.macros.len())
        .map(|i| i - 1)
    else {
        return rq.not_found("macro");
    };
    let name = s.macros[i].file.display_name();
    let crumb = ["5250 session", title.as_str(), "Macros"];
    if let Err(r) = start_policy(rq, &crumb) {
        return r;
    }
    match rest {
        ["download"] => Response::download(
            &format!("{name}.qsm"),
            "application/toml; charset=utf-8",
            s.macros[i].file.to_text().into_bytes(),
        ),
        ["delete", "confirm"] => {
            if let Err(r) = rq.policy(policy::S5250_SAVE, &crumb) {
                return r;
            }
            let back = format!("/5250/{}/macros", s.id);
            if rq.post() {
                if rq.form().has("cancel") {
                    return rq.redirect(&back);
                }
                s.macros.remove(i);
                return match rq.env().session5250.put(rq.user(), s) {
                    Ok(_) => rq.done(
                        &back,
                        rq.message(Kind::Info, crate::msgs::MACRO_DELETED, &[&name]),
                    ),
                    Err(m) => rq.done(&back, m),
                };
            }
            let m = &s.macros[i];
            let table = format!(
                "<table class=\"list\" cellspacing=\"0\" cellpadding=\"0\">\n<tr>\n<th>Macro</th>\n<th>Steps</th>\n<th>Recorded</th>\n</tr>\n<tr>\n<td class=\"mono\">{}</td>\n<td>{}</td>\n<td>{}</td>\n</tr>\n</table>",
                esc(&name),
                m.file.steps.len(),
                esc(&when_short(rq, m.recorded))
            );
            rq.page(Page {
                title: "Confirm delete".into(),
                crumb: vec![
                    "5250 session".into(),
                    title.clone(),
                    "Macros".into(),
                    "Delete".into(),
                ],
                h1: "Confirm delete".into(),
                body: qsweb::pages::confirm_body(
                    &rq.ctx,
                    "Delete this macro from the session?",
                    &table,
                    "",
                ),
                help: "confirm".into(),
                nav: Some((GROUP, Some(ITEM))),
                ..Page::default()
            })
        }
        _ => rq.not_found("page"),
    }
}
