// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A message queue (`DSPMSG`): the operator's view of any queue, with
//! its filter and Clear queue (`CLRMSGQ`), and My messages, the user's
//! own queue with the Send message form (`SNDMSG`). Remove is `RMVMSG`,
//! confirmed.

use std::cmp::Reverse;
use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{
    Cell, Col, FilterField, List, Paging, actions, filter, form, list_table, paging, radio,
};

use super::{GROUP, QSYSOPR, back, from, idlink, job_link, run};
use crate::env::Need;
use crate::env::msgs::QueuedMessage;
use crate::routes::Rq;
use crate::routes::common::{Confirm, confirm_in, keep, mono, quote, strings};
use crate::util::parse_mdy;

/// The Type filter's choices.
const TYPES: &[&str] = &[
    "All",
    "Inquiry (unanswered)",
    "Informational",
    "Severity 30+",
];
/// The longest text the operator's list shows whole.
const SHOWN: usize = 70;

/// The messages of `lib/name`, or the page saying why not.
fn read(rq: &Rq<'_>, lib: &str, name: &str) -> Result<Vec<QueuedMessage>, Response> {
    rq.env().msgs.messages(rq.user(), lib, name).map_err(|m| {
        if m.id == "CPF2207" {
            let t = crate::env::Target::obj(lib, name, "*MSGQ");
            rq.refuse(m, Need::AllObj, "Display messages", &t, &[GROUP])
        } else {
            rq.not_found(&format!("message queue {lib}/{name}"))
        }
    })
}

/// A message's text as the list shows it: a user's message says who
/// sent it.
pub fn text(m: &QueuedMessage) -> String {
    if m.id.is_empty() {
        format!("From {}: {}", m.user, m.text)
    } else {
        m.text.clone()
    }
}

/// `Info` or `Inquiry`.
pub fn kind(m: &QueuedMessage) -> &'static str {
    if m.inquiry { "Inquiry" } else { "Info" }
}

/// Text cut to what a list cell shows.
fn cut(s: &str) -> String {
    if s.chars().count() <= SHOWN {
        return s.to_string();
    }
    let mut t: String = s.chars().take(SHOWN - 3).collect();
    t.push_str("...");
    t
}

fn sort(ms: &mut [QueuedMessage], key: &str) {
    match key {
        "sev" => ms.sort_by_key(|m| Reverse(m.severity)),
        "id" => ms.sort_by(|a, b| a.id.cmp(&b.id)),
        "job" => ms.sort_by(|a, b| a.job.cmp(&b.job)),
        _ => {}
    }
}

/// `/msgq/<lib>/<name>` (the operator's view, `my` false) and
/// `/mymsgs`.
pub fn page(rq: &Rq<'_>, lib: &str, name: &str, my: bool) -> Response {
    let lib = lib.to_ascii_uppercase();
    let name = name.to_ascii_uppercase();
    if my && rq.post() {
        return send(rq, &lib, &name);
    }
    let q = rq.query();
    if !my && q.has("clear") {
        return rq.redirect(&format!("/msgq/{lib}/{name}/clear"));
    }
    let queues = rq.env().msgs.queues(rq.user());
    if !my
        && let Some(want) = q
            .get("q")
            .map(str::to_ascii_uppercase)
            .filter(|w| *w != name)
        && let Some((l, n)) = queues.iter().find(|(_, n)| *n == want)
    {
        let mut kept: Vec<(&str, &str)> = Vec::new();
        for k in ["t", "d"] {
            if let Some(v) = q.get(k) {
                kept.push((k, v));
            }
        }
        return rq.redirect(&format!("/msgq/{l}/{n}{}", qsweb::html::query(&kept)));
    }
    let mut ms = match read(rq, &lib, &name) {
        Ok(v) => v,
        Err(r) => return r,
    };
    ms.reverse();
    let (today, _) = rq.stamp(rq.env().system.now());
    let t = TYPES
        .iter()
        .find(|t| q.str("t") == **t)
        .copied()
        .unwrap_or("All");
    let d = q
        .get("d")
        .map_or_else(|| today.clone(), |d| d.trim().to_string());
    if !my {
        let since = parse_mdy(&d);
        ms.retain(|m| since.is_none_or(|s| m.sent >= s));
        ms.retain(|m| match t {
            "Inquiry (unanswered)" => m.waiting(),
            "Informational" => !m.inquiry,
            "Severity 30+" => m.severity >= 30,
            _ => true,
        });
    }
    let key = q.str("sort").to_string();
    sort(&mut ms, &key);
    let t_kept = if t == "All" { "" } else { t };
    let d_kept = if d == today { "" } else { d.as_str() };
    let filters = keep(&[("t", t_kept), ("d", d_kept)]);
    let mut kept = filters.clone();
    if !key.is_empty() {
        kept.push(("sort".into(), key.clone()));
    }
    let p = Paging::from_query(q.str("page"), q.str("rows"), ms.len(), kept);
    let rows = ms[p.range()]
        .iter()
        .map(|m| row(rq, &lib, &name, m, my))
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Date/time", "dt"),
            Col::sorted("Sev", "sev"),
            Col::sorted("Message ID", "id"),
            Col::plain("Message text"),
            Col::sorted("From job", "job"),
            Col::plain("Type"),
        ],
        rows,
        empty: "No messages on the queue.".into(),
        keep: filters,
    };
    let head = head(&queues, &name, my, t, &d);
    let mut body = format!("{head}{}{}", list_table(&list), paging(&p));
    if my {
        body.push_str(&send_form(rq, &ms));
    }
    let title = format!("Message queue: {name}");
    if my {
        return super::page(
            rq,
            &["My messages"],
            &title,
            &title,
            None,
            body,
            "mymsgs",
            Some((GROUP, Some("/mymsgs"))),
        );
    }
    let desc = if lib == "QSYS" && name == "QSYSOPR" {
        "System operator message queue (DSPMSG QSYSOPR). Inquiry messages awaiting a reply are shown in bold.".to_string()
    } else {
        format!(
            "Message queue {lib}/{name} (DSPMSG {lib}/{name}). Inquiry messages awaiting a reply are shown in bold."
        )
    };
    let item = (format!("/msgq/{lib}/{name}") == QSYSOPR).then_some(QSYSOPR);
    super::page(
        rq,
        &[&name],
        &title,
        &title,
        Some(&desc),
        body,
        "msgq",
        Some((GROUP, item)),
    )
}

/// The filter row: the queue's name and Refresh for My messages; the
/// queue, type and date for the operator, with Clear queue.
fn head(queues: &[(String, String)], name: &str, my: bool, t: &str, d: &str) -> String {
    if my {
        return format!(
            "<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr><td>Queue:</td><td>{}</td><td><input type=\"submit\" value=\"Refresh\"> </td></tr></table></form>",
            mono(name)
        );
    }
    let names: Vec<String> = queues.iter().map(|(_, n)| n.clone()).collect();
    let chosen = if names.first().is_some_and(|f| f == name) {
        String::new()
    } else {
        name.to_string()
    };
    let tc = if t == "All" {
        String::new()
    } else {
        t.to_string()
    };
    filter(
        &[
            FilterField::Select("Queue".into(), "q".into(), names, chosen),
            FilterField::Select("Type".into(), "t".into(), strings(TYPES), tc),
            FilterField::Text("Since".into(), "d".into(), d.to_string(), 8),
        ],
        true,
    )
    .replace(
        "value=\"Refresh\">",
        "value=\"Refresh\"> <input type=\"submit\" name=\"clear\" value=\"Clear queue...\">",
    )
}

/// One list row.
fn row(rq: &Rq<'_>, lib: &str, name: &str, m: &QueuedMessage, my: bool) -> Vec<Cell> {
    let base = format!("/msgq/{lib}/{name}/{}", m.key);
    let f = from(my);
    let detail = rq.ctx.url(&format!("{base}{f}"));
    let mut acts = vec![(detail.clone(), "Details")];
    if m.waiting() {
        acts.push((detail, "Reply"));
    }
    acts.push((rq.ctx.url(&format!("{base}/remove{f}")), "Remove"));
    let (dd, tt) = rq.stamp(m.sent);
    let txt = text(m);
    let txt = esc(&if my { txt } else { cut(&txt) });
    let txt = if m.waiting() {
        format!("<b>{txt}</b>")
    } else {
        txt
    };
    vec![
        Cell::act(actions(&acts)),
        Cell::mono(&format!("{dd} {tt}")),
        Cell::num(&format!("{:02}", m.severity)),
        Cell::mono_html(idlink(rq, &m.id)),
        Cell::html(txt),
        if my {
            Cell::html(mono(&m.job))
        } else {
            Cell::mono_html(job_link(rq, &m.job))
        },
        Cell::text(kind(m)),
    ]
}

/// My messages' Send message form; To user is the sender of the newest
/// message a user typed.
fn send_form(rq: &Rq<'_>, ms: &[QueuedMessage]) -> String {
    let to = ms
        .iter()
        .find(|m| m.id.is_empty())
        .map(|m| m.user.clone())
        .unwrap_or_default();
    let mut s = String::from("<h3>Send message</h3><table class=\"props\">");
    let _ = write!(
        s,
        "<tr><th>To user</th><td><input type=\"text\" name=\"to\" value=\"{}\" size=\"10\"> or message queue <input type=\"text\" name=\"toq\" value=\"\" size=\"10\"></td></tr>",
        esc(&to)
    );
    s.push_str(
        "<tr><th>Message</th><td><textarea name=\"m\" rows=\"3\" cols=\"70\"></textarea></td></tr>",
    );
    let _ = write!(
        s,
        "<tr><th>Type</th><td>{} Informational {} Inquiry (reply to my queue)</td></tr></table>",
        radio("t", "*INFO", true),
        radio("t", "*INQ", false)
    );
    s.push_str("<div class=\"buttons\"><input type=\"submit\" name=\"send\" value=\"Send\"></div>");
    form(&rq.ctx, "", &s)
}

/// My messages' POST: `SNDMSG` to a user or a queue; an inquiry's
/// reply comes to the user's own queue.
fn send(rq: &Rq<'_>, lib: &str, name: &str) -> Response {
    let f = rq.form();
    let toq = f.str("toq").trim().to_ascii_uppercase();
    let to = f.str("to").trim().to_ascii_uppercase();
    let mut c = format!("SNDMSG MSG({})", quote(f.str("m").trim()));
    if toq.is_empty() {
        let _ = write!(c, " TOUSR({})", if to.is_empty() { "*SYSOPR" } else { &to });
    } else {
        let _ = write!(c, " TOMSGQ({toq})");
    }
    if f.str("t") == "*INQ" {
        let _ = write!(c, " MSGTYPE(*INQ) RPYMSGQ({lib}/{name})");
    }
    run(rq, &c, "/mymsgs")
}

/// `/msgq/<lib>/<name>/<key>/remove`: confirm, then `RMVMSG`.
pub fn remove(rq: &Rq<'_>, lib: &str, name: &str, key: &str) -> Response {
    let back = back(rq, lib, name);
    let ms = match read(rq, lib, name) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(m) = ms.into_iter().find(|m| m.key.eq_ignore_ascii_case(key)) else {
        return rq.done(&back, rq.message(Kind::Error, "CPF2410", &[key]));
    };
    let command = format!("RMVMSG MSGQ({lib}/{name}) MSGKEY({})", m.key);
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect(&back);
        }
        return run(rq, &command, &back);
    }
    let (d, t) = rq.stamp(m.sent);
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what: "Remove Message",
            crumb: &[name],
            sentence: "Press OK to remove the message listed below from its queue, or Cancel to return without removing it.".into(),
            cols: &["Date/time", "Sev", "Message ID", "Message text", "Queue"],
            rows: vec![vec![
                format!("{d} {t}"),
                format!("{:02}", m.severity),
                mono(&m.id),
                esc(&text(&m)),
                mono(&format!("{lib}/{name}")),
            ]],
            options: String::new(),
            command,
        },
    )
}

/// `/msgq/<lib>/<name>/clear`: confirm, then `CLRMSGQ`, keeping the
/// inquiries waiting for a reply unless told otherwise.
pub fn clear(rq: &Rq<'_>, lib: &str, name: &str) -> Response {
    let ms = match read(rq, lib, name) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let back = format!("/msgq/{lib}/{name}");
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect(&back);
        }
        let which = if f.str("clr") == "*ALL" {
            "*ALL"
        } else {
            "*KEEPUNANS"
        };
        return run(
            rq,
            &format!("CLRMSGQ MSGQ({lib}/{name}) CLEAR({which})"),
            &back,
        );
    }
    let waiting = ms.iter().filter(|m| m.waiting()).count();
    let options = format!(
        "<p>{} Keep inquiry messages waiting for a reply &nbsp; {} Remove every message</p>",
        radio("clr", "*KEEPUNANS", true),
        radio("clr", "*ALL", false)
    );
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what: "Clear Message Queue",
            crumb: &[name],
            sentence: "Clearing a message queue removes its messages; they cannot be displayed again. Press OK to clear the queue listed below, or Cancel to return without clearing it.".into(),
            cols: &["Message queue", "Messages", "Inquiries waiting"],
            rows: vec![vec![mono(&format!("{lib}/{name}")), ms.len().to_string(), waiting.to_string()]],
            options,
            command: format!("CLRMSGQ MSGQ({lib}/{name}) CLEAR(*KEEPUNANS)"),
        },
    )
}
