// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/instance/<name>/config` (`instance-config.html`): the
//! configuration file itself. Validate parses the text without writing
//! it; Apply writes it through a temporary file and a rename, keeping one
//! `.bak`; Reset goes back to the file. Text being worked on waits in the
//! session between the POST and the page.

use httpd::conf;
use httpd::instance;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{Button, buttons, form};

use super::{Rq, conf_error, io_error, tab_page};
use crate::msgs;
use crate::routes::instances::View;

/// The session key of `name`'s text being edited (ending in `.` so
/// clearing it leaves other instances' alone).
fn key(name: &str) -> String {
    format!("conf.{name}.")
}

/// `GET` shows the text and whether it parses; `POST` validates,
/// applies or resets.
pub fn page(rq: &Rq<'_>, v: &View) -> Response {
    if rq.post() {
        return post(rq, v);
    }
    let path = instance::conf_path(&v.entry.root);
    let pending = rq.get(&key(v.name()));
    let text = if pending.is_empty() {
        match instance::read_conf(rq.env().files.as_ref(), &v.entry.root) {
            Ok(t) => t,
            Err(e) => {
                return rq.page(tab_page(
                    v,
                    "config",
                    Some(&path),
                    &format!("<p>{}</p>", esc(&e.to_string())),
                ));
            }
        }
    } else {
        pending
    };
    let (date, time) = rq.stamp(rq.env().system.now());
    let state = match conf::parse(&text) {
        Ok(c) => format!(
            "<span class=\"ok\">Syntax OK</span> &mdash; {} lines, checked {date} {time}.",
            c.line_count()
        ),
        Err(e) => format!(
            "<span class=\"err\">Error at line {} column {}</span> &mdash; {} Checked {date} {time}.",
            e.line,
            e.col,
            esc(&e.text)
        ),
    };
    let status = format!(
        "<div class=\"small\"><b>Validate:</b> {state} &nbsp; <span class=\"grey\">Editing the file directly bypasses the forms; the forms re-read it on the next page load.</span></div>"
    );
    // A browser drops one newline straight after <textarea>.
    let lead = if text.starts_with('\n') { "\n" } else { "" };
    let area = format!(
        "<textarea name=\"conf\" class=\"conf\" rows=\"40\" wrap=\"off\">{lead}{}</textarea>{}",
        esc(&text),
        buttons(&[
            Button::new("validate", "Validate"),
            Button::new("apply", "Apply"),
            Button::new("reset", "Reset")
        ])
    );
    let body = format!("{status}{}", form(&rq.ctx, "", &area));
    rq.page(tab_page(v, "config", Some(&path), &body))
}

fn post(rq: &Rq<'_>, v: &View) -> Response {
    let f = rq.form();
    let here = format!("/instance/{}/config", v.name());
    if !(f.has("validate") || f.has("apply")) {
        rq.clear(&key(v.name()));
        return rq.redirect(&here);
    }
    // Browsers send the text with CR LF line ends.
    let text = f.str("conf").replace("\r\n", "\n");
    if f.has("validate") {
        rq.set(&[(&key(v.name()), &text)]);
        let m = match conf::parse(&text) {
            Ok(_) => msgs::message(
                Kind::Info,
                "HTP8101",
                &[&instance::conf_path(&v.entry.root)],
            ),
            Err(e) => conf_error(v, &e),
        };
        return rq.done(&here, m);
    }
    if let Err(r) = super::may_change(rq, v, "Change HTTP server configuration file") {
        return r;
    }
    if let Err(e) = conf::parse(&text) {
        rq.set(&[(&key(v.name()), &text)]);
        return rq.done(&here, conf_error(v, &e));
    }
    let files = rq.env().files.as_ref();
    let m = match instance::apply_conf(files, v.name(), &v.entry.root, &text, v.running()) {
        Ok(()) => {
            rq.clear(&key(v.name()));
            rq.audit_change(
                &instance::conf_path(&v.entry.root),
                "configuration file replaced",
            );
            super::changed(v)
        }
        Err(httpd::Error::Conf(e)) => conf_error(v, &e),
        Err(e) => {
            rq.set(&[(&key(v.name()), &text)]);
            io_error(v, &e.to_string())
        }
    };
    rq.done(&here, m)
}
