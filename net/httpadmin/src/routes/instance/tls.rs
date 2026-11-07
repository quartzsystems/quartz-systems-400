// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/instance/<name>/tls` (`instance-tls.html`): `SSLEngine`,
//! `SSLAppName`, `SSLProtocolDisable`, `SSLCipherSpec` and
//! `SSLClientAuth` in the instance's TLS virtual hosts, and the
//! certificate its application presents: choosing one assigns it, and
//! registers the instance's application ID when it is not.

use httpd::conf::{Conf, Node, Scope as _, Section};
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Button, Prop, PropGroup, checkbox, props, radio};

use super::{Rq, bad_value, edit, pick, tab_page};
use crate::env::Need;
use crate::routes::certs::apps::label_of;
use crate::routes::certs::failed;
use crate::routes::instances::View;
use crate::{PRODUCT, msgs};

/// The cipher choices; the first is the system value's list.
const CIPHERS: &[&str] = &["*DFT (system value QSSLCSL)", "*STRONG", "Custom..."];

/// The protocol boxes: field, label, `SSLProtocolDisable` word.
const PROTOCOLS: &[(&str, &str, &str)] =
    &[("t13", "TLS 1.3", "TLSv1.3"), ("t12", "TLS 1.2", "TLSv1.2")];

/// The versions the server never offers, always written disabled.
const NEVER: &[&str] = &["TLSv1", "TLSv1.1"];

/// The Certificate choice when none is assigned.
const NONE: &str = "(none)";

/// Whether a virtual host is a TLS one.
fn is_tls(s: &Section) -> bool {
    s.name.eq_ignore_ascii_case("VirtualHost")
        && (s.get("SSLEngine").is_some() || s.get("SSLAppName").is_some())
}

fn tls_hosts(c: &Conf) -> Vec<&Section> {
    c.sections("VirtualHost")
        .into_iter()
        .filter(|s| is_tls(s))
        .collect()
}

/// The instance's certificate application (an old ID as its new one).
pub fn app_name(c: Option<&Conf>, name: &str) -> String {
    c.and_then(|c| {
        tls_hosts(c)
            .iter()
            .map(|s| s.value("SSLAppName"))
            .find(|a| !a.is_empty())
    })
    .map_or_else(
        || certs::apps::http_server(name),
        |a| certs::apps::canonical(&a).into_owned(),
    )
}

/// `GET` shows the settings; `POST` applies them.
#[allow(clippy::too_many_lines)]
pub fn page(rq: &Rq<'_>, v: &View) -> Response {
    if rq.post() {
        return post(rq, v);
    }
    let empty = Conf::default();
    let c = v.conf.as_ref().unwrap_or(&empty);
    let hosts = tls_hosts(c);
    let first = hosts.first();
    let on = hosts
        .iter()
        .any(|s| s.value("SSLEngine").eq_ignore_ascii_case("on"));
    let app = app_name(v.conf.as_ref(), v.name());
    let disabled = first
        .map(|s| s.value("SSLProtocolDisable"))
        .unwrap_or_default();
    let disabled: Vec<&str> = disabled.split_whitespace().collect();
    let cipher = match first
        .map(|s| s.value("SSLCipherSpec"))
        .unwrap_or_default()
        .as_str()
    {
        "" => CIPHERS[0],
        "*STRONG" => CIPHERS[1],
        _ => CIPHERS[2],
    };
    let auth = match first
        .map(|s| s.value("SSLClientAuth"))
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "optional" | "1" => "opt",
        "required" | "2" => "req",
        _ => "none",
    };
    let protocols = PROTOCOLS
        .iter()
        .map(|(f, l, w)| format!("{} {}", checkbox(f, !disabled.contains(w)), esc(l)))
        .collect::<Vec<_>>()
        .join("<br>");
    let protocols = format!(
        "{protocols}<br><span class=\"small grey\">TLS 1.1 and older are not offered.</span>"
    );
    let enable = format!(
        "{} Use TLS for ports marked TLS on the Ports &amp; addresses page",
        checkbox("tls", on)
    );
    let enable = if hosts.is_empty() {
        format!(
            "{enable}<br><span class=\"small grey\">The instance has no TLS port. Add one on the <a href=\"{}\">Ports &amp; addresses</a> page first.</span>",
            esc(&PRODUCT.url(&format!("/instance/{}/ports", v.name())))
        )
    } else {
        enable
    };
    let client = [("none", "None"), ("opt", "Optional"), ("req", "Required")]
        .iter()
        .map(|(v, l)| format!("{} {l}", radio("ca", v, auth == *v)))
        .collect::<Vec<_>>()
        .join("<br>");
    let groups = [
        PropGroup {
            title: Some("Transport Layer Security".into()),
            rows: vec![
                Prop::new("Enable TLS", enable).help("instance-tls-enable"),
                Prop::new(
                    "Certificate application",
                    format!(
                        "<span class=\"mono\">{}</span> &nbsp; <a href=\"{}\">Certificate applications</a>",
                        esc(&app),
                        esc(&PRODUCT.url("/certs/apps"))
                    ),
                ),
                Prop::new("Certificate", cert_choice(rq, &app)),
                Prop::new(
                    "Certificate store",
                    "<span class=\"mono\">*SYSTEM</span>".into(),
                ),
            ],
        },
        PropGroup {
            title: Some("Protocols".into()),
            rows: vec![
                Prop::new("Protocol versions", protocols),
                Prop::new("Cipher specification", pick("cipher", CIPHERS, cipher)),
                Prop::new("Client authentication", client),
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
        "tls",
        Some("TLS settings. The certificate chosen is assigned to the instance's certificate application."),
        &body,
    ))
}

/// The certificates of `*SYSTEM` the application may present, the
/// assigned one first, as `label (expires date)`; `(none)` leads when
/// none is assigned.
fn choices(rq: &Rq<'_>, app: &str) -> Vec<String> {
    // Certificates still good for 60 days; one expiring sooner is the
    // store page's warning, not a choice.
    let soon = rq.env().system.now() + 60 * 86_400;
    let cm = &rq.env().certs;
    let assigned = cm.app_label(app);
    let mut out: Vec<String> = cm
        .for_app(app)
        .into_iter()
        .filter(|(_, expires)| *expires > soon)
        .map(|(label, expires)| format!("{label} (expires {})", rq.stamp(expires).0))
        .collect();
    let first_assigned = out
        .first()
        .is_some_and(|o| assigned.as_deref() == Some(label_of(o)));
    if !out.is_empty() && !first_assigned {
        out.insert(0, NONE.into());
    }
    out
}

/// The Certificate row: the choices, or where to get one.
fn cert_choice(rq: &Rq<'_>, app: &str) -> String {
    let options = choices(rq, app);
    if options.is_empty() {
        return format!(
            "<span class=\"grey\">No server certificate of store *SYSTEM can be chosen.</span> <a href=\"{}\">Create or import one</a>",
            esc(&PRODUCT.url("/certs/store/*SYSTEM"))
        );
    }
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    pick("cert", &options, "")
}

/// The label chosen on the form, if any.
fn chosen(rq: &Rq<'_>) -> Option<String> {
    let c = label_of(rq.form().str("cert").trim()).to_string();
    (!c.is_empty() && c != NONE).then_some(c)
}

fn post(rq: &Rq<'_>, v: &View) -> Response {
    let form = rq.form();
    if !(form.has("apply") || form.has("ok")) {
        return rq.redirect("/instances");
    }
    if let Err(r) = super::may_change(rq, v, "Change HTTP server TLS") {
        return r;
    }
    // Assigning a certificate is certificate management.
    let app = app_name(v.conf.as_ref(), v.name());
    if let Some(c) = chosen(rq)
        && rq.env().certs.app_label(&app).as_deref() != Some(c.as_str())
        && let Err(r) = rq.require_each(
            &[Need::AllObj, Need::SecAdm],
            "Assign certificate",
            &app,
            &["Manage", "Server instances", v.name()],
        )
    {
        return r;
    }
    let msg = apply(rq, v).unwrap_or_else(|e| e);
    super::after(rq, v, "tls", msg)
}

fn apply(rq: &Rq<'_>, v: &View) -> Result<Message, Message> {
    let form = rq.form();
    let on = form.has("tls");
    let mut disable: Vec<String> = NEVER.iter().map(ToString::to_string).collect();
    for (field, _, word) in PROTOCOLS.iter().rev() {
        if !form.has(field) {
            disable.push((*word).to_string());
        }
    }
    if disable.len() == PROTOCOLS.len() + NEVER.len() {
        return Err(bad_value(
            "",
            "Protocol versions",
            "at least one TLS version",
        ));
    }
    let cipher = form.str("cipher");
    let auth = match form.str("ca") {
        "opt" => Some("Optional"),
        "req" => Some("Required"),
        _ => None,
    };
    let app = app_name(v.conf.as_ref(), v.name());
    let name = v.name();
    let has_port = v.conf.as_ref().is_some_and(|c| !tls_hosts(c).is_empty());
    if on && !has_port {
        return Err(msgs::message(Kind::Error, "HTP8124", &[name]));
    }
    let cm = &rq.env().certs;
    let assigned = cm.app_label(&app);
    let chosen = chosen(rq);
    // Without an assignment the store's default certificate is presented.
    let default = || {
        cm.stores()
            .into_iter()
            .find(|s| s.name == "*SYSTEM")
            .and_then(|s| s.default)
    };
    if on && chosen.is_none() && assigned.is_none() && default().is_none() {
        return Err(msgs::message(Kind::Error, "HTP8125", &[&app, name]));
    }
    // The instance's own ID is registered before it is used; one the
    // file names otherwise is someone's to register.
    if (on || chosen.is_some()) && app == certs::apps::http_server(name) {
        cm.register_instance(name)
            .map_err(|e| msgs::message(Kind::Error, "HTP8126", &[&app, name, &e.to_string()]))?;
    }
    if let Some(c) = chosen.filter(|c| assigned.as_ref() != Some(c)) {
        cm.assign(rq.user(), &app, Some(&c))
            .map_err(|e| failed(&e))?;
        rq.audit_change(&app, &format!("certificate {c} assigned"));
    }
    edit(rq, v, |c| {
        if !c
            .items
            .iter()
            .any(|n| matches!(n, Node::Section(s) if is_tls(s)))
        {
            return Ok(());
        }
        for n in &mut c.items {
            let Node::Section(s) = n else { continue };
            if !is_tls(s) {
                continue;
            }
            s.set("SSLEngine", &[if on { "On" } else { "Off" }]);
            if s.get("SSLAppName").is_none() {
                s.set("SSLAppName", &[&app]);
            }
            s.set(
                "SSLProtocolDisable",
                &disable.iter().map(String::as_str).collect::<Vec<_>>(),
            );
            if cipher == CIPHERS[0] {
                s.unset("SSLCipherSpec");
            } else if cipher == CIPHERS[1] {
                s.set("SSLCipherSpec", &["*STRONG"]);
            } else if s.get("SSLCipherSpec").is_none() {
                return Err(bad_value(
                    cipher,
                    "Cipher specification",
                    "*DFT or *STRONG; a custom list is written with SSLCipherSpec on the Configuration file page",
                ));
            }
            match auth {
                Some(a) => s.set("SSLClientAuth", &[a]),
                None => {
                    s.unset("SSLClientAuth");
                }
            }
        }
        Ok(())
    })
}
