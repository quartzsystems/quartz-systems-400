// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Commands (handoff steps 18-20), the desktop's Run Command: run a CL
//! command as the signed-on user and wait, or submit it (`SBMJOB`) to a
//! job queue; Check; the results with their messages; the recent
//! commands (50, newest first); Prompt ([`prompt`], the web F4) and My
//! commands ([`saved`]).

pub mod parse;
pub mod prompt;
pub mod saved;

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Cell, Col, List, help_link, list_table, radio};

use super::Rq;
use super::msgs::{id_link, quote};
use crate::env::cmd::{How, Outcome, Ran, Saved};
use crate::{PRODUCT, msgs, policy};

/// The tree's group.
const GROUP: &str = "Commands";
/// The breadcrumb.
const CRUMB: &[&str] = &["Commands", "Run command"];
/// Recent commands kept.
const RECENT: usize = 50;
/// Recent commands shown.
const SHOWN: usize = 5;

/// Route the area's first path segments.
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    match segs {
        ["cmd"] => run_page(rq),
        ["cmd", "prompt"] => prompt::page(rq),
        ["cmd", "saved"] => saved::page(rq),
        ["cmd", "saved", n, "confirm"] => saved::delete(rq, n),
        _ => rq.not_found(&rq.req.path),
    }
}

/// The command's name, as its messages name it (`CALL`).
pub fn name_of(command: &str) -> String {
    let p = parse::parse(command);
    p.name.rsplit('/').next().unwrap_or("").to_string()
}

/// The message a run's result line shows.
fn outcome_message(rq: &Rq<'_>, r: &Ran) -> Message {
    let n = name_of(&r.command);
    let (kind, id) = match r.outcome {
        Outcome::Completed => (Kind::Done, msgs::CMD_COMPLETED),
        Outcome::Failed => (Kind::Error, msgs::CMD_FAILED),
        Outcome::Cancelled => (Kind::Info, msgs::CMD_CANCELLED),
        Outcome::NotFound => (Kind::Error, msgs::CMD_NOT_FOUND),
        Outcome::Checked => (Kind::Done, msgs::CMD_CHECKED),
        Outcome::CheckFailed => (Kind::Error, msgs::CMD_ERRORS),
    };
    rq.message(kind, id, &[&n])
}

/// One message row, inline in a result line.
fn inline_row(m: &Message) -> String {
    let mut href = PRODUCT.msg(&m.id);
    for (i, d) in m.data.iter().enumerate() {
        href.push(if i == 0 { '?' } else { '&' });
        href.push_str("d=");
        href.push_str(&qsweb::http::form::encode(d));
    }
    format!(
        "<table class=\"msg\" style=\"display:inline-table;width:auto;margin:0\"><tr><td class=\"{}\"><a class=\"msgid\" href=\"{}\" target=\"_blank\">{}</a> - {}</td></tr></table>",
        m.kind.class(),
        esc(&href),
        esc(&m.id),
        esc(&m.text)
    )
}

/// Run `command` (`how`, to `jobq` in batch) as the user, keep it with
/// the recent commands and a saved one's Last run; the message to show
/// after, when the result line does not say it all.
pub fn run(rq: &Rq<'_>, command: &str, how: How, jobq: &str, check: bool) -> Option<Message> {
    let typed = command.trim().to_string();
    let line = match how {
        How::Batch if !check => {
            let q = if jobq.trim().is_empty() {
                String::new()
            } else {
                format!(" JOBQ({})", jobq.trim())
            };
            format!("SBMJOB CMD({typed}){q}")
        }
        _ => typed.clone(),
    };
    let cmd = &rq.env().cmd;
    let ran = cmd.run(rq.user(), &line, check);
    if check {
        return None;
    }
    let mut recent = cmd.recent(rq.user());
    recent.retain(|c| *c != typed);
    recent.insert(0, typed.clone());
    recent.truncate(RECENT);
    let _ = cmd.put_recent(rq.user(), &recent);
    let mut kept = cmd.saved(rq.user());
    if let Some(s) = kept.iter_mut().find(|s| s.command == typed) {
        s.last = rq.env().system.now();
        let _ = cmd.put_saved(rq.user(), &kept);
    }
    if how != How::Batch || ran.outcome != Outcome::Completed {
        return None;
    }
    // The job the submit made, from its completion message.
    let job = ran
        .messages
        .iter()
        .rev()
        .flat_map(|m| m.text.split_whitespace())
        .find(|w| {
            let p: Vec<&str> = w.split('/').collect();
            p.len() == 3 && p[0].len() == 6 && p[0].chars().all(|c| c.is_ascii_digit())
        })
        .unwrap_or("")
        .to_string();
    let q = if jobq.trim().is_empty() {
        "*JOBD".to_string()
    } else {
        jobq.trim().to_string()
    };
    Some(rq.message(Kind::Done, msgs::SUBMITTED, &[&q, &job]))
}

/// Save `command` to My commands; `edit` replaces that entry.
pub fn save(rq: &Rq<'_>, command: &str, how: How, edit: Option<usize>) -> Response {
    if let Err(r) = rq.policy(policy::CMD_SAVE, CRUMB) {
        return r;
    }
    let command = command.trim();
    if command.is_empty() {
        return rq.done(
            "/cmd",
            rq.message(Kind::Error, msgs::BAD_VALUE, &["", "Command"]),
        );
    }
    let cmd = &rq.env().cmd;
    let mut all = cmd.saved(rq.user());
    let name = if let Some(s) = edit.and_then(|i| all.get_mut(i)) {
        s.command = command.to_string();
        s.how = how;
        s.name.clone()
    } else {
        let name: String = command.chars().take(30).collect();
        all.push(Saved {
            name: name.clone(),
            command: command.to_string(),
            how,
            last: 0,
            share: String::new(),
        });
        name
    };
    match cmd.put_saved(rq.user(), &all) {
        Ok(()) => rq.done(
            "/cmd/saved",
            rq.message(Kind::Done, msgs::CMD_SAVED, &[&name]),
        ),
        Err(m) => rq.done("/cmd", m),
    }
}

/// `GET /cmd`: the command line, the results, the recent commands;
/// `POST` runs, checks, cancels, clears or saves.
#[allow(clippy::too_many_lines)]
fn run_page(rq: &Rq<'_>) -> Response {
    if let Err(r) = rq.policy(policy::CMD_RUN, CRUMB) {
        return r;
    }
    let q = rq.query();
    let cmd = &rq.env().cmd;
    let edit: Option<usize> = q.get("edit").and_then(|n| n.parse().ok());
    if rq.post() {
        let f = rq.form();
        let how = How::from_code(f.str("how"));
        let typed = f.str("cmd");
        if f.has("run") || f.has("check") {
            if typed.trim().is_empty() {
                return rq.done(
                    "/cmd",
                    rq.message(Kind::Error, msgs::BAD_VALUE, &["", "Command"]),
                );
            }
            let next = run(rq, typed, how, f.str("jobq"), f.has("check"));
            let back = format!("/cmd{}", query(&[("c", typed.trim()), ("how", how.code())]));
            return match next {
                Some(m) => rq.done(&back, m),
                None => rq.redirect(&back),
            };
        }
        if f.has("cancel") {
            cmd.cancel(rq.user());
            return rq.redirect("/cmd");
        }
        if f.has("clear") {
            cmd.clear(rq.user());
            return rq.redirect("/cmd");
        }
        if f.has("save") {
            return save(rq, typed, how, edit);
        }
        // New command... from My commands: an empty line.
        return rq.redirect("/cmd?new=1");
    }
    let results = cmd.results(rq.user());
    let saved_cmd = edit.and_then(|i| cmd.saved(rq.user()).get(i).cloned());
    let typed = match (&saved_cmd, q.get("c").or_else(|| q.get("new").map(|_| ""))) {
        (Some(s), _) => s.command.clone(),
        (None, Some(c)) => c.to_string(),
        (None, None) => results
            .last()
            .map(|r| r.command.clone())
            .unwrap_or_default(),
    };
    let how = saved_cmd
        .as_ref()
        .map_or_else(|| How::from_code(q.str("how")), |s| s.how);
    let queues = cmd.job_queues(rq.user());
    let mut jobq = String::from("<select name=\"jobq\">");
    for o in &queues {
        let _ = write!(jobq, "<option>{}</option>", esc(o));
    }
    jobq.push_str("</select>");
    let mut body = format!(
        "<form method=\"post\">{}<table class=\"props\"><tr><th>Command {}</th><td><input type=\"text\" name=\"cmd\" value=\"{}\" size=\"90\" class=\"cmd\"></td></tr><tr><th>Run</th><td>{} In this window's own connection job (waits for completion; no timeout)<br>{} In batch (SBMJOB), job queue: {jobq}</td></tr></table><div class=\"buttons\"><input type=\"submit\" name=\"run\" value=\"Run\"><input type=\"submit\" name=\"check\" value=\"Check\">",
        rq.ctx.csrf_field(),
        help_link(&PRODUCT, "cmd"),
        esc(&typed),
        radio("how", "int", how == How::Interactive),
        radio("how", "batch", how == How::Batch),
    );
    if rq.allowed(policy::CMD_PROMPT) {
        let _ = write!(
            body,
            "<input type=\"submit\" name=\"prompt\" value=\"Prompt\" formaction=\"{}\">",
            esc(&PRODUCT.url("/cmd/prompt"))
        );
    }
    body.push_str("<input type=\"submit\" name=\"cancel\" value=\"Cancel\"><input type=\"submit\" name=\"clear\" value=\"Clear\">");
    if rq.allowed(policy::CMD_SAVE) {
        body.push_str("<input type=\"submit\" name=\"save\" value=\"Save to My commands\">");
    }
    body.push_str("</div></form>");
    if !results.is_empty() {
        body.push_str("<h2>Results</h2>");
        for r in &results {
            let _ = write!(
                body,
                "<p class=\"small\">Command: <span class=\"mono\">{}</span> &nbsp; Job: <span class=\"mono\">{}</span> &nbsp; {}</p>",
                esc(&r.command),
                esc(&r.job),
                inline_row(&outcome_message(rq, r))
            );
        }
        let mut all: Vec<_> = results.iter().flat_map(|r| r.messages.iter()).collect();
        if q.str("sort") == "id" {
            all.sort_by(|a, b| a.id.cmp(&b.id));
        }
        let rows = all
            .iter()
            .map(|m| {
                let severe = m.severity >= 30;
                let sev = format!("{:02}", m.severity);
                let ms = m.sent_ms % 1000;
                let (_, t) = rq.stamp(m.sent_ms / 1000);
                vec![
                    if severe {
                        Cell {
                            class: Some("num"),
                            html: format!("<span class=\"err\">{sev}</span>"),
                        }
                    } else {
                        Cell::num(&sev)
                    },
                    Cell::mono_html(id_link(&m.id, severe)),
                    Cell::mono(&m.text),
                    Cell::mono(&m.program),
                    Cell::mono(&format!("{t}.{ms:03}")),
                ]
            })
            .collect();
        body.push_str(&list_table(&List {
            cols: vec![
                Col::plain("Sev"),
                Col::sorted("Message ID", "id"),
                Col::plain("Message text"),
                Col::plain("From program"),
                Col::plain("Time"),
            ],
            rows,
            empty: "The commands sent no messages.".into(),
            keep: Vec::new(),
        }));
    }
    let recent = cmd.recent(rq.user());
    if !recent.is_empty() {
        let links: Vec<String> = recent
            .iter()
            .take(SHOWN)
            .map(|c| {
                format!(
                    "<a href=\"{}\">{}</a>",
                    esc(&PRODUCT.url(&format!("/cmd{}", query(&[("c", c)])))),
                    esc(c)
                )
            })
            .collect();
        let _ = write!(
            body,
            "<h2>Recent commands</h2><div class=\"small mono\">{}</div>",
            links.join("<br>")
        );
    }
    rq.page(Page {
        title: "Run command".into(),
        crumb: CRUMB.iter().map(ToString::to_string).collect(),
        h1: "Run command".into(),
        desc: Some("Run a CL command on this system, as QS/400 Access for Desktop's Run Command does. Prompt is the web F4.".into()),
        body,
        help: "cmd".into(),
        nav: Some((GROUP, Some("/cmd"))),
        ..Page::default()
    })
}

/// `KEYWORD(value)` with a CL quote when the value needs one.
pub fn keyword(kw: &str, v: &str) -> String {
    let plain = v.starts_with('\'')
        || v.chars()
            .all(|c| c.is_ascii_alphanumeric() || "*/._$#@ ()-,+".contains(c));
    if plain {
        format!("{kw}({v})")
    } else {
        format!("{kw}({})", quote(v))
    }
}
