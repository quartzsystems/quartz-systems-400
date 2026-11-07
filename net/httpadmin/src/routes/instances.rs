// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Server instances: the list (`instances.html`) and the view of one
//! instance the other pages share: its registry entry, configuration,
//! ports and job.

use httpd::conf::Conf;
use httpd::instance::{self, Port};
use httpd::registry::{Entry, Registry};
use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Cell, Col, FilterField, List, Paging, actions, filter, list_table, paging, status};

use super::Rq;
use crate::{PRODUCT, seed};
use crate::env::{Env, HttpJob};

/// One instance as the pages see it.
#[derive(Clone, Debug)]
pub struct View {
    /// The registry entry.
    pub entry: Entry,
    /// The parsed configuration, if it reads and parses.
    pub conf: Option<Conf>,
    /// Its ports.
    pub ports: Vec<Port>,
    /// Its job, when running.
    pub job: Option<HttpJob>,
}

impl View {
    /// The name.
    pub fn name(&self) -> &str {
        &self.entry.name
    }

    /// Whether it runs.
    pub fn running(&self) -> bool {
        self.job.is_some()
    }

    /// `80, 443`.
    pub fn port_list(&self) -> String {
        self.ports.iter().map(|p| p.port.to_string()).collect::<Vec<_>>().join(", ")
    }
}

fn view(entry: Entry, env: &Env, jobs: &[HttpJob]) -> View {
    let conf = instance::load_conf(env.files.as_ref(), &entry.root).ok();
    let ports = conf.as_ref().map(instance::ports).unwrap_or_default();
    let job = jobs.iter().find(|j| j.instance.eq_ignore_ascii_case(&entry.name)).cloned();
    View { entry, conf, ports, job }
}

/// Whether an entry is listed: Navigator's and Access for Web's only while
/// their products are installed.
fn listed(env: &Env, e: &Entry) -> bool {
    seed::PRODUCTS
        .iter()
        .find(|(name, _)| e.name.eq_ignore_ascii_case(name))
        .is_none_or(|(_, product)| env.system.product_installed(product))
}

/// Every instance, in registry order.
pub fn views(env: &Env) -> Vec<View> {
    let jobs = env.jobs.active();
    Registry::load(env.files.as_ref())
        .map(|r| {
            r.instances
                .into_iter()
                .filter(|e| listed(env, e))
                .map(|e| view(e, env, &jobs))
                .collect()
        })
        .unwrap_or_default()
}

/// Instance `name`.
pub fn one(env: &Env, name: &str) -> Option<View> {
    let jobs = env.jobs.active();
    let e = Registry::load(env.files.as_ref()).ok()?.get(name)?.clone();
    listed(env, &e).then(|| view(e, env, &jobs))
}

/// The URL of instance `name`'s page `page` (`general`, `ports`, ...).
pub fn url(name: &str, page: &str) -> String {
    PRODUCT.url(&format!("/instance/{name}/{page}"))
}

/// The confirmation page for `op` on instance `name`.
pub fn confirm_url(name: &str, op: &str) -> String {
    PRODUCT.url(&format!("/instance/{name}/confirm?op={op}"))
}

/// Navigator's job properties page of a qualified job.
pub fn job_url(qualified: &str) -> String {
    format!("/qs400/navigator/job/{qualified}/general")
}

/// The status word of an instance.
pub fn status_cell(v: &View) -> String {
    if v.running() {
        status("Running", Some(true))
    } else {
        status("Stopped", Some(false))
    }
}

fn matches_name(name: &str, pat: &str) -> bool {
    let pat = pat.trim().to_ascii_uppercase();
    if pat.is_empty() {
        return true;
    }
    match pat.strip_suffix('*') {
        Some(p) => name.starts_with(p),
        None => name.contains(&pat),
    }
}

/// `GET /instances?status=&name=&sort=&page=&rows=`.
#[allow(clippy::too_many_lines)]
pub fn list(rq: &Rq<'_>) -> Response {
    let q = rq.query();
    let st = q.str("status");
    let st = if matches!(st, "Running" | "Stopped") { st } else { "" };
    let name = q.str("name").trim().to_string();
    let sort = q.str("sort");
    let mut all: Vec<View> = views(rq.env())
        .into_iter()
        .filter(|v| match st {
            "Running" => v.running(),
            "Stopped" => !v.running(),
            _ => true,
        })
        .filter(|v| matches_name(v.name(), &name))
        .collect();
    match sort {
        "name" => all.sort_by(|a, b| a.name().cmp(b.name())),
        "status" => all.sort_by_key(|v| !v.running()),
        "ports" => all.sort_by_key(|v| v.ports.first().map_or(u16::MAX, |p| p.port)),
        "job" => all.sort_by_key(|v| v.job.as_ref().map_or(String::from("~"), |j| j.number.clone())),
        "started" => all.sort_by_key(|v| v.job.as_ref().map_or(u64::MAX, |j| j.started)),
        _ => {}
    }
    let mut keep: Vec<(String, String)> = Vec::new();
    if !st.is_empty() {
        keep.push(("status".into(), st.into()));
    }
    if !name.is_empty() {
        keep.push(("name".into(), name.clone()));
    }
    let list_keep = keep.clone();
    if !sort.is_empty() {
        keep.push(("sort".into(), sort.into()));
    }
    let p = Paging::from_query(q.str("page"), q.str("rows"), all.len(), keep);
    let rows = all[p.range()]
        .iter()
        .map(|v| {
            let n = v.name();
            let acts = if v.running() {
                actions(&[
                    (confirm_url(n, "stop"), "Stop"),
                    (confirm_url(n, "restart"), "Restart"),
                    (url(n, "general"), "Manage"),
                ])
            } else {
                actions(&[
                    (confirm_url(n, "start"), "Start"),
                    (url(n, "general"), "Manage"),
                    (confirm_url(n, "delete"), "Delete"),
                ])
            };
            let (job, started) = match &v.job {
                Some(j) => (
                    format!("<a href=\"{}\">{}</a>", esc(&job_url(&j.qualified())), esc(&j.qualified())),
                    rq.when(j.started),
                ),
                None => (String::new(), String::new()),
            };
            vec![
                Cell::act(acts),
                Cell::mono_html(format!("<a href=\"{}\">{}</a>", esc(&url(n, "general")), esc(n))),
                Cell::html(status_cell(v)),
                Cell::text(&v.port_list()),
                Cell::mono_html(job),
                Cell::text(&started),
            ]
        })
        .collect();
    let table = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Instance name", "name"),
            Col::sorted("Status", "status"),
            Col::sorted("Ports", "ports"),
            Col::sorted("Job", "job"),
            Col::sorted("Started", "started"),
        ],
        rows,
        empty: "No instances match the filter.".into(),
        keep: list_keep,
    };
    let fields = [
        FilterField::Select(
            "Status".into(),
            "status".into(),
            vec!["All".into(), "Running".into(), "Stopped".into()],
            st.into(),
        ),
        FilterField::Text("Name".into(), "name".into(), name, 12),
    ];
    let buttons = qsweb::partials::form(
        &rq.ctx,
        &PRODUCT.url("/create/step/1"),
        &qsweb::partials::buttons(&[
            qsweb::partials::Button::new("create_new_instance", "Create new instance..."),
            qsweb::partials::Button::new("refresh", "Refresh"),
        ]),
    );
    let body = format!("{}{}{}{buttons}", filter(&fields, true), list_table(&table), paging(&p));
    rq.page(Page {
        title: "Server instances".into(),
        crumb: vec!["Manage".into(), "Server instances".into()],
        h1: "Server instances".into(),
        desc: Some(
            "HTTP server instances defined on this system. Each running instance is a job in subsystem QHTTPSVR."
                .into(),
        ),
        body,
        help: "instances".into(),
        nav: Some(("Manage", Some("/instances"))),
        ..Page::default()
    })
}
