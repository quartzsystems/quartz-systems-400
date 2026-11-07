// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/instance/<name>/general` (`instance-general.html`): the
//! description, document root, server user profile and threads, the
//! status with Start or Stop, autostart and the hot backup partner.

use httpd::conf::{Conf, Node, Scope as _};
use httpd::registry::Registry;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Button, Prop, PropGroup, checkbox, input, input_cmd, props};

use super::{Rq, bad_value, edit, pick, tab_page};
use crate::routes::create::low_port;
use crate::routes::instances::{self, View, job_url, status_cell, views};
use crate::settings::Settings;

/// The profile that owns the files under `/www` (the `ADMIN` job's);
/// any other server user profile is given authority to its instance.
pub const SERVER_OWNER: &str = "QTMHHTTP";
use crate::msgs;

/// Fewest server threads.
const MIN_THREADS: u32 = 10;

/// The instances that may be `v`'s hot backup: the partner already set,
/// then the stopped instances other than itself.
fn partners(rq: &Rq<'_>, v: &View) -> Vec<String> {
    let mut out: Vec<String> = v.entry.hot_backup.iter().cloned().collect();
    for o in views(rq.env()) {
        if !o.running() && o.name() != v.name() && !out.iter().any(|n| n == o.name()) {
            out.push(o.name().to_string());
        }
    }
    out
}

fn conf_value(v: &View, name: &str) -> String {
    v.conf.as_ref().map(|c| c.value(name)).unwrap_or_default()
}

/// `GET` shows the properties; `POST` applies them or goes to a
/// confirmation.
pub fn page(rq: &Rq<'_>, v: &View) -> Response {
    if rq.post() {
        return post(rq, v);
    }
    let s = Settings::load(rq.env().files.as_ref());
    let n = v.name();
    let status = if v.running() {
        format!(
            "{} &nbsp; <input type=\"submit\" name=\"stop\" value=\"Stop\"> <input type=\"submit\" name=\"restart\" value=\"Restart\">",
            status_cell(v)
        )
    } else {
        format!(
            "{} &nbsp; <input type=\"submit\" name=\"start\" value=\"Start\">",
            status_cell(v)
        )
    };
    let job = match &v.job {
        Some(j) => format!(
            "<a href=\"{}\">{}</a>",
            esc(&job_url(&j.qualified())),
            esc(&j.qualified())
        ),
        None => "<span class=\"grey\">Not active</span>".into(),
    };
    let started = v.job.as_ref().map(|j| j.started).or(v.entry.last_started);
    let started = started.map_or_else(|| "Never".into(), |t| rq.when(t));
    let ended = match v.entry.last_ended {
        Some(t) if v.entry.ended_how.is_empty() => rq.when(t),
        Some(t) => format!("{} ({})", rq.when(t), v.entry.ended_how),
        None => "Never".into(),
    };
    let partners = partners(rq, v);
    let partners: Vec<&str> = partners.iter().map(String::as_str).collect();
    let hot = format!(
        "{} {}",
        checkbox("hotbackup", v.entry.hot_backup_active),
        pick(
            "hot",
            &partners,
            v.entry.hot_backup.as_deref().unwrap_or("")
        )
    );
    let groups = [
        PropGroup {
            title: Some("Identification".into()),
            rows: vec![
                Prop::new(
                    "Instance name",
                    format!("<span class=\"mono\">{}</span>", esc(n)),
                ),
                Prop::new("Description", input("desc", &v.entry.description, 50)),
                Prop::new(
                    "Server root",
                    format!("<span class=\"mono\">{}</span>", esc(&v.entry.root)),
                ),
                Prop::new(
                    "Document root",
                    input_cmd("docroot", &conf_value(v, "DocumentRoot"), 45),
                ),
                Prop::new(
                    "Server user profile",
                    input("usrprf", &conf_value(v, "ServerUserID"), 10),
                )
                .help("instance-usrprf"),
            ],
        },
        PropGroup {
            title: Some("Status".into()),
            rows: vec![
                Prop::new("Status", status),
                Prop::new("Job information", job),
                Prop::new("Last started", esc(&started)),
                Prop::new("Last ended", esc(&ended)),
            ],
        },
        PropGroup {
            title: Some("Startup".into()),
            rows: vec![
                Prop::new(
                    "Start automatically with STRTCP",
                    checkbox("autostart", v.entry.autostart),
                ),
                Prop::new("Hot backup instance", hot).help("instance-hotbackup"),
                Prop::new(
                    "Number of server threads",
                    format!(
                        "{} ({MIN_THREADS}-{})",
                        input("threads", &conf_value(v, "ThreadsPerChild"), 4),
                        s.max_threads
                    ),
                ),
            ],
        },
    ];
    let body = props(
        &rq.ctx,
        &groups,
        &[
            Button::new("apply", "Apply"),
            Button::new("ok", "OK"),
            Button::new("cancel", "Cancel"),
        ],
    );
    rq.page(tab_page(
        v,
        "general",
        Some("General properties of the instance. Changes take effect the next time the instance is started."),
        &body,
    ))
}

/// Rename the `<Directory>` of the old document root with it.
fn move_docroot(c: &mut Conf, old: &str, new: &str) {
    for n in &mut c.items {
        if let Node::Section(s) = n
            && s.name.eq_ignore_ascii_case("Directory")
            && s.args
                .first()
                .is_some_and(|a| a.trim_end_matches('/') == old.trim_end_matches('/'))
        {
            s.args = vec![new.to_string()];
            s.raw = None;
        }
    }
}

fn post(rq: &Rq<'_>, v: &View) -> Response {
    let f = rq.form();
    for op in ["start", "stop", "restart"] {
        if f.has(op) {
            return Response::redirect(&instances::confirm_url(v.name(), op));
        }
    }
    if !(f.has("apply") || f.has("ok")) {
        return rq.redirect("/instances");
    }
    if let Err(r) = super::may_change(rq, v, "Change HTTP server instance") {
        return r;
    }
    let m = apply(rq, v).unwrap_or_else(|e| e);
    super::after(rq, v, "general", m)
}

fn apply(rq: &Rq<'_>, v: &View) -> Result<Message, Message> {
    let form = rq.form();
    let settings = Settings::load(rq.env().files.as_ref());
    let sys = &rq.env().system;
    let user = form.str("usrprf").trim().to_ascii_uppercase();
    if user.is_empty() || !sys.profile_exists(&user) {
        return Err(msgs::message(Kind::Error, "HTP8201", &[&user]));
    }
    for p in &v.ports {
        low_port(rq, p.port, &user)?;
    }
    let docroot = form.str("docroot").trim().trim_end_matches('/').to_string();
    if !docroot.starts_with('/') {
        return Err(bad_value(
            &docroot,
            "Document root",
            "an absolute path name starting with /",
        ));
    }
    let threads = form.str("threads").trim();
    let range = format!("a number from {MIN_THREADS} to {}", settings.max_threads);
    let count = threads
        .parse::<u32>()
        .ok()
        .filter(|n| (MIN_THREADS..=settings.max_threads).contains(n))
        .ok_or_else(|| bad_value(threads, "Number of server threads", &range))?;
    let hot = form.str("hot").trim().to_ascii_uppercase();
    if form.has("hotbackup") && (hot.is_empty() || hot == v.name()) {
        return Err(bad_value(&hot, "Hot backup instance", "another instance"));
    }
    let old_doc = conf_value(v, "DocumentRoot");
    let msg = edit(rq, v, |c| {
        if old_doc != docroot {
            move_docroot(c, &old_doc, &docroot);
            c.set("DocumentRoot", &[&docroot]);
        }
        if c.value("ServerUserID") != user {
            c.set("ServerUserID", &[&user]);
        }
        if c.value("ThreadsPerChild") != count.to_string() {
            c.set("ThreadsPerChild", &[&count.to_string()]);
        }
        Ok(())
    })?;
    if user != SERVER_OWNER {
        rq.env().jobs.authorize(&v.entry.root, &user)?;
    }
    let desc = form.str("desc").trim().to_string();
    let auto = form.has("autostart");
    let active = form.has("hotbackup");
    Registry::update(rq.env().files.as_ref(), v.name(), |e| {
        e.description = desc;
        e.autostart = auto;
        e.hot_backup_active = active;
        if !hot.is_empty() {
            e.hot_backup = Some(hot);
        }
    })
    .map_err(|e| super::io_error(v, &e.to_string()))?;
    Ok(msg)
}
