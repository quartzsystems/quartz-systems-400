// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/instance/<name>/logs?which=access|error&n=200&q=`
//! (`instance-logs.html`): the last lines of the access or error log,
//! optionally only those holding some text, and
//! `/instance/<name>/logs/download?which=` streaming the whole file.

use std::fmt::Write as _;

use httpd::conf::{Conf, Scope as _};
use httpd::instance;
use qsweb::html::{esc, query};
use qsweb::http::Response;
use qsweb::partials::tabs;

use super::logging::{Rotation, rotation};
use super::{Rq, pick, tab_page};
use crate::routes::instances::{self, View};

/// Line counts offered.
const COUNTS: &[&str] = &["50", "100", "200", "500", "1000"];

/// The log `which` names: its path as the file writes it and in full.
fn log_path(v: &View, which: &str) -> Option<(String, String)> {
    let empty = Conf::default();
    let c = v.conf.as_ref().unwrap_or(&empty);
    let p = if which == "error" {
        let e = c.value("ErrorLog");
        if e.is_empty() { "logs/error_log".to_string() } else { e }
    } else {
        c.get("CustomLog")?.args.first()?.clone()
    };
    Some((p.clone(), instance::resolve(&v.entry.root, &p)))
}

fn rotation_words(c: Option<&Conf>, path: &str) -> String {
    let Some(c) = c else { return "not rotated".into() };
    match rotation(c, path).0 {
        Rotation::Daily(t) if t == "00:00" => "rotated daily".into(),
        Rotation::Daily(t) => format!("rotated daily at {t}"),
        Rotation::Size(mb) => format!("rotated when larger than {mb} MB"),
        Rotation::Never => "not rotated".into(),
    }
}

/// `GET /logs`: the tail of a log.
pub fn page(rq: &Rq<'_>, v: &View) -> Response {
    let q = rq.query();
    let which = if q.str("which") == "error" { "error" } else { "access" };
    let n = if COUNTS.contains(&q.str("n")) { q.str("n") } else { "200" };
    let filter = q.str("q").to_string();
    let name = v.name();
    let second = tabs(
        &[
            ("Access log", instances::url(name, "logs")),
            ("Error log", format!("{}{}", instances::url(name, "logs"), query(&[("which", "error")]))),
        ],
        usize::from(which == "error"),
    );
    let hidden = if which == "error" { "<input type=\"hidden\" name=\"which\" value=\"error\">" } else { "" };
    let form = format!(
        "<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr><td>Lines:</td><td>{}</td><td>Contains:</td><td><input type=\"text\" name=\"q\" value=\"{}\" size=\"20\"></td><td>{hidden}<input type=\"submit\" value=\"Refresh\"> </td></tr></table></form>",
        pick("n", COUNTS, n),
        esc(&filter)
    );
    let mut body = format!("{second}{form}");
    match log_path(v, which) {
        None => body.push_str("<p class=\"small grey\">The instance keeps no access log. Turn it on on the Logging page.</p>"),
        Some((as_written, path)) => {
            let text = rq.env().files.read(&path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            let total = text.lines().filter(|l| filter.is_empty() || l.contains(filter.as_str())).count();
            let lines = instance::tail(&text, n.parse().unwrap_or(200), &filter);
            body.push_str("<pre class=\"out\">");
            for l in &lines {
                let _ = writeln!(body, "{}", esc(l));
            }
            let download = format!("{}{}", instances::url(name, "logs/download"), query(&[("which", which)]));
            let _ = write!(
                body,
                "</pre><p class=\"small grey\">Showing last {} of {total} lines of {} ({}). <a href=\"{}\">Download complete file</a></p>",
                lines.len(),
                esc(&path),
                rotation_words(v.conf.as_ref(), &as_written),
                esc(&download)
            );
        }
    }
    rq.page(tab_page(v, "logs", None, &body))
}

/// `GET /logs/download?which=`: the whole log.
pub fn download(rq: &Rq<'_>, v: &View) -> Response {
    let which = if rq.query().str("which") == "error" { "error" } else { "access" };
    let Some((_, path)) = log_path(v, which) else {
        return rq.not_found(&format!("{which} log of instance {}", v.name()));
    };
    let file = format!("{}-{}.txt", v.name(), path.rsplit('/').next().unwrap_or("log"));
    // A log not written yet downloads empty.
    let from = rq
        .env()
        .files
        .open(&path)
        .unwrap_or_else(|_| Box::new(std::io::Cursor::new(Vec::new())));
    Response::stream(&file, "text/plain; charset=utf-8", from)
}
