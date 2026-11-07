// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages (handoff step 16), the desktop's Messages window: the
//! user's own queue, `QSYSOPR` or any other, 100 a page newest first,
//! inquiries waiting for a reply in bold; Reply (`SNDRPYMSG`, an empty
//! reply sends the default), Remove (`RMVMSG`; an unanswered inquiry
//! asks twice), Send (`SNDMSG`), all run as the signed-on user;
//! Message Arrived ([`watch`]) and Send message.

pub mod watch;

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::pages::confirm_body;
use qsweb::partials::{Cell, Col, List, Paging, input, input_cmd, list_table, paging, radio};

use super::Rq;
use crate::env::msgs::QueuedMessage;
use crate::{PRODUCT, msgs, policy};

/// The tree's group (a leaf).
const GROUP: &str = "Messages";
/// The breadcrumb.
const CRUMB: &[&str] = &["Messages"];
/// Rows per page unless asked otherwise (the desktop's 100).
const ROWS: &str = "100";

/// Route the area's first path segments.
pub fn route(rq: &Rq<'_>, segs: &[&str]) -> Response {
    if let Err(r) = rq.policy(policy::MSGS_DISPLAY, CRUMB) {
        return r;
    }
    match segs {
        ["messages"] => list(rq),
        ["messages", "send"] => send_page(rq),
        ["messages", "arrived"] => arrived(rq),
        ["messages", key, "confirm"] => remove_inquiry(rq, key),
        _ => rq.not_found(&rq.req.path),
    }
}

/// A CL string literal.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// A message ID as a link to its second-level text (red when `err`).
pub fn id_link(id: &str, err: bool) -> String {
    if id.is_empty() {
        return String::new();
    }
    let class = if err { " class=\"err\"" } else { "" };
    format!(
        "<a{class} href=\"{}\" target=\"_blank\">{}</a>",
        esc(&PRODUCT.msg(id)),
        esc(id)
    )
}

/// A page of Navigator for QS/400 (port 2002, 2005 with TLS) on the
/// host the browser reached this one by; a path when it named none.
pub fn navigator(rq: &Rq<'_>, route: &str) -> String {
    let path = format!("/qs400/navigator{route}");
    let Some(host) = rq.req.header("host").filter(|h| !h.is_empty()) else {
        return path;
    };
    let name = match host.strip_prefix('[') {
        Some(v6) => format!("[{}]", v6.split(']').next().unwrap_or("")),
        None => host.split(':').next().unwrap_or(host).to_string(),
    };
    let (scheme, port) = if rq.peer.secure { ("https", 2005) } else { ("http", 2002) };
    format!("{scheme}://{name}:{port}{path}")
}

/// A confirmation page: `sentence`, the table, OK and Cancel; `hidden`
/// is HTML inside the form.
pub fn confirm(rq: &Rq<'_>, title: &str, crumb: &[&str], sentence: &str, table: &str, hidden: &str, nav: (&'static str, Option<&'static str>)) -> Response {
    let mut c: Vec<String> = crumb.iter().map(ToString::to_string).collect();
    c.push(title.into());
    rq.page(Page {
        title: title.into(),
        crumb: c,
        h1: title.into(),
        body: confirm_body(&rq.ctx, sentence, table, hidden),
        help: "confirm".into(),
        nav: Some(nav),
        ..Page::default()
    })
}

/// The queue the page shows: from `?q=` (`My queue (...)`, `QSYSOPR`,
/// `Other:` with `ql`/`qnm`).
fn chosen(rq: &Rq<'_>) -> (String, String) {
    let q = rq.query();
    match q.str("q") {
        "QSYSOPR" => (watch::QSYSOPR.0.into(), watch::QSYSOPR.1.into()),
        s if s.starts_with("Other") && !q.str("qnm").trim().is_empty() => {
            let lib = q.str("ql").trim().to_ascii_uppercase();
            let lib = if lib.is_empty() { "*LIBL".to_string() } else { lib };
            (lib, q.str("qnm").trim().to_ascii_uppercase())
        }
        _ => rq.env().msgs.own_queue(rq.user()),
    }
}

/// The text a list shows: a message a user typed says who typed it.
fn text_of(m: &QueuedMessage) -> String {
    if m.id.is_empty() && !m.user.is_empty() {
        format!("From {}: {}", m.user, m.text)
    } else {
        m.text.clone()
    }
}

/// Who sent it: the user who typed it, else the job.
fn from_of(m: &QueuedMessage) -> &str {
    if m.id.is_empty() && !m.user.is_empty() { &m.user } else { &m.job }
}

/// The queue's messages or the page that says why not.
fn read(rq: &Rq<'_>, lib: &str, name: &str) -> Result<Vec<QueuedMessage>, Response> {
    rq.env().msgs.messages(rq.user(), lib, name).map_err(|m| {
        rq.page(Page {
            title: "Messages".into(),
            crumb: vec!["Messages".into()],
            h1: "Messages".into(),
            messages: vec![m],
            body: format!("<p><a href=\"{}\">My queue</a></p>", esc(&PRODUCT.url("/messages"))),
            help: "messages".into(),
            nav: Some((GROUP, Some("/messages"))),
            ..Page::default()
        })
    })
}

/// Run `command`; its escape message, or `done` when it completes.
fn run(rq: &Rq<'_>, command: &str, done: Message, back: &str) -> Response {
    match rq.env().msgs.run(rq.user(), command) {
        Ok(_) => rq.done(back, done),
        Err(m) => rq.done(back, m),
    }
}

/// `/messages` with the current queue choice kept.
fn here_url(rq: &Rq<'_>) -> String {
    let q = rq.query();
    let keep: Vec<(&str, &str)> = ["q", "ql", "qnm"]
        .iter()
        .filter_map(|k| q.get(k).filter(|v| !v.is_empty()).map(|v| (*k, v)))
        .collect();
    format!("/messages{}", qs(&keep))
}

/// A select of `choices` (label, code), `code` chosen.
fn select_of(name: &str, choices: &[(&str, &str)], code: &str) -> String {
    let mut s = format!("<select name=\"{name}\">");
    for (i, (label, c)) in choices.iter().enumerate() {
        let sel = if i > 0 && *c == code { " selected" } else { "" };
        let _ = write!(s, "<option{sel}>{label}</option>");
    }
    s.push_str("</select>");
    s
}

/// `GET /messages`; the Reply, Send and Remove posts.
#[allow(clippy::too_many_lines)]
fn list(rq: &Rq<'_>) -> Response {
    let (lib, name) = chosen(rq);
    let qual = format!("{lib}/{name}");
    let back = here_url(rq);
    if rq.post() {
        return post(rq, &lib, &name, &back);
    }
    let q = rq.query();
    // The watch options are the session's.
    for (field, key, choices) in [("w", watch::WATCH, &watch::WATCH_CHOICES), ("a", watch::ARRIVED, &watch::ARRIVED_CHOICES)] {
        if let Some(c) = q.get(field).and_then(|v| choices.iter().find(|x| x.0 == v)) {
            rq.set(&[(key, c.1)]);
        }
    }
    let ms = match read(rq, &lib, &name) {
        Ok(v) => v,
        Err(r) => return r,
    };
    watch::seen(rq, &qual, ms.iter().map(QueuedMessage::order).max().unwrap_or(0));
    let own = rq.env().msgs.own_queue(rq.user());
    let mut body = String::new();
    let mut messages = Vec::new();
    // Remove, asked once: an informational message or an answered inquiry.
    if let Some(key) = q.get("remove")
        && let Some(m) = ms.iter().find(|m| m.key.eq_ignore_ascii_case(key))
    {
        let _ = write!(
            body,
            "<form method=\"post\">{}<p>Remove the message of {} from {}? <input type=\"hidden\" name=\"remove\" value=\"{}\"><input type=\"submit\" name=\"ok\" value=\"Remove\"> <input type=\"submit\" name=\"cancel\" value=\"Cancel\"></p></form>",
            rq.ctx.csrf_field(),
            esc(&rq.when(m.sent)),
            esc(&qual),
            esc(&m.key)
        );
    } else if q.has("remove") {
        messages.push(rq.message(Kind::Error, "CPF2410", &[q.str("remove"), &name, &lib]));
    }
    // The filter: the queue, the counts, the watch options.
    let waiting = ms.iter().filter(|m| m.waiting()).count();
    let opr = qual == format!("{}/{}", watch::QSYSOPR.0, watch::QSYSOPR.1);
    let other = !opr && (lib.clone(), name.clone()) != own;
    let mine = format!("My queue ({}/{})", own.0, own.1);
    let options = [mine.as_str(), "QSYSOPR", "Other:"];
    let pick = if opr { 1 } else if other { 2 } else { 0 };
    let _ = write!(body, "<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr><td>Queue:</td><td><select name=\"q\">");
    for (i, o) in options.iter().enumerate() {
        let sel = if i == pick && i > 0 { " selected" } else { "" };
        let _ = write!(body, "<option{sel}>{}</option>", esc(o));
    }
    let (ql, qn) = if other { (lib.as_str(), name.as_str()) } else { ("", "") };
    let _ = write!(
        body,
        "</select> {}/{}</td><td><input type=\"submit\" value=\"Go\"> <input type=\"submit\" name=\"refresh\" value=\"Refresh\"> &nbsp; <span class=\"small\">Messages waiting: <b>{}</b>, {waiting} {} &nbsp; Watch: {} Arrived page for {}</span></td></tr></table></form>",
        input("ql", ql, 10),
        input("qnm", qn, 10),
        ms.len(),
        if waiting == 1 { "inquiry" } else { "inquiries" },
        select_of("w", &watch::WATCH_CHOICES, &rq.get(watch::WATCH)),
        select_of("a", &watch::ARRIVED_CHOICES, &rq.get(watch::ARRIVED)),
    );
    // The list, newest first unless sorted by ID.
    let mut sorted = ms.clone();
    if q.str("sort") == "id" {
        sorted.sort_by(|a, b| a.id.cmp(&b.id).then(b.order().cmp(&a.order())));
    } else {
        sorted.sort_by_key(|m| std::cmp::Reverse(m.order()));
    }
    let keep: Vec<(String, String)> = ["q", "ql", "qnm", "sort"]
        .iter()
        .map(|k| ((*k).to_string(), q.str(k).to_string()))
        .filter(|(_, v)| !v.is_empty())
        .collect();
    let rows = q.get("rows").unwrap_or(ROWS);
    let p = Paging::from_query(q.str("page"), rows, sorted.len(), keep.clone());
    let reply_ok = rq.allowed(policy::MSGS_REPLY);
    let keepq: Vec<(&str, &str)> = keep.iter().filter(|(k, _)| k != "sort").map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let rows: Vec<Vec<Cell>> = sorted[p.range()]
        .iter()
        .map(|m| {
            let mut acts = vec![format!(
                "<a href=\"{}\">Details</a>",
                esc(&navigator(rq, &format!("/msgq/{lib}/{name}/{}", m.key)))
            )];
            let with = |k: &str| {
                let mut v = keepq.clone();
                v.push((k, &m.key));
                PRODUCT.url(&format!("/messages{}", query(&v)))
            };
            if m.waiting() && reply_ok {
                acts.push(format!("<a href=\"{}\">Reply</a>", esc(&with("reply"))));
            }
            let remove = if m.waiting() {
                PRODUCT.url(&format!("/messages/{}/confirm{}", m.key, qs(&keepq)))
            } else {
                with("remove")
            };
            acts.push(format!("<a href=\"{}\">Remove</a>", esc(&remove)));
            let mut text = esc(&text_of(m));
            if m.waiting() {
                text = format!("<b>{text}</b>");
            } else if let Some(r) = &m.reply {
                let _ = write!(text, " &nbsp; <span class=\"small grey\">Reply: {}</span>", esc(r));
            }
            vec![
                Cell::act(acts.join(" | ")),
                Cell::mono(&rq.when(m.sent)),
                Cell::num(&format!("{:02}", m.severity)),
                Cell::mono_html(id_link(&m.id, false)),
                Cell::html(text),
                Cell::html(format!("<span class=\"mono\">{}</span>", esc(from_of(m)))),
                Cell::text(if m.inquiry { "Inquiry" } else { "Info" }),
            ]
        })
        .collect();
    body.push_str(&list_table(&List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Date/time", "d"),
            Col::plain("Sev"),
            Col::sorted("Message ID", "id"),
            Col::plain("Message text"),
            Col::plain("From"),
            Col::plain("Type"),
        ],
        rows,
        empty: format!("No messages on {qual}."),
        keep: keepq.iter().map(|(a, b)| ((*a).to_string(), (*b).to_string())).collect(),
    }));
    body.push_str(&paging(&p));
    body.push_str(&bottom(rq, &ms, q.get("reply")));
    rq.page(Page {
        title: "Messages".into(),
        crumb: vec!["Messages".into()],
        h1: "Messages".into(),
        desc: Some("Your messages (DSPMSG), as QS/400 Access for Desktop's Messages window lists them.".into()),
        messages,
        body,
        help: "messages".into(),
        nav: Some((GROUP, Some("/messages"))),
        ..Page::default()
    })
}

/// The form under the list: Reply to the inquiry asked for (or the
/// newest waiting) and Send message, as the policies allow.
fn bottom(rq: &Rq<'_>, ms: &[QueuedMessage], want: Option<&str>) -> String {
    let inquiry = ms
        .iter()
        .filter(|m| m.waiting())
        .filter(|m| want.is_none_or(|k| m.key.eq_ignore_ascii_case(k)))
        .max_by_key(|m| m.order())
        .filter(|_| rq.allowed(policy::MSGS_REPLY));
    let send = rq.allowed(policy::MSGS_SEND);
    if inquiry.is_none() && !send {
        return String::new();
    }
    let mut s = format!("<form method=\"post\">{}<table width=\"100%\"><tr>", rq.ctx.csrf_field());
    if let Some(m) = inquiry {
        let valid = m.replies();
        let hint = if valid.is_empty() { "empty = the default".to_string() } else { format!("valid: {valid}; empty = the default") };
        let what = if m.id.is_empty() { m.key.as_str() } else { m.id.as_str() };
        let _ = write!(
            s,
            "<td width=\"50%\" valign=\"top\"><h3>Reply to inquiry {}</h3><input type=\"hidden\" name=\"key\" value=\"{}\">Reply: {} <span class=\"small grey\">{}</span><div class=\"buttons\"><input type=\"submit\" name=\"reply\" value=\"Reply\"></div></td>",
            esc(what),
            esc(&m.key),
            input_cmd("r", &m.default, 10),
            esc(&hint)
        );
    }
    if send {
        // To user starts as the sender of the newest message a user typed.
        let to = ms
            .iter()
            .filter(|m| m.id.is_empty() && !m.user.is_empty() && m.user != rq.user())
            .max_by_key(|m| m.order())
            .map_or("", |m| m.user.as_str());
        let _ = write!(
            s,
            "<td valign=\"top\"><h3>Send message</h3><table class=\"props\"><tr><th>To</th><td>{} user {} {} queue {}</td></tr><tr><th>Message text</th><td><textarea name=\"m\" rows=\"3\" cols=\"50\"></textarea></td></tr><tr><th>Inquiry</th><td>{} No {} Yes, reply to my queue (the reply shows on this page when it comes)</td></tr></table><div class=\"buttons\"><input type=\"submit\" name=\"send\" value=\"Send\"></div></td>",
            radio("to", "user", true),
            input("tou", to, 10),
            radio("to", "queue", false),
            input_cmd("toq", "", 20),
            radio("i", "no", true),
            radio("i", "yes", false),
        );
    }
    s.push_str("</tr></table></form>");
    s
}

/// The list's posts: Reply, Send, Remove (asked once).
fn post(rq: &Rq<'_>, lib: &str, name: &str, back: &str) -> Response {
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect(back);
    }
    if f.has("reply") {
        return reply(rq, lib, name, f.str("key"), f.str("r"), back);
    }
    if f.has("send") {
        let to = if f.str("to") == "queue" { f.str("toq") } else { f.str("tou") };
        return send(rq, to, f.str("to") == "queue", f.str("m"), f.str("i") == "yes", back);
    }
    if f.has("remove") {
        let Ok(ms) = rq.env().msgs.messages(rq.user(), lib, name) else {
            return rq.redirect(back);
        };
        let Some(m) = ms.into_iter().find(|m| m.key.eq_ignore_ascii_case(f.str("remove"))) else {
            return rq.done(back, rq.message(Kind::Error, "CPF2410", &[f.str("remove"), name, lib]));
        };
        if m.waiting() {
            // An unanswered inquiry is asked about twice, on its own page.
            return rq.redirect(&format!("/messages/{}/confirm", m.key));
        }
        return remove(rq, lib, name, &m.key, back);
    }
    rq.redirect(back)
}

/// `SNDRPYMSG`: an empty reply sends the inquiry's default.
fn reply(rq: &Rq<'_>, lib: &str, name: &str, key: &str, r: &str, back: &str) -> Response {
    if let Err(resp) = rq.policy(policy::MSGS_REPLY, CRUMB) {
        return resp;
    }
    let Ok(ms) = rq.env().msgs.messages(rq.user(), lib, name) else {
        return rq.redirect(back);
    };
    let Some(m) = ms.into_iter().find(|m| m.key.eq_ignore_ascii_case(key)) else {
        return rq.done(back, rq.message(Kind::Error, "CPF2410", &[key, name, lib]));
    };
    let r = r.trim();
    let rpy = if r.is_empty() { "*DFT".to_string() } else { quote(r) };
    let what = if m.id.is_empty() { m.key.clone() } else { m.id.clone() };
    let done = rq.message(Kind::Done, msgs::MSG_REPLY_SENT, &[&what]);
    run(rq, &format!("SNDRPYMSG MSGKEY({}) MSGQ({lib}/{name}) RPY({rpy}) RMV(*NO)", m.key), done, back)
}

/// `RMVMSG` of one message.
fn remove(rq: &Rq<'_>, lib: &str, name: &str, key: &str, back: &str) -> Response {
    let done = rq.message(Kind::Done, msgs::MSG_REMOVED, &["1", &format!("{lib}/{name}")]);
    run(rq, &format!("RMVMSG MSGQ({lib}/{name}) MSGKEY({key})"), done, back)
}

/// `SNDMSG` to a user or a queue; an inquiry's reply comes to the
/// user's own queue.
fn send(rq: &Rq<'_>, to: &str, queue: bool, text: &str, inquiry: bool, back: &str) -> Response {
    if let Err(r) = rq.policy(policy::MSGS_SEND, CRUMB) {
        return r;
    }
    if inquiry && let Err(r) = rq.policy(policy::MSGS_INQUIRY, CRUMB) {
        return r;
    }
    let to = to.trim().to_ascii_uppercase();
    let text = text.trim();
    if text.is_empty() {
        return rq.done(back, rq.message(Kind::Error, msgs::BAD_VALUE, &["", "Message text"]));
    }
    if to.is_empty() {
        return rq.done(back, rq.message(Kind::Error, msgs::BAD_VALUE, &["", if queue { "To queue" } else { "To user" }]));
    }
    let mut c = format!("SNDMSG MSG({})", quote(text));
    if queue {
        let _ = write!(c, " TOMSGQ({to})");
    } else {
        let _ = write!(c, " TOUSR({to})");
    }
    if inquiry {
        let (ol, on) = rq.env().msgs.own_queue(rq.user());
        let _ = write!(c, " MSGTYPE(*INQ) RPYMSGQ({ol}/{on})");
    }
    let done = rq.message(Kind::Done, msgs::MSG_SENT, &[&to]);
    run(rq, &c, done, back)
}

/// `/messages/<key>/confirm`: Remove of an inquiry still waiting for its
/// reply, asked twice.
fn remove_inquiry(rq: &Rq<'_>, key: &str) -> Response {
    let (lib, name) = chosen(rq);
    let back = here_url(rq);
    let ms = match read(rq, &lib, &name) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(m) = ms.into_iter().find(|m| m.key.eq_ignore_ascii_case(key)) else {
        return rq.done(&back, rq.message(Kind::Error, "CPF2410", &[key, &name, &lib]));
    };
    let f = rq.form();
    if rq.post() && f.has("cancel") {
        return rq.redirect(&back);
    }
    let again = rq.post() && f.has("ok");
    if again && (f.has("again") || !m.waiting()) {
        return remove(rq, &lib, &name, &m.key, &back);
    }
    let table = format!(
        "<table class=\"list\" cellspacing=\"0\" cellpadding=\"0\">\n<tr>\n<th>Date/time</th>\n<th>Sev</th>\n<th>Message ID</th>\n<th>Message text</th>\n<th>From</th>\n</tr>\n<tr>\n<td class=\"mono\">{}</td>\n<td class=\"num\">{:02}</td>\n<td class=\"mono\">{}</td>\n<td>{}</td>\n<td><span class=\"mono\">{}</span></td>\n</tr>\n</table><p class=\"small\">Equivalent command: <span class=\"mono\">RMVMSG MSGQ({}) MSGKEY({})</span></p>",
        esc(&rq.when(m.sent)),
        m.severity,
        esc(&m.id),
        esc(&text_of(&m)),
        esc(from_of(&m)),
        esc(&format!("{lib}/{name}")),
        esc(&m.key),
    );
    let (sentence, hidden) = if again {
        (
            "This inquiry has not been answered: once it is removed its sender gets no reply and waits until it ends some other way. Press OK again to remove it, or Cancel to keep it.",
            "<input type=\"hidden\" name=\"again\" value=\"1\">",
        )
    } else {
        ("Press OK to remove the inquiry listed below from its queue, or Cancel to return without removing it.", "")
    };
    confirm(rq, "Confirm Remove Message", CRUMB, sentence, &table, hidden, (GROUP, None))
}

/// `/messages/send`: Send message on its own page.
fn send_page(rq: &Rq<'_>) -> Response {
    if let Err(r) = rq.policy(policy::MSGS_SEND, CRUMB) {
        return r;
    }
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/messages");
        }
        let (to, queue) = if f.str("toq").trim().is_empty() { (f.str("to"), false) } else { (f.str("toq"), true) };
        return send(rq, to, queue, f.str("m"), f.str("i") == "yes", "/messages");
    }
    let body = format!(
        "<form method=\"post\">{}<table class=\"props\" cellspacing=\"0\">\n<tr><th>To user</th><td>{} or queue {}</td></tr>\n<tr><th>Message text</th><td><textarea name=\"m\" rows=\"4\" cols=\"70\"></textarea></td></tr>\n<tr><th>Inquiry</th><td>{} No (informational)<br>{} Yes - reply to my message queue</td></tr>\n<tr><th>Equivalent command</th><td><span class=\"mono\">SNDMSG MSG(...) TOUSR(...) MSGTYPE(*INFO)</span></td></tr>\n</table>\n<div class=\"buttons\"><input type=\"submit\" name=\"send\" value=\"Send\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\"></div></form>",
        rq.ctx.csrf_field(),
        input("to", "", 10),
        input("toq", "", 10),
        radio("i", "no", true),
        radio("i", "yes", false),
    );
    rq.page(Page {
        title: "Send message".into(),
        crumb: vec!["Messages".into(), "Send message".into()],
        h1: "Send message".into(),
        body,
        help: "messages".into(),
        nav: Some((GROUP, None)),
        ..Page::default()
    })
}

/// `/messages/arrived`: the first message that arrived on a watched
/// queue (the newest inquiry waiting when none did), Reply and Later.
fn arrived(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let back = q
        .get("back")
        .filter(|b| b.starts_with(PRODUCT.prefix) && !b.contains("//") && !b.contains("/messages/arrived"))
        .map_or_else(|| "/messages".into(), |b| b.strip_prefix(PRODUCT.prefix).unwrap_or(b).to_string());
    if rq.post() {
        let f = rq.form();
        let key = f.str("key");
        let qual = f.str("queue");
        let (lib, name) = qual.split_once('/').unwrap_or(("", qual));
        watch::acknowledged(rq, qual, u64::from_str_radix(key, 16).unwrap_or(0));
        if f.has("reply") {
            return reply(rq, lib, name, key, f.str("r"), &back);
        }
        return rq.redirect(&back);
    }
    let found = watch::first_arrived(rq).or_else(|| {
        let (lib, name) = rq.env().msgs.own_queue(rq.user());
        let ms = rq.env().msgs.messages(rq.user(), &lib, &name).ok()?;
        ms.into_iter().filter(QueuedMessage::waiting).max_by_key(QueuedMessage::order).map(|m| (format!("{lib}/{name}"), m))
    });
    let Some((qual, m)) = found else {
        return rq.redirect("/messages");
    };
    let (_, time) = rq.stamp(m.sent);
    let what = if m.inquiry { "Inquiry message" } else { "Message" };
    let sender = if m.id.is_empty() && !m.user.is_empty() {
        format!("user {}", m.user)
    } else {
        format!("job {}", m.job)
    };
    let line = format!("{what} from {sender} arrived at {time}.");
    let row = if m.id.is_empty() {
        format!("<table class=\"msg\"><tr><td class=\"info\">{}</td></tr></table>", esc(&line))
    } else {
        format!(
            "<table class=\"msg\"><tr><td class=\"info\"><a class=\"msgid\" href=\"{}\" target=\"_blank\">{}</a> - {}</td></tr></table>",
            esc(&PRODUCT.msg(&m.id)),
            esc(&m.id),
            esc(&line)
        )
    };
    let mut body = format!(
        "{row}<form method=\"post\">{}<input type=\"hidden\" name=\"key\" value=\"{}\"><input type=\"hidden\" name=\"queue\" value=\"{}\"><table class=\"props\" cellspacing=\"0\">\n<tr><th>From job</th><td><span class=\"mono\">{}</span></td></tr>\n<tr><th>Severity</th><td>{:02}</td></tr>\n<tr><th>Message</th><td><pre class=\"out\">{}</pre></td></tr>\n",
        rq.ctx.csrf_field(),
        esc(&m.key),
        esc(&qual),
        esc(&m.job),
        m.severity,
        esc(&text_of(&m)),
    );
    let can_reply = m.waiting() && rq.allowed(policy::MSGS_REPLY);
    if can_reply {
        let valid = m.replies();
        let _ = writeln!(
            body,
            "<tr><th>Reply</th><td>{} <span class=\"small grey\">{}</span></td></tr>",
            input_cmd("r", "", 10),
            esc(&if valid.is_empty() { "empty = the default".to_string() } else { format!("valid: {valid}") })
        );
    }
    body.push_str("</table>\n<div class=\"buttons\">");
    if can_reply {
        body.push_str("<input type=\"submit\" name=\"reply\" value=\"Reply\">");
    }
    body.push_str("<input type=\"submit\" name=\"later\" value=\"Later\"></div></form>");
    rq.page(Page {
        title: "Message arrived".into(),
        crumb: vec!["Messages".into(), "Message arrived".into()],
        h1: "Message arrived".into(),
        body,
        help: "messages".into(),
        nav: Some((GROUP, Some("/messages"))),
        ..Page::default()
    })
}

/// A query string, empty when no pair has a value.
fn qs(pairs: &[(&str, &str)]) -> String {
    if pairs.iter().all(|(_, v)| v.is_empty()) { String::new() } else { query(pairs) }
}
