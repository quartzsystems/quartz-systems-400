// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Create HTTP server wizard (`create-1.html` to `create-6.html`):
//! `/create/step/1..6`, the answers kept in the session under
//! `create.`, each step checked on Next, the instance made on Create.

use std::fmt::Write as _;

use httpd::conf::{self, Scope as _};
use httpd::instance::{self, LogFormat, Spec};
use httpd::registry::Registry;
use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{
    Button, Prop, PropGroup, checkbox, input, input_cmd, props_table, radio, wizard,
};

use super::Rq;
use super::instance::{bad_value, pick};
use crate::env::Need;
use crate::settings::Settings;
use crate::{PRODUCT, msgs};

/// The steps.
const STEPS: &[&str] = &[
    "Name and description",
    "Server root",
    "Document root",
    "IP address and port",
    "Access log",
    "Create",
];

/// The instances of the QS/400 web products, not offered as models.
const PRODUCT_INSTANCES: &[&str] = &["ADMIN", "NAVIGATOR", "ACCESSWEB"];

/// Rotation choices and the `LogMaint` they make.
const ROTATIONS: &[&str] = &["Daily", "When size exceeds 10 MB", "Never"];

/// Log formats.
const FORMATS: &[&str] = &["Combined", "Common", "Extended"];

/// The answers so far, defaults filled in.
struct Answers {
    name: String,
    desc: String,
    copy: bool,
    copyfrom: String,
    root: String,
    docroot: String,
    sample: bool,
    ip: Option<String>,
    port: String,
    log: bool,
    logpath: String,
    fmt: String,
    rot: String,
}

fn default_root(s: &Settings, name: &str) -> String {
    format!(
        "{}/{}",
        s.default_root.trim_end_matches('/'),
        name.to_ascii_lowercase()
    )
}

impl Answers {
    fn load(rq: &Rq<'_>, s: &Settings) -> Self {
        let g = |k: &str| rq.get(&format!("create.{k}"));
        let name = g("name");
        let root = Some(g("root"))
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| default_root(s, &name));
        let docroot = Some(g("docroot"))
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| format!("{root}/htdocs"));
        let logpath = Some(g("logpath"))
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| format!("{root}/logs/access_log"));
        Self {
            desc: g("desc"),
            copy: g("base") == "copy",
            copyfrom: g("copyfrom"),
            root,
            docroot,
            sample: g("sample") != "off",
            ip: (g("addr") == "ip").then(|| g("ip")),
            port: Some(g("port"))
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| "80".into()),
            log: g("log") != "no",
            logpath,
            fmt: Some(g("fmt"))
                .filter(|f| FORMATS.contains(&f.as_str()))
                .unwrap_or_else(|| FORMATS[0].into()),
            rot: Some(g("rot"))
                .filter(|f| ROTATIONS.contains(&f.as_str()))
                .unwrap_or_else(|| ROTATIONS[0].into()),
            name,
        }
    }

    fn format(&self) -> LogFormat {
        match self.fmt.as_str() {
            "Common" => LogFormat::Common,
            "Extended" => LogFormat::Extended,
            _ => LogFormat::Combined,
        }
    }
}

/// Instances a new one may copy.
fn models(rq: &Rq<'_>) -> Vec<String> {
    Registry::load(rq.env().files.as_ref())
        .map(|r| {
            r.instances
                .into_iter()
                .map(|e| e.name)
                .filter(|n| !PRODUCT_INSTANCES.contains(&n.as_str()))
                .collect()
        })
        .unwrap_or_default()
}

/// Set by a wizard button for the step it redirects to; step 1 without
/// it starts afresh.
const WITHIN: &str = "create.within";

/// `/create/step/<n>`.
pub fn step(rq: &Rq<'_>, n: &str) -> Response {
    let Some(n) = n
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=STEPS.len()).contains(n))
    else {
        return rq.not_found(&format!("wizard step {n}"));
    };
    if rq.post() {
        return post(rq, n);
    }
    // Step 1 reached other than from the wizard's own buttons (the
    // navigation item, a bookmark) starts the wizard anew.
    let within = !rq.get(WITHIN).is_empty();
    rq.clear(WITHIN);
    if n == 1 && !within {
        rq.clear("create.");
    }
    let reached = rq
        .get("create.reached")
        .parse::<usize>()
        .unwrap_or(1)
        .max(1);
    if n > reached {
        return rq.redirect(&format!("/create/step/{reached}"));
    }
    let s = Settings::load(rq.env().files.as_ref());
    let a = Answers::load(rq, &s);
    let body = match n {
        1 => step1(rq, &a, &s),
        2 => step2(&a),
        3 => step3(&a),
        4 => step4(rq, &a),
        5 => step5(&a),
        _ => step6(&a, &s),
    };
    let bs = match n {
        1 => vec![
            Button::new("back", "< Back").disabled(),
            Button::new("next", "Next >"),
            Button::new("cancel", "Cancel"),
        ],
        6 => vec![
            Button::new("back", "< Back"),
            Button::new("cancel", "Cancel"),
            Button::new("finish", "Create"),
        ],
        _ => vec![
            Button::new("back", "< Back"),
            Button::new("next", "Next >"),
            Button::new("cancel", "Cancel"),
        ],
    };
    rq.page(Page {
        title: "Create HTTP server".into(),
        crumb: vec!["Create".into(), "New HTTP server".into()],
        h1: "Create HTTP server".into(),
        desc: Some(format!("Step {n} of {}.", STEPS.len())),
        body: wizard(&rq.ctx, STEPS, n, &body, &bs),
        help: format!("create-{n}"),
        // The navigation item is step 1; later steps leave the tree shut.
        nav: (n == 1).then_some(("Create", Some("/create/step/1"))),
        ..Page::default()
    })
}

fn heading(title: &str, text: &str) -> String {
    format!(
        "<h2>{}</h2><p class=\"small grey\">{}</p>",
        esc(title),
        esc(text)
    )
}

fn table(rows: Vec<Prop>) -> String {
    props_table(&PRODUCT, &[PropGroup { title: None, rows }], false)
}

fn step1(rq: &Rq<'_>, a: &Answers, s: &Settings) -> String {
    let models = models(rq);
    let models: Vec<&str> = models.iter().map(String::as_str).collect();
    let base = format!(
        "{} New configuration<br>{} Copy of existing instance: {}",
        radio("base", "new", !a.copy),
        radio("base", "copy", a.copy),
        pick("copyfrom", &models, &a.copyfrom)
    );
    let text = format!(
        "The instance name becomes the job name in subsystem QHTTPSVR and the directory name under {}. 1 to 10 characters, letters and digits, starting with a letter.",
        s.default_root.trim_end_matches('/')
    );
    format!(
        "{}{}",
        heading("Name and description", &text),
        table(vec![
            Prop::new("Instance name", input("name", &a.name, 10))
                .required()
                .help("create-name"),
            Prop::new("Description", input("desc", &a.desc, 50)),
            Prop::new("Based on", base),
        ])
    )
}

fn step2(a: &Answers) -> String {
    let r = a.root.trim_end_matches('/');
    format!(
        "{}{}",
        heading(
            "Server root",
            "The directory that holds the configuration file, logs and content for this instance. It is created if it does not exist."
        ),
        table(vec![
            Prop::new("Server root", input_cmd("root", &a.root, 45)).help("create-root"),
            Prop::new("Configuration file", esc(&instance::conf_path(r))).mono(),
            Prop::new("Log directory", esc(&format!("{r}/logs"))).mono(),
        ])
    )
}

fn step3(a: &Answers) -> String {
    format!(
        "{}{}",
        heading(
            "Document root",
            "The directory from which the server serves files. Requests for / map to this directory."
        ),
        table(vec![
            Prop::new(
                "Document root",
                format!(
                    "{} <input type=\"submit\" value=\"Browse...\">",
                    input_cmd("docroot", &a.docroot, 45)
                )
            )
            .help("create-docroot"),
            Prop::new("Create a sample index.html", checkbox("sample", a.sample)),
        ])
    )
}

fn step4(rq: &Rq<'_>, a: &Answers) -> String {
    let ifs = rq.env().system.interfaces();
    let ifs: Vec<&str> = ifs.iter().map(String::as_str).collect();
    let addr = format!(
        "{} *ALL (all interfaces)<br>{} Specific address: {}",
        radio("addr", "*ALL", a.ip.is_none()),
        radio("addr", "ip", a.ip.is_some()),
        pick("ip", &ifs, a.ip.as_deref().unwrap_or(""))
    );
    format!(
        "{}{}",
        heading(
            "IP address and port",
            "The address and port the instance listens on. *ALL listens on every interface. Ports below 1024 require the server user profile to have *IOSYSCFG special authority."
        ),
        table(vec![
            Prop::new("IP address", addr).help("create-addr"),
            Prop::new("Port", input("port", &a.port, 6)).required(),
        ])
    )
}

fn step5(a: &Answers) -> String {
    let log = format!(
        "{} Yes, log requests to: {}<br>{} No",
        radio("log", "yes", a.log),
        input_cmd("logpath", &a.logpath, 40),
        radio("log", "no", !a.log)
    );
    format!(
        "{}{}",
        heading(
            "Access log",
            "Record each request in an access log. The log can be viewed on the Logs page of the instance."
        ),
        table(vec![
            Prop::new("Access log", log).help("create-log"),
            Prop::new("Log format", pick("fmt", FORMATS, &a.fmt)),
            Prop::new("Rotate", pick("rot", ROTATIONS, &a.rot)),
        ])
    )
}

fn rotation_word(rot: &str) -> &'static str {
    match rot {
        "Never" => "not rotated",
        "Daily" => "daily",
        _ => "when size exceeds 10 MB",
    }
}

fn step6(a: &Answers, s: &Settings) -> String {
    let yes = |b: bool| if b { "Yes" } else { "No" };
    let rows = [
        ("Instance name", a.name.clone()),
        ("Description", a.desc.clone()),
        (
            "Based on",
            if a.copy {
                format!("Copy of {}", a.copyfrom)
            } else {
                "New configuration".into()
            },
        ),
        ("Server root", a.root.clone()),
        ("Configuration file", instance::conf_path(&a.root)),
        ("Document root", a.docroot.clone()),
        ("Sample index.html", yes(a.sample).into()),
        ("IP address", a.ip.clone().unwrap_or_else(|| "*ALL".into())),
        ("Port", a.port.clone()),
        (
            "Access log",
            if a.log {
                format!("{} ({}, {})", a.logpath, a.fmt, rotation_word(&a.rot))
            } else {
                "None".into()
            },
        ),
        ("Server user profile", s.default_user.clone()),
        ("Start with STRTCP", "No".into()),
    ];
    let mut t = String::from(
        "<table class=\"list\" style=\"width:auto\"><tr><th>Setting</th><th>Value</th></tr>",
    );
    for (i, (k, v)) in rows.iter().enumerate() {
        let tr = if i % 2 == 1 {
            "<tr class=\"alt\">"
        } else {
            "<tr>"
        };
        let _ = write!(
            t,
            "{tr}<td>{}</td><td class=\"mono\">{}</td></tr>",
            esc(k),
            esc(v)
        );
    }
    t.push_str("</table>");
    format!(
        "{}{t}",
        heading(
            "Create",
            "Review the choices below. Press Create to create the instance. The instance is created stopped; start it from the Server instances page."
        )
    )
}

/// The fields each step posts.
const FIELDS: &[&[&str]] = &[
    &["name", "desc", "base", "copyfrom"],
    &["root"],
    &["docroot"],
    &["addr", "ip", "port"],
    &["log", "logpath", "fmt", "rot"],
    &[],
];

fn post(rq: &Rq<'_>, n: usize) -> Response {
    let f = rq.form();
    if f.has("cancel") {
        rq.clear("create.");
        return rq.redirect("/instances");
    }
    if f.has("refresh") {
        return rq.redirect("/instances");
    }
    if f.has("create_new_instance") {
        rq.clear("create.");
        return rq.redirect("/create/step/1");
    }
    // The page this answers is the wizard's own.
    rq.set(&[(WITHIN, "1")]);
    let s = Settings::load(rq.env().files.as_ref());
    if n == 1 && f.has("name") {
        // A new name moves the paths that still follow the old one.
        let old = Answers::load(rq, &s);
        let def = default_root(&s, &old.name);
        if old.root == def {
            rq.clear("create.root");
            if old.docroot == format!("{def}/htdocs") {
                rq.clear("create.docroot");
            }
            if old.logpath == format!("{def}/logs/access_log") {
                rq.clear("create.logpath");
            }
        }
    }
    let mut pairs: Vec<(String, String)> = FIELDS[n - 1]
        .iter()
        .filter(|k| f.has(k))
        .map(|k| (format!("create.{k}"), f.str(k).trim().to_string()))
        .collect();
    if n == 3 && f.has("docroot") {
        pairs.push((
            "create.sample".into(),
            if f.has("sample") { "on" } else { "off" }.into(),
        ));
    }
    if n == 1 && f.has("name") {
        pairs.retain(|(k, _)| k != "create.name");
        pairs.push((
            "create.name".into(),
            f.str("name").trim().to_ascii_uppercase(),
        ));
    }
    let refs: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    rq.set(&refs);
    if f.has("back") {
        return rq.redirect(&format!("/create/step/{}", n.saturating_sub(1).max(1)));
    }
    let a = Answers::load(rq, &s);
    if f.has("next") {
        if let Err(m) = check(rq, &s, &a, n) {
            return rq.done(&format!("/create/step/{n}"), m);
        }
        let reached = rq
            .get("create.reached")
            .parse::<usize>()
            .unwrap_or(1)
            .max(n + 1);
        rq.set(&[("create.reached", &reached.to_string())]);
        return rq.redirect(&format!("/create/step/{}", n + 1));
    }
    if f.has("finish") && n == STEPS.len() {
        return finish(rq, &s, &a);
    }
    rq.redirect(&format!("/create/step/{n}"))
}

fn absolute(v: &str, field: &str) -> Result<(), Message> {
    if v.starts_with('/') && !v.contains("..") {
        Ok(())
    } else {
        Err(bad_value(v, field, "an absolute path name starting with /"))
    }
}

/// Check step `n`'s answers.
fn check(rq: &Rq<'_>, s: &Settings, a: &Answers, n: usize) -> Result<(), Message> {
    let files = rq.env().files.as_ref();
    match n {
        1 => {
            let name = instance::check_name(&a.name).map_err(|_| {
                bad_value(
                    &a.name,
                    "Instance name",
                    "1 to 10 letters and digits, starting with a letter",
                )
            })?;
            // The web products' names stay theirs, installed or not.
            if PRODUCT_INSTANCES.contains(&name.as_str()) {
                return Err(bad_value(
                    &a.name,
                    "Instance name",
                    "a name other than ADMIN, NAVIGATOR and ACCESSWEB",
                ));
            }
            let reg = Registry::load(files).unwrap_or_default();
            if reg.get(&name).is_some() {
                return Err(msgs::message(Kind::Error, "HTP8122", &[&name]));
            }
            if reg.instances.len() >= s.max_instances as usize {
                return Err(msgs::message(
                    Kind::Error,
                    "HTP8123",
                    &[&reg.instances.len().to_string()],
                ));
            }
            if a.copy && reg.get(&a.copyfrom).is_none() {
                return Err(bad_value(
                    &a.copyfrom,
                    "Copy of existing instance",
                    "an instance on this system",
                ));
            }
        }
        2 => {
            absolute(&a.root, "Server root")?;
            let taken = Registry::load(files)
                .unwrap_or_default()
                .instances
                .iter()
                .any(|e| e.root.trim_end_matches('/') == a.root.trim_end_matches('/'));
            if taken || files.exists(&instance::conf_path(&a.root)) {
                return Err(bad_value(
                    &a.root,
                    "Server root",
                    "a directory no other instance uses",
                ));
            }
        }
        3 => absolute(&a.docroot, "Document root")?,
        4 => {
            if let Some(ip) = &a.ip
                && !rq.env().system.interfaces().contains(ip)
            {
                return Err(bad_value(ip, "IP address", "an address of this system"));
            }
            let port = a
                .port
                .parse::<u16>()
                .ok()
                .filter(|p| *p > 0)
                .ok_or_else(|| bad_value(&a.port, "Port", "a number from 1 to 65535"))?;
            low_port(rq, port, &s.default_user)?;
        }
        5 => {
            if a.log {
                absolute(&a.logpath, "Access log")?;
            }
        }
        _ => {
            for i in 1..STEPS.len() {
                check(rq, s, a, i)?;
            }
        }
    }
    Ok(())
}

/// Ports below 1024 need `*IOSYSCFG` on the server user profile.
pub fn low_port(rq: &Rq<'_>, port: u16, user: &str) -> Result<(), Message> {
    let sys = &rq.env().system;
    if port < 1024
        && !sys
            .special(user)
            .iter()
            .any(|a| a == "*IOSYSCFG" || a == "*ALLOBJ")
    {
        return Err(msgs::message(
            Kind::Error,
            "HTP8202",
            &[&port.to_string(), user],
        ));
    }
    Ok(())
}

fn finish(rq: &Rq<'_>, s: &Settings, a: &Answers) -> Response {
    if let Err(r) = rq.require(
        Need::IoSysCfg,
        "Create HTTP server instance",
        &a.name,
        &["Create", "New HTTP server"],
    ) {
        return r;
    }
    if let Err(m) = check(rq, s, a, STEPS.len()) {
        return rq.done("/create/step/6", m);
    }
    let sys = &rq.env().system;
    // From here an error ends the wizard: the next one starts blank.
    let failed = |m: Message| {
        rq.clear("create.");
        rq.done("/instances", m)
    };
    if !sys.profile_exists(&s.default_user) {
        return failed(msgs::message(Kind::Error, "HTP8201", &[&s.default_user]));
    }
    let port: u16 = a.port.parse().unwrap_or(80);
    let spec = Spec {
        name: a.name.clone(),
        description: a.desc.clone(),
        root: a.root.trim_end_matches('/').to_string(),
        docroot: a.docroot.trim_end_matches('/').to_string(),
        sample: a.sample,
        addr: a.ip.clone(),
        port,
        access_log: a.log.then(|| (a.logpath.clone(), a.format())),
        keep_days: if a.rot == "Never" { 0 } else { 30 },
        user: s.default_user.clone(),
        autostart: false,
        copy_from: a.copy.then(|| a.copyfrom.clone()),
    };
    let (date, time) = sys.stamp(sys.now());
    let files = rq.env().files.as_ref();
    if let Err(e) = instance::create(files, &spec, &format!("{date} {time}"), sys.now()) {
        return failed(msgs::message(
            Kind::Error,
            "HTP8106",
            &[&a.name, &e.to_string()],
        ));
    }
    // What the generated file does not say: threads, size rotation, and
    // a copied file's port.
    let path = instance::conf_path(&spec.root);
    if let Ok(mut c) = instance::load_conf(files, &spec.root) {
        let listen = format!("{}:{port}", a.ip.as_deref().unwrap_or("*"));
        if a.copy {
            c.set("Listen", &[&listen]);
        }
        if !a.copy && s.default_threads != 40 {
            c.set("ThreadsPerChild", &[&s.default_threads.to_string()]);
        }
        if a.rot == ROTATIONS[1] {
            let maint: Vec<conf::Directive> = c.all("LogMaint").into_iter().cloned().collect();
            c.unset("LogMaint");
            for m in maint {
                let p = m.args.first().cloned().unwrap_or_default();
                c.add(conf::Directive::new(
                    "LogMaint",
                    &[&p, &m.args.get(1).cloned().unwrap_or_default(), "10"],
                ));
            }
        }
        let _ = files.write_atomic(&path, c.print().as_bytes());
    }
    if spec.user != crate::routes::instance::general::SERVER_OWNER
        && let Err(m) = rq.env().jobs.authorize(&spec.root, &spec.user)
    {
        return failed(m);
    }
    rq.clear("create.");
    rq.audit_change(&spec.name, "instance created");
    // Its TLS ports present the certificate of its application ID.
    let name = spec.name.to_ascii_uppercase();
    if let Err(e) = rq.env().certs.register_instance(&name) {
        let app = certs::apps::http_server(&name);
        return rq.done(
            "/instances",
            msgs::message(Kind::Error, "HTP8126", &[&app, &name, &e.to_string()]),
        );
    }
    rq.done(
        "/instances",
        msgs::message(Kind::Done, "HTP8020", &[&spec.name, &spec.root]),
    )
}
