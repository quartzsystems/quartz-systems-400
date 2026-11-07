// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Global settings (`global.html`): defaults for new instances, limits,
//! the administration server's own rules and the audit flag. Changing
//! them needs `*IOSYSCFG`.

use std::time::Duration;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Button, Prop, PropGroup, checkbox, input, input_cmd, props};

use super::Rq;
use super::instances;
use crate::env::{Audit, Need};
use crate::settings::{PATH, Settings, in_net};
use crate::{SELF_INSTANCE, msgs};

/// The administration server's certificate application ID.
pub const ADMIN_APP_ID: &str = "QS400_HTTP_SERVER_ADMIN";

const CRUMB: [&str; 2] = ["Advanced", "Global settings"];

/// The administration instance's plain port (2001 unless its
/// configuration says otherwise).
fn admin_port(rq: &Rq<'_>) -> u16 {
    instances::one(rq.env(), SELF_INSTANCE)
        .and_then(|v| v.ports.first().map(|p| p.port))
        .unwrap_or(2001)
}

/// `GET /global`, `POST` applies.
pub fn page(rq: &Rq<'_>) -> Response {
    if let Err(r) = rq.require(Need::IoSysCfg, "Change global settings", PATH, &CRUMB) {
        return r;
    }
    if rq.post() {
        return apply(rq);
    }
    let s = Settings::load(rq.env().files.as_ref());
    let groups = [
        PropGroup {
            title: Some("Defaults for new instances".into()),
            rows: vec![
                Prop::new(
                    "Default server user profile",
                    input("usrprf", &s.default_user, 10),
                ),
                Prop::new(
                    "Default server root",
                    input_cmd("root", &s.default_root, 30),
                ),
                Prop::new(
                    "Default threads per instance",
                    input("thr", &s.default_threads.to_string(), 4),
                ),
            ],
        },
        PropGroup {
            title: Some("Limits".into()),
            rows: vec![
                Prop::new(
                    "Maximum instances",
                    input("max", &s.max_instances.to_string(), 4),
                ),
                Prop::new(
                    "Maximum threads per instance",
                    input("maxthr", &s.max_threads.to_string(), 4),
                ),
            ],
        },
        PropGroup {
            title: Some(format!(
                "Administration server ({SELF_INSTANCE}, port {})",
                admin_port(rq)
            )),
            rows: vec![
                Prop::new(
                    "Require TLS for administration",
                    format!(
                        "{} Redirect port {} to {} (TLS)",
                        checkbox("admtls", s.admin_tls),
                        admin_port(rq),
                        s.admin_tls_port
                    ),
                )
                .help("global-tls"),
                Prop::new(
                    "Administration certificate application",
                    format!("<span class=\"mono\">{}</span>", esc(ADMIN_APP_ID)),
                ),
                Prop::new(
                    "Session timeout",
                    format!("{} minutes", input("to", &s.session_minutes.to_string(), 3)),
                ),
                Prop::new(
                    "Allow administration from",
                    input_cmd("from", &s.allow_from.join(", "), 40),
                ),
            ],
        },
        PropGroup {
            title: Some("Logging".into()),
            rows: vec![Prop::new(
                "Administration audit log",
                format!(
                    "{} Write every change to the QAUDJRN journal (entry type ZC)",
                    checkbox("audit", s.audit)
                ),
            )],
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
    rq.page(Page {
        title: "Global settings".into(),
        crumb: CRUMB.iter().map(ToString::to_string).collect(),
        h1: "Global settings".into(),
        desc: Some(
            "Settings that apply to the administration server and to every instance.".into(),
        ),
        body,
        help: "global".into(),
        nav: Some(("Advanced", Some("/global"))),
        ..Page::default()
    })
}

fn num(
    f: &qsweb::http::Form,
    name: &str,
    what: &str,
    range: std::ops::RangeInclusive<u32>,
) -> Result<u32, Message> {
    f.str(name)
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| range.contains(n))
        .ok_or_else(|| invalid(&format!("{what} is {} to {}", range.start(), range.end())))
}

fn invalid(what: &str) -> Message {
    msgs::message(Kind::Error, "HTP8322", &[what])
}

/// The settings the form describes, from `old`.
fn read(rq: &Rq<'_>, old: &Settings) -> Result<Settings, Message> {
    let f = rq.form();
    let user = f.str("usrprf").trim().to_ascii_uppercase();
    if !rq.env().system.profile_exists(&user) {
        return Err(msgs::message(Kind::Error, "HTP8201", &[&user]));
    }
    let mut root = f.str("root").trim().to_string();
    if !root.starts_with('/') {
        return Err(invalid("the default server root must start with /"));
    }
    if !root.ends_with('/') {
        root.push('/');
    }
    let max_threads = num(&f, "maxthr", "the maximum threads per instance", 1..=9999)?;
    let threads = num(&f, "thr", "the default threads per instance", 1..=9999)?;
    if threads > max_threads {
        return Err(invalid(
            "the default threads per instance exceed the maximum",
        ));
    }
    let allow_from: Vec<String> = f
        .str("from")
        .split([',', ' '])
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(String::from)
        .collect();
    for n in &allow_from {
        let (a, bits) = n.split_once('/').unwrap_or((n, "0"));
        let ip = a.parse::<std::net::IpAddr>();
        let max = if matches!(ip, Ok(std::net::IpAddr::V6(_))) {
            128
        } else {
            32
        };
        if ip.is_err() || bits.parse::<u32>().map_or(true, |b| b > max) {
            return Err(invalid(&format!(
                "{n} is not an address or network such as 10.0.0.0/8"
            )));
        }
    }
    if f.has("admtls") && !old.admin_tls && !rq.admin.tls_up() {
        return Err(invalid(&format!(
            "TLS cannot be required: port {} is not listening (is a certificate assigned to QS400_HTTP_SERVER_ADMIN?)",
            old.admin_tls_port
        )));
    }
    let me = rq.peer.addr.ip();
    if !allow_from.is_empty() && !allow_from.iter().any(|n| in_net(me, n)) {
        return Err(invalid(&format!(
            "the allowed networks do not include your own address {me}"
        )));
    }
    Ok(Settings {
        default_user: user,
        default_root: root,
        default_threads: threads,
        max_instances: num(&f, "max", "the maximum number of instances", 1..=999)?,
        max_threads,
        admin_tls: f.has("admtls"),
        session_minutes: num(&f, "to", "the session timeout", 1..=1440)?,
        allow_from,
        audit: f.has("audit"),
        ..old.clone()
    })
}

/// What changed, for the audit entry.
fn changes(a: &Settings, b: &Settings) -> String {
    let mut c = Vec::new();
    let mut d = |name: &str, x: String, y: String| {
        if x != y {
            c.push(format!("{name} {x} to {y}"));
        }
    };
    d(
        "default user",
        a.default_user.clone(),
        b.default_user.clone(),
    );
    d(
        "default root",
        a.default_root.clone(),
        b.default_root.clone(),
    );
    d(
        "default threads",
        a.default_threads.to_string(),
        b.default_threads.to_string(),
    );
    d(
        "maximum instances",
        a.max_instances.to_string(),
        b.max_instances.to_string(),
    );
    d(
        "maximum threads",
        a.max_threads.to_string(),
        b.max_threads.to_string(),
    );
    d(
        "TLS required",
        a.admin_tls.to_string(),
        b.admin_tls.to_string(),
    );
    d(
        "session timeout",
        a.session_minutes.to_string(),
        b.session_minutes.to_string(),
    );
    d(
        "allowed networks",
        a.allow_from.join(","),
        b.allow_from.join(","),
    );
    d("audit", a.audit.to_string(), b.audit.to_string());
    if c.is_empty() {
        "no change".into()
    } else {
        c.join("; ")
    }
}

fn apply(rq: &Rq<'_>) -> Response {
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect("/instances");
    }
    let files = rq.env().files.as_ref();
    let old = Settings::load(files);
    let new = match read(rq, &old) {
        Ok(s) => s,
        Err(m) => return rq.done("/global", m),
    };
    if let Err(e) = new.save(files) {
        return rq.done(
            "/global",
            invalid(&format!("the settings could not be written: {e}")),
        );
    }
    rq.admin
        .sessions
        .set_idle(Duration::from_secs(u64::from(new.session_minutes) * 60));
    // The change that turns auditing off is itself audited.
    if old.audit || new.audit {
        rq.env().system.audit(Audit::Change {
            user: rq.user().to_string(),
            object: PATH.to_string(),
            what: changes(&old, &new),
        });
    }
    let m = msgs::message(Kind::Done, "HTP8330", &[PATH]);
    if f.has("ok") {
        rq.done("/instances", m)
    } else {
        rq.done("/global", m)
    }
}
