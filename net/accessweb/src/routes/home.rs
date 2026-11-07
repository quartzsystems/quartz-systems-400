// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! My home page: the welcome line (the previous sign-on, the counts)
//! and a link to each part of the product the policies leave the user.
//! The only page with a 32x32 glyph per link (a letter square here).

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;

use super::Rq;
use super::signon::{PREV_AT, PREV_FROM};
use crate::prefs::{COUNTS, LAYOUT};
use crate::{PRODUCT, policy};

/// A section: glyph letter, label, route, description, the function
/// that must be allowed for it to show (`None`: always).
const SECTIONS: &[(&str, &str, &str, &str, Option<&str>)] = &[
    (
        "5",
        "5250 session",
        "/5250",
        "Start a 5250 display session to {system} in your browser.",
        Some(policy::S5250_START),
    ),
    (
        "D",
        "Database",
        "/db/tables",
        "Browse tables, run SQL statements, copy data to and from tables.",
        Some(policy::DB_BROWSE),
    ),
    (
        "F",
        "Files",
        "/files",
        "Browse, upload, download and edit files in the integrated file system.",
        Some(policy::FILES_BROWSE),
    ),
    (
        "P",
        "Print",
        "/print",
        "View your printer output as text or PDF, and manage printers.",
        Some(policy::PRINT_TEXT),
    ),
    (
        "M",
        "Messages",
        "/messages",
        "Display your messages, send messages, reply to inquiry messages.",
        Some(policy::MSGS_DISPLAY),
    ),
    (
        "J",
        "Jobs",
        "/jobs",
        "Work with your jobs and the Access for Web server jobs.",
        Some(policy::JOBS_DISPLAY),
    ),
    (
        "C",
        "Commands",
        "/cmd",
        "Run CL commands, prompt for parameters, save commands you use often.",
        Some(policy::CMD_RUN),
    ),
    (
        "C",
        "Customize",
        "/preferences",
        "Preferences for this user; policies for administrators.",
        None,
    ),
    (
        "D",
        "Download",
        "/download",
        "Printer drivers, the terminal font and other client pieces.",
        None,
    ),
];

/// `1 message` or `3 messages`.
fn count(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `GET /home`.
pub fn page(rq: &Rq<'_>) -> Response {
    let env = rq.env();
    let user = rq.user();
    let system = env.system.name();
    let prefs = rq.prefs();
    let mut body = String::new();
    let text = env.system.text(user);
    let name = if text.trim().is_empty() {
        user.to_string()
    } else {
        text
    };
    let _ = write!(
        body,
        "<p>Welcome, <b>{}</b> (<span class=\"mono\">{}</span>).",
        esc(&name),
        esc(user)
    );
    let at: u64 = rq.get(PREV_AT).parse().unwrap_or(0);
    if at == 0 {
        body.push_str(" This is your first sign-on.");
    } else {
        let from = rq.get(PREV_FROM);
        let _ = write!(body, " Last sign-on {}", esc(&rq.when(at)));
        if !from.is_empty() {
            let _ = write!(body, " from {}", esc(&from));
        }
        body.push('.');
    }
    if prefs.on(COUNTS) {
        let c = env.system.counts(user);
        let _ = write!(
            body,
            " You have <a href=\"{}\">{}</a>, <a href=\"{}\">{}</a> and <a href=\"{}\">{}</a>.",
            esc(&PRODUCT.url("/messages")),
            count(c.messages, "message", "messages"),
            esc(&PRODUCT.url("/print")),
            count(c.spooled, "spooled file", "spooled files"),
            esc(&PRODUCT.url("/jobs")),
            count(c.active_jobs, "active job", "active jobs"),
        );
    }
    body.push_str("</p>");
    let shown: Vec<_> = SECTIONS
        .iter()
        .filter(|s| s.4.is_none_or(|f| rq.allowed(f)))
        .collect();
    if prefs.get(LAYOUT) == "compact" {
        let links: Vec<String> = shown
            .iter()
            .map(|(_, label, route, _, _)| {
                format!(
                    "<a href=\"{}\">{}</a>",
                    esc(&PRODUCT.url(route)),
                    esc(label)
                )
            })
            .collect();
        let _ = write!(body, "<p>{}</p>", links.join(" | "));
    } else {
        body.push_str("<table class=\"home\" cellspacing=\"0\">");
        for (glyph, label, route, desc, _) in shown {
            let _ = write!(
                body,
                "<tr><td width=\"40\" align=\"center\"><span class=\"glyph\" style=\"font-size:16pt; border:1px solid #999; padding:2px 6px; background:#f0f0f0\">{glyph}</span></td><td><a class=\"big\" href=\"{}\">{}</a><br><span class=\"small grey\">{}</span></td></tr>",
                esc(&PRODUCT.url(route)),
                esc(label),
                esc(&desc.replace("{system}", &system)),
            );
        }
        body.push_str("</table><p class=\"small grey\">The letter squares stand in for the 32&times;32 glyphs allowed on this page only.</p>");
    }
    rq.page(Page {
        title: "My home page".into(),
        crumb: vec!["My home page".into()],
        h1: "My home page".into(),
        desc: Some(format!("QS/400 Access for Web on system {system}.")),
        body,
        help: "home".into(),
        nav: Some(("My home page", Some("/home"))),
        ..Page::default()
    })
}
