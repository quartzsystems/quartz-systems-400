// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/netsvr`: whether the server runs, Start (`QZLSSTRS`, with reset
//! to make the pending attributes current) and End (`QZLSENDS`,
//! confirmed at `/netsvr/confirm`); the attributes, current and
//! pending, and Apply, which makes the values typed pending
//! (`QZLSCHSN`, `QZLSCHSG`, `QZLSCHSI`).

use qsweb::html::esc;
use qsweb::http::{Form, Response};
use qsweb::partials::{Button, Cell, Col, List, buttons, form, input, list_table, mono};

use super::{GROUP, answer, call, need, ns_tabs, page, refused, yes};
use crate::env::netsvr::Settings;
use crate::routes::Rq;
use crate::routes::work::{Confirm, confirm_in, pick, strings, word};

const DIALECTS: &[&str] = &["2.0.2", "2.1", "3.0", "3.0.2", "3.1.1"];

fn status_word(active: bool) -> String {
    if active {
        word("Active", Some(true))
    } else {
        word("Inactive", Some(false))
    }
}

/// An integer field; one that does not read goes to the API as a value
/// it refuses, so its message names the field.
fn int(f: &Form, name: &str) -> i32 {
    f.str(name).trim().parse().unwrap_or(i32::MIN)
}

fn upper(f: &Form, name: &str) -> String {
    f.str(name).trim().to_ascii_uppercase()
}

/// The pending settings the form asks for, from `base`.
fn posted(f: &Form, base: &Settings) -> Settings {
    let mut s = base.clone();
    s.server_name = upper(f, "name");
    s.domain = upper(f, "domain");
    s.text = f.str("text").trim().to_string();
    s.system_name_access = f.str("sysname") == "*YES";
    let g = upper(f, "guest");
    s.guest = if g == "*NONE" { String::new() } else { g };
    s.code_page = int(f, "codepage");
    s.idle_timeout = int(f, "idle");
    s.oplock_timeout = int(f, "oplock");
    s.browse_interval = int(f, "browse");
    s.message_auth = int(f, "msgauth");
    s.encryption = int(f, "encrypt");
    let a = upper(f, "autl");
    s.server_autl = if a == "*NONE" { String::new() } else { a };
    s.allow_netbios = f.str("netbios") == "*YES";
    if let Some(d) = netsvrcfg::Dialect::parse(f.str("dialect")) {
        s.min_dialect = d;
    }
    s.log_signon = f.str("logon") == "*YES";
    s
}

fn none(s: &str) -> &str {
    if s.is_empty() { "*NONE" } else { s }
}

fn hint(h: &str) -> String {
    format!(" <span class=\"small grey\">{}</span>", esc(h))
}

/// The attribute rows: label, current, the pending value's field.
#[allow(clippy::too_many_lines)]
fn rows(c: &Settings, p: &Settings) -> Vec<(&'static str, String, String)> {
    let sel = |name: &str, b: bool| {
        // *NO first: the shipped value.
        pick(name, &strings(&["*NO", "*YES"]), yes(b))
    };
    vec![
        (
            "Server name",
            mono(&c.server_name),
            input("name", &p.server_name, 15),
        ),
        (
            "Domain name",
            mono(&c.domain),
            input("domain", &p.domain, 15),
        ),
        ("Text description", esc(&c.text), input("text", &p.text, 50)),
        (
            "Allow system name",
            mono(yes(c.system_name_access)),
            sel("sysname", p.system_name_access),
        ),
        (
            "Guest user profile",
            mono(none(&c.guest)),
            format!(
                "{}{}",
                input("guest", none(&p.guest), 10),
                hint("*NONE: no guest access")
            ),
        ),
        (
            "Code page",
            mono(&c.code_page.to_string()),
            format!(
                "{}{}",
                input("codepage", &p.code_page.to_string(), 5),
                hint("0: the job's")
            ),
        ),
        (
            "Idle time-out (seconds)",
            mono(&c.idle_timeout.to_string()),
            format!(
                "{}{}",
                input("idle", &p.idle_timeout.to_string(), 7),
                hint("-1: never")
            ),
        ),
        (
            "Oplock time-out (seconds)",
            mono(&c.oplock_timeout.to_string()),
            format!(
                "{}{}",
                input("oplock", &p.oplock_timeout.to_string(), 4),
                hint("1 to 600; -1: no leases")
            ),
        ),
        (
            "Browse interval (milliseconds)",
            mono(&c.browse_interval.to_string()),
            format!(
                "{}{}",
                input("browse", &p.browse_interval.to_string(), 7),
                hint("0 to 720000")
            ),
        ),
        (
            "Message authentication",
            mono(&c.message_auth.to_string()),
            format!(
                "{}{}",
                pick(
                    "msgauth",
                    &strings(&["0", "1", "2"]),
                    &p.message_auth.to_string()
                ),
                hint("0, 1: signing offered; 2: required")
            ),
        ),
        (
            "Encrypted connections",
            mono(&c.encryption.to_string()),
            format!(
                "{}{}",
                pick("encrypt", &strings(&["0", "1"]), &p.encryption.to_string()),
                hint("1: required for every connection")
            ),
        ),
        (
            "Server authorization list",
            mono(none(&c.server_autl)),
            input("autl", none(&p.server_autl), 10),
        ),
        (
            "Authentication method",
            "Passwords".into(),
            "Passwords".into(),
        ),
        (
            "Allow NetBIOS",
            mono(yes(c.allow_netbios)),
            sel("netbios", p.allow_netbios),
        ),
        (
            "SMB2 minimum dialect",
            mono(c.min_dialect.name()),
            pick("dialect", &strings(DIALECTS), p.min_dialect.name()),
        ),
        (
            "Log sign-on messages",
            mono(yes(c.log_signon)),
            sel("logon", p.log_signon),
        ),
    ]
}

/// The page; `typed` the pending values a refused Apply asked for.
fn show(rq: &Rq<'_>, typed: Option<&Settings>) -> Response {
    let st = rq.env().netsvr.status(rq.user());
    let sessions = rq.env().netsvr.sessions(rq.user());
    let attrs = match rq.env().netsvr.attributes(rq.user()) {
        Ok(a) => a,
        Err(m) => return rq.done("/home", m),
    };
    let users: usize = sessions.iter().map(|s| s.connections.len()).sum();
    let status = format!(
        "<table class=\"status\" cellspacing=\"0\">\
<tr><td class=\"lbl\">Status</td><td>{}</td></tr>\
<tr><td class=\"lbl\">Server job</td><td>{}</td></tr>\
<tr><td class=\"lbl\">Server name</td><td>{}</td></tr>\
<tr><td class=\"lbl\">Domain name</td><td>{}</td></tr>\
<tr><td class=\"lbl\">Sessions</td><td>{} &nbsp; <span class=\"grey\">{} connections</span></td></tr></table>",
        status_word(st.active),
        if st.job.is_empty() {
            "&nbsp;".to_string()
        } else {
            mono(&st.job)
        },
        mono(&attrs.current.server_name),
        mono(&attrs.current.domain),
        sessions.len(),
        users
    );
    let control = if st.active {
        vec![
            Button::new("end", "End..."),
            Button::new("refresh", "Refresh"),
        ]
    } else {
        vec![
            Button::new("start", "Start"),
            Button::new("reset", "Start and apply pending values"),
            Button::new("refresh", "Refresh"),
        ]
    };
    let pending = typed.unwrap_or(&attrs.pending);
    let list = List {
        cols: vec![
            Col::plain("Attribute"),
            Col::plain("Current"),
            Col::plain("Pending"),
        ],
        rows: rows(&attrs.current, pending)
            .into_iter()
            .map(|(l, c, p)| vec![Cell::text(l), Cell::html(c), Cell::html(p)])
            .collect(),
        empty: String::new(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}{status}{}<h2>Attributes</h2><p class=\"small grey\">Changes are pending: they take effect when NetServer is started with its pending values, or at the next IPL.</p>{}",
        ns_tabs(rq, 0),
        form(&rq.ctx, "", &buttons(&control)),
        form(
            &rq.ctx,
            "",
            &format!(
                "{}{}",
                list_table(&list),
                buttons(&[
                    Button::new("apply", "Apply"),
                    Button::new("cancel", "Cancel")
                ])
            )
        )
    );
    page(
        rq,
        &[],
        "NetServer",
        Some("SMB file and print serving (QZLSSERVER)."),
        body,
        "netsvr",
        Some("/netsvr"),
    )
}

/// `/netsvr`; its forms post here.
pub fn page_route(rq: &Rq<'_>) -> Response {
    if !rq.post() {
        return show(rq, None);
    }
    let f = rq.form();
    if f.has("end") {
        return rq.redirect("/netsvr/confirm?op=end");
    }
    if f.has("start") || f.has("reset") {
        let reset = f.has("reset");
        if let Err(r) = need(rq, "Start NetServer", "QZLSSTRS", "Server") {
            return r;
        }
        let r = rq.env().netsvr.start(rq.user(), reset);
        return answer(rq, r, "Start NetServer", "QZLSSTRS", "Server", "/netsvr");
    }
    if f.has("apply") {
        if let Err(r) = need(rq, "Change NetServer attributes", "QZLSCHSI", "Server") {
            return r;
        }
        let from = match rq.env().netsvr.attributes(rq.user()) {
            Ok(a) => a.pending,
            Err(m) => return rq.done("/netsvr", m),
        };
        let to = posted(&f, &from);
        return match rq.env().netsvr.change_attributes(rq.user(), &from, &to) {
            Ok(()) => rq.redirect("/netsvr"),
            Err(m) => {
                if let Some(r) =
                    refused(rq, &m, "Change NetServer attributes", "QZLSCHSI", "Server")
                {
                    return r;
                }
                rq.flash(m);
                show(rq, Some(&to))
            }
        };
    }
    rq.redirect("/netsvr")
}

/// `/netsvr/confirm?op=end`: End, confirmed.
pub fn end(rq: &Rq<'_>) -> Response {
    let command = call("QZLSENDS", &[], &[]);
    if rq.post() {
        if rq.form().has("cancel") {
            return rq.redirect("/netsvr");
        }
        if let Err(r) = need(rq, "End NetServer", "QZLSENDS", "Server") {
            return r;
        }
        let r = rq.env().netsvr.end(rq.user());
        return answer(rq, r, "End NetServer", "QZLSENDS", "Server", "/netsvr");
    }
    let st = rq.env().netsvr.status(rq.user());
    let n = rq.env().netsvr.sessions(rq.user()).len();
    confirm_in(
        rq,
        GROUP,
        &Confirm {
            what: "End NetServer",
            crumb: &["NetServer"],
            sentence: "Ending NetServer ends its jobs and every session; clients lose their open files. Press OK to end the server listed below, or Cancel to return.".into(),
            cols: &["Server job", "Status", "Sessions"],
            rows: vec![vec![mono(&st.job), status_word(st.active), n.to_string()]],
            options: String::new(),
            command,
        },
    )
}
