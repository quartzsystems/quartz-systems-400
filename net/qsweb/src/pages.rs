// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The generic pages, framed for a product: a message's second-level
//! text (`shared/error.html`), Not authorized (`shared/notauth.html`)
//! and confirmations (`shared/confirm.html`).

use std::fmt::Write as _;

use crate::frame::{Ctx, Page};
use crate::html::esc;
use crate::msg::{Kind, Message, SecondLevel};
use crate::partials::{Button, Prop, PropGroup, buttons, form, props_table};

/// Where a message came from, for the error page's details.
#[derive(Clone, Debug, Default)]
pub struct Sent {
    /// `Escape`, `Diagnostic`, `Information`, `Completion`.
    pub msg_type: String,
    /// `MM/DD/YY`.
    pub date: String,
    /// `HH:MM:SS`.
    pub time: String,
    /// The qualified job.
    pub job: String,
    /// The sending program.
    pub program: String,
    /// The technical description, system output.
    pub technical: String,
}

/// The error page: Message, Cause, Recovery, then the details. With no
/// `sent`, the bare second-level text (a message ID link).
pub fn second_level(ctx: &Ctx, sl: &SecondLevel, sent: Option<&Sent>, crumb: &str) -> Page {
    let mut body = String::new();
    let mut rows = vec![
        Prop::new("Message ID", esc(&sl.id)).mono(),
        Prop::new("Severity", format!("{:02}", sl.severity)),
    ];
    if let Some(s) = sent {
        rows.push(Prop::new("Message type", esc(&s.msg_type)));
        rows.push(Prop::new(
            "Date sent",
            format!("{} &nbsp; {}", esc(&s.date), esc(&s.time)),
        ));
        rows.push(Prop::new("From job", esc(&s.job)).mono());
        rows.push(Prop::new("From program", esc(&s.program)).mono());
    }
    body.push('\n');
    body.push_str(&props_table(
        ctx.product,
        &[PropGroup { title: None, rows }],
        true,
    ));
    let _ = write!(
        body,
        "\n<h2>Message</h2>\n<pre class=\"out\">{}</pre>\n<h2>Cause</h2>\n<p>{}</p>\n<h2>Recovery</h2>\n<p>{}</p>\n",
        esc(&sl.text),
        esc(&sl.cause),
        esc(&sl.recovery)
    );
    if let Some(s) = sent.filter(|s| !s.technical.is_empty()) {
        let _ = writeln!(
            body,
            "<h2>Technical details</h2>\n<pre class=\"out\">{}</pre>",
            esc(&s.technical)
        );
    }
    body.push_str(&form(
        ctx,
        "",
        &buttons(&[
            Button::bare("< Back"),
            Button::bare("Job log"),
            Button::bare("Close"),
        ]),
    ));
    body.push('\n');
    Page {
        title: "Error".into(),
        crumb: vec![crumb.into()],
        h1: "Error".into(),
        desc: Some(
            "The request could not be completed. The message below was returned by the system."
                .into(),
        ),
        messages: vec![Message::new(Kind::Error, &sl.id, &sl.text)],
        body,
        help: "error".into(),
        nav: None,
        generic: true,
        ..Page::default()
    }
}

/// What a refused request wanted.
#[derive(Clone, Debug, Default)]
pub struct Refusal {
    /// The message (`CPF2207`), its text.
    pub message: Message,
    /// The explanation paragraph, HTML.
    pub explain: String,
    /// The function (`End job`).
    pub function: String,
    /// The object.
    pub object: String,
    /// The authority needed (`*JOBCTL`).
    pub required: String,
    /// The user profile.
    pub user: String,
    /// A policy row, HTML, when a product policy refused it.
    pub policy: Option<String>,
    /// The breadcrumb before `Not authorized`.
    pub crumb: Vec<String>,
}

/// The Not authorized page.
pub fn not_authorized(ctx: &Ctx, r: &Refusal) -> Page {
    let mut rows = vec![
        Prop::new("Function", esc(&r.function)),
        Prop::new("Object", esc(&r.object)).mono(),
        Prop::new("Required authority", esc(&r.required)).mono(),
        Prop::new("User profile", esc(&r.user)).mono(),
    ];
    if let Some(p) = &r.policy {
        rows.push(Prop::new("Policy", p.clone()));
    }
    let body = format!(
        "\n<p>{}</p>\n{}\n<p>Contact the security officer (QSECOFR) if you need this authority. The attempt has been recorded in the audit journal (entry type AF).</p>\n{}\n",
        r.explain,
        props_table(ctx.product, &[PropGroup { title: None, rows }], true),
        form(
            ctx,
            "",
            &buttons(&[Button::bare("< Back"), Button::bare("Home")])
        )
    );
    let mut crumb = r.crumb.clone();
    crumb.push("Not authorized".into());
    Page {
        title: "Not authorized".into(),
        crumb,
        h1: "Not authorized".into(),
        messages: vec![r.message.clone()],
        body,
        help: "notauth".into(),
        generic: true,
        ..Page::default()
    }
}

/// A confirmation: a sentence, the table of what is affected, a note
/// or options, OK and Cancel. `options` is HTML inside the form.
pub fn confirm_body(ctx: &Ctx, sentence: &str, table: &str, options: &str) -> String {
    format!(
        "<p>{}</p>{table}{}",
        esc(sentence),
        form(
            ctx,
            "",
            &format!(
                "{options}{}",
                buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
            )
        )
    )
}
