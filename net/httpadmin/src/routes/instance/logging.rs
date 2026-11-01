// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/instance/<name>/logging` (`instance-logging.html`): `CustomLog`
//! with its `LogFormat`, `ErrorLog`, `LogLevel`, rotation and keeping
//! (`LogMaint path files size-MB [HH:MM]`), the log CCSID (`LogCCSID`).

use httpd::conf::{Conf, Directive, Node, Scope as _, rules};
use httpd::instance::{self, LogFormat};
use qsweb::http::Response;
use qsweb::msg::Message;
use qsweb::partials::{Button, Prop, PropGroup, checkbox, input, input_cmd, props, radio};

use super::{Rq, bad_value, edit, pick, tab_page};
use crate::routes::instances::View;

/// Log levels, least to most detail.
const LEVELS: &[&str] = &["emerg", "alert", "crit", "error", "warn", "notice", "info", "debug"];

/// Formats: label and the format.
const FORMATS: &[(&str, LogFormat)] = &[
    ("Combined", LogFormat::Combined),
    ("Common", LogFormat::Common),
    ("Extended", LogFormat::Extended),
];

/// The CCSID logs are written in unless `LogCCSID` says otherwise.
const DEFAULT_CCSID: &str = "1208";

/// How logs rotate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rotation {
    /// Daily at `HH:MM`.
    Daily(String),
    /// When the file passes this many MB.
    Size(u32),
    /// Never.
    Never,
}

/// The settings the page shows.
struct Logging {
    access: Option<(String, &'static str)>,
    error: String,
    level: String,
    rotation: Rotation,
    keep: String,
    ccsid: String,
}

/// The `LogMaint` of `path` (as the file writes it) in `c`.
fn maint<'a>(c: &'a Conf, path: &str) -> Option<&'a Directive> {
    c.all("LogMaint").into_iter().find(|d| d.args.first().map(String::as_str) == Some(path))
}

/// How the log `path` (as the file writes it) rotates, and how many
/// rotated files are kept.
pub fn rotation(c: &Conf, path: &str) -> (Rotation, String) {
    match maint(c, path) {
        None => (Rotation::Never, "30".into()),
        Some(d) => {
            let keep = d.args.get(1).cloned().unwrap_or_default();
            match d.args.get(2).and_then(|s| s.parse::<u32>().ok()) {
                Some(mb) if mb > 0 => (Rotation::Size(mb), keep),
                _ => (Rotation::Daily(d.args.get(3).cloned().unwrap_or_else(|| "00:00".into())), keep),
            }
        }
    }
}

fn read(c: &Conf, root: &str) -> Logging {
    let access = c.get("CustomLog").map(|d| {
        let nick = d.args.get(1).map_or("", String::as_str);
        let label = FORMATS.iter().find(|(_, f)| f.nick() == nick).map_or("Combined", |(l, _)| *l);
        (instance::resolve(root, &d.args[0]), label)
    });
    let error = c.value("ErrorLog");
    let error = if error.is_empty() { "logs/error_log".to_string() } else { error };
    let rot_of = c.get("CustomLog").map_or(error.clone(), |d| d.args[0].clone());
    let (rotation, keep) = rotation(c, &rot_of);
    let level = c.value("LogLevel");
    let ccsid = c.value("LogCCSID");
    Logging {
        access,
        error: instance::resolve(root, &error),
        level: if level.is_empty() { "warn".into() } else { level.to_ascii_lowercase() },
        rotation,
        keep,
        ccsid: if ccsid.is_empty() { DEFAULT_CCSID.into() } else { ccsid },
    }
}

/// `GET` shows the settings; `POST` applies them.
pub fn page(rq: &Rq<'_>, v: &View) -> Response {
    if rq.post() {
        return post(rq, v);
    }
    let empty = Conf::default();
    let l = read(v.conf.as_ref().unwrap_or(&empty), &v.entry.root);
    let root = v.entry.root.trim_end_matches('/');
    let (alp, alf) = l.access.clone().unwrap_or_else(|| (format!("{root}/logs/access_log"), "Combined"));
    let labels: Vec<&str> = FORMATS.iter().map(|(l, _)| *l).collect();
    let (at, size) = match &l.rotation {
        Rotation::Daily(t) => (t.clone(), "10".to_string()),
        Rotation::Size(mb) => ("00:00".into(), mb.to_string()),
        Rotation::Never => ("00:00".into(), "10".into()),
    };
    let rotate = format!(
        "{} Daily at {}<br>{} When size exceeds {} MB<br>{} Never",
        radio("rot", "daily", matches!(l.rotation, Rotation::Daily(_))),
        input("at", &at, 5),
        radio("rot", "size", matches!(l.rotation, Rotation::Size(_))),
        input("size", &size, 4),
        radio("rot", "never", l.rotation == Rotation::Never)
    );
    let groups = [
        PropGroup {
            title: Some("Access log".into()),
            rows: vec![
                Prop::new("Access log", format!("{} Enabled", checkbox("al", l.access.is_some()))),
                Prop::new("Path", input_cmd("alp", &alp, 45)),
                Prop::new("Format", pick("alf", &labels, alf)),
            ],
        },
        PropGroup {
            title: Some("Error log".into()),
            rows: vec![
                Prop::new("Path", input_cmd("elp", &l.error, 45)),
                Prop::new("Log level", pick("ll", LEVELS, &l.level)),
            ],
        },
        PropGroup {
            title: Some("Rotation".into()),
            rows: vec![
                Prop::new("Rotate", rotate),
                Prop::new("Keep", format!("{} rotated files", input("keep", &l.keep, 3))),
                Prop::new("Log directory CCSID", input("ccsid", &l.ccsid, 5)),
            ],
        },
    ];
    let body = props(
        &rq.ctx,
        &groups,
        &[Button::new("apply", "Apply"), Button::new("ok", "OK"), Button::new("cancel", "Cancel")],
    );
    rq.page(tab_page(v, "logging", None, &body))
}

fn post(rq: &Rq<'_>, v: &View) -> Response {
    let form = rq.form();
    if !(form.has("apply") || form.has("ok")) {
        return rq.redirect("/instances");
    }
    if let Err(r) = super::may_change(rq, v, "Change HTTP server logging") {
        return r;
    }
    let msg = apply(rq, v).unwrap_or_else(|e| e);
    super::after(rq, v, "logging", msg)
}

/// A path as the file writes it: relative to the server root when
/// under it.
fn rel(root: &str, p: &str) -> String {
    p.strip_prefix(&format!("{}/", root.trim_end_matches('/'))).unwrap_or(p).to_string()
}

fn number(form: &qsweb::http::Form, field: &str, label: &str, max: u32) -> Result<u32, Message> {
    let s = form.str(field).trim();
    s.parse::<u32>()
        .ok()
        .filter(|n| (1..=max).contains(n))
        .ok_or_else(|| bad_value(s, label, &format!("a number from 1 to {max}")))
}

fn apply(rq: &Rq<'_>, v: &View) -> Result<Message, Message> {
    let form = rq.form();
    let root = v.entry.root.clone();
    let absolute = |field: &str, label: &str| -> Result<String, Message> {
        let p = form.str(field).trim().to_string();
        if p.starts_with('/') && !p.contains("..") {
            Ok(rel(&root, &p))
        } else {
            Err(bad_value(&p, label, "an absolute path name starting with /"))
        }
    };
    let access = if form.has("al") {
        let fmt = FORMATS.iter().find(|(l, _)| *l == form.str("alf")).map_or(LogFormat::Combined, |(_, f)| *f);
        Some((absolute("alp", "Access log path")?, fmt))
    } else {
        None
    };
    let error = absolute("elp", "Error log path")?;
    let level = form.str("ll");
    if !LEVELS.contains(&level) {
        return Err(bad_value(level, "Log level", "one of emerg, alert, crit, error, warn, notice, info, debug"));
    }
    let keep = number(&form, "keep", "Keep", 999)?;
    let rotation = match form.str("rot") {
        "daily" => {
            let at = form.str("at").trim();
            if !rules::is_time(at) {
                return Err(bad_value(at, "Daily at", "a time of day as HH:MM"));
            }
            Rotation::Daily(at.to_string())
        }
        "size" => Rotation::Size(number(&form, "size", "When size exceeds", 100_000)?),
        _ => Rotation::Never,
    };
    let ccsid = number(&form, "ccsid", "Log directory CCSID", 65_535)?;
    edit(rq, v, |c| {
        match &access {
            Some((path, fmt)) => {
                if !c.all("LogFormat").iter().any(|d| d.args.get(1).map(String::as_str) == Some(fmt.nick())) {
                    c.add(Directive::new("LogFormat", &[fmt.format(), fmt.nick()]));
                }
                c.set("CustomLog", &[path, fmt.nick()]);
            }
            None => {
                c.unset("CustomLog");
            }
        }
        c.set("ErrorLog", &[&error]);
        c.set("LogLevel", &[level]);
        // New LogMaint lines go where the old ones were.
        let at = c.items.iter().position(|n| matches!(n, Node::Directive(d) if d.is("LogMaint")));
        c.unset("LogMaint");
        let mut lines = Vec::new();
        if rotation != Rotation::Never {
            let keep = keep.to_string();
            let mut paths: Vec<&str> = access.iter().map(|(p, _)| p.as_str()).collect();
            paths.push(&error);
            for p in paths {
                let args: Vec<String> = match &rotation {
                    Rotation::Daily(t) if t == "00:00" => vec![p.into(), keep.clone(), "0".into()],
                    Rotation::Daily(t) => vec![p.into(), keep.clone(), "0".into(), t.clone()],
                    Rotation::Size(mb) => vec![p.into(), keep.clone(), mb.to_string()],
                    Rotation::Never => Vec::new(),
                };
                lines.push(Directive::new("LogMaint", &args.iter().map(String::as_str).collect::<Vec<_>>()));
            }
        }
        match at {
            Some(i) => {
                for (k, d) in lines.into_iter().enumerate() {
                    c.items.insert(i + k, Node::Directive(d));
                }
            }
            None => lines.into_iter().for_each(|d| c.add(d)),
        }
        if ccsid.to_string() == DEFAULT_CCSID && c.get("LogCCSID").is_none() {
            // The default needs no directive.
        } else {
            c.set("LogCCSID", &[&ccsid.to_string()]);
        }
        Ok(())
    })
}
