// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One instance's pages under `/instance/<name>/`: the confirmations
//! and the tabs (General, Ports & addresses, Directories & aliases,
//! TLS, Logging, Configuration file, Logs). Shared here: the tab strip,
//! the page frame, and changing the configuration file safely.

pub mod confirm;
pub mod general;

use std::fmt::Write as _;

use httpd::conf::{self, Conf};
use httpd::instance;
use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};

use super::Rq;
use super::instances::{self, View};
use crate::env::Need;
use crate::msgs;

/// The tabs: label and page.
pub const TABS: &[(&str, &str)] = &[
    ("General", "general"),
    ("Ports & addresses", "ports"),
    ("Directories & aliases", "directories"),
    ("TLS", "tls"),
    ("Logging", "logging"),
    ("Configuration file", "config"),
    ("Logs", "logs"),
];

/// `/instance/<name>/<rest>`.
pub fn route(rq: &Rq<'_>, name: &str, rest: &[&str]) -> Response {
    let Some(v) = instances::one(rq.env(), name) else {
        return rq.not_found(&format!("HTTP server instance {name}"));
    };
    match rest {
        ["confirm"] => confirm::page(rq, &v),
        ["general"] => general::page(rq, &v),
        _ => rq.not_found(&format!("page {} of instance {}", rest.join("/"), v.name())),
    }
}

/// The tab strip with tab `current` (its page name) bold.
pub fn tab_strip(name: &str, current: &str) -> String {
    let tabs: Vec<(&str, String)> = TABS.iter().map(|(l, p)| (*l, instances::url(name, p))).collect();
    let at = TABS.iter().position(|(_, p)| *p == current).unwrap_or(0);
    qsweb::partials::tabs(&tabs, at)
}

/// A tab's page: `Manage instance: NAME`, the tab strip, then `body`.
pub fn tab_page(v: &View, tab: &str, desc: Option<&str>, body: &str) -> Page {
    let label = TABS.iter().find(|(_, p)| *p == tab).map_or("", |(l, _)| *l);
    let mut crumb = vec!["Manage".to_string(), "Server instances".to_string(), v.name().to_string()];
    if tab != "general" {
        crumb.push(label.to_string());
    }
    Page {
        title: format!("Manage instance: {}", v.name()),
        crumb,
        h1: format!("Manage instance: {}", v.name()),
        desc: desc.map(ToString::to_string),
        body: format!("{}{body}", tab_strip(v.name(), tab)),
        help: format!("instance-{tab}"),
        ..Page::default()
    }
}

/// A select with `chosen` marked; the first option, which a browser
/// shows anyway, is never marked.
pub fn pick(name: &str, options: &[&str], chosen: &str) -> String {
    let mut s = format!("<select name=\"{}\">", esc(name));
    for (i, o) in options.iter().enumerate() {
        let sel = if i > 0 && *o == chosen { " selected" } else { "" };
        let _ = write!(s, "<option{sel}>{}</option>", esc(o));
    }
    s.push_str("</select>");
    s
}

/// Check `*IOSYSCFG` for changing instance `v`.
pub fn may_change(rq: &Rq<'_>, v: &View, function: &str) -> Result<(), Response> {
    rq.require(Need::IoSysCfg, function, v.name(), &["Manage", "Server instances", v.name()])
}

/// Change instance `v`'s configuration with `f` and write it safely
/// (one `.bak`, temporary file and rename; restart required when it
/// runs). Gives the completion message, or the error to show.
pub fn edit(rq: &Rq<'_>, v: &View, f: impl FnOnce(&mut Conf) -> Result<(), Message>) -> Result<Message, Message> {
    let files = rq.env().files.as_ref();
    let text = instance::read_conf(files, &v.entry.root).map_err(|e| io_error(v, &e.to_string()))?;
    let mut c = conf::parse(&text).map_err(|e| conf_error(v, &e))?;
    f(&mut c)?;
    let new = c.print();
    if new == text {
        return Ok(changed(v));
    }
    instance::apply_conf(files, v.name(), &v.entry.root, &new, v.running()).map_err(|e| match e {
        httpd::Error::Conf(ce) => conf_error(v, &ce),
        e => io_error(v, &e.to_string()),
    })?;
    rq.audit_change(&instance::conf_path(&v.entry.root), "configuration changed");
    Ok(changed(v))
}

/// `HTP8022`: the configuration changed.
pub fn changed(v: &View) -> Message {
    msgs::message(Kind::Done, "HTP8022", &[v.name()])
}

/// `HTP8100` for a configuration error.
pub fn conf_error(v: &View, e: &conf::ConfError) -> Message {
    msgs::message(
        Kind::Error,
        "HTP8100",
        &[&e.line.to_string(), &e.col.to_string(), &e.text, &instance::conf_path(&v.entry.root)],
    )
}

/// `HTP8106` for a file that could not be read or written.
pub fn io_error(v: &View, what: &str) -> Message {
    msgs::message(Kind::Error, "HTP8106", &[v.name(), what])
}

/// `HTP8120`: a field's value is not valid.
pub fn bad_value(value: &str, field: &str, rule: &str) -> Message {
    msgs::message(Kind::Error, "HTP8120", &[value, field, rule])
}

/// The page to return to after a form: the same tab with the message,
/// or the instance list on OK.
pub fn after(rq: &Rq<'_>, v: &View, tab: &str, m: Message) -> Response {
    let ok = rq.form().has("ok") && m.kind != Kind::Error;
    let route = if ok { "/instances".to_string() } else { format!("/instance/{}/{tab}", v.name()) };
    rq.done(&route, m)
}
