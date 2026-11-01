// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/instance/<name>/tls` (`instance-tls.html`): `SSLEngine`,
//! `SSLAppName`, `SSLProtocolDisable`, `SSLCipherSpec` and
//! `SSLClientAuth` in the instance's TLS virtual hosts, and the
//! certificates its application may present.

use httpd::conf::{Conf, Node, Scope as _, Section};
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::Message;
use qsweb::partials::{Button, Prop, PropGroup, checkbox, props, radio};

use super::{Rq, bad_value, edit, pick, tab_page};
use crate::PRODUCT;
use crate::routes::instances::View;

/// The cipher choices; the first is the system value's list.
const CIPHERS: &[&str] = &["*DFT (system value QSSLCSL)", "*STRONG", "Custom..."];

/// The protocol boxes: field, label, `SSLProtocolDisable` word.
const PROTOCOLS: &[(&str, &str, &str)] = &[
    ("t13", "TLS 1.3", "TLSv1.3"),
    ("t12", "TLS 1.2", "TLSv1.2"),
    ("t11", "TLS 1.1 (not recommended)", "TLSv1.1"),
];

/// Whether a virtual host is a TLS one.
fn is_tls(s: &Section) -> bool {
    s.name.eq_ignore_ascii_case("VirtualHost") && (s.get("SSLEngine").is_some() || s.get("SSLAppName").is_some())
}

fn tls_hosts(c: &Conf) -> Vec<&Section> {
    c.sections("VirtualHost").into_iter().filter(|s| is_tls(s)).collect()
}

/// The instance's certificate application.
pub fn app_name(c: Option<&Conf>, name: &str) -> String {
    c.and_then(|c| tls_hosts(c).iter().map(|s| s.value("SSLAppName")).find(|a| !a.is_empty()))
        .unwrap_or_else(|| format!("QIBM_HTTP_SERVER_{name}"))
}

/// `GET` shows the settings; `POST` applies them.
pub fn page(rq: &Rq<'_>, v: &View) -> Response {
    if rq.post() {
        return post(rq, v);
    }
    let empty = Conf::default();
    let c = v.conf.as_ref().unwrap_or(&empty);
    let hosts = tls_hosts(c);
    let first = hosts.first();
    let on = hosts.iter().any(|s| s.value("SSLEngine").eq_ignore_ascii_case("on"));
    let app = app_name(v.conf.as_ref(), v.name());
    let disabled = first.map(|s| s.value("SSLProtocolDisable")).unwrap_or_default();
    let disabled: Vec<&str> = disabled.split_whitespace().collect();
    let cipher = match first.map(|s| s.value("SSLCipherSpec")).unwrap_or_default().as_str() {
        "" => CIPHERS[0],
        "*STRONG" => CIPHERS[1],
        _ => CIPHERS[2],
    };
    let auth = match first.map(|s| s.value("SSLClientAuth")).unwrap_or_default().to_ascii_lowercase().as_str() {
        "optional" | "1" => "opt",
        "required" | "2" => "req",
        _ => "none",
    };
    // Certificates still good for 60 days; one expiring sooner is the
    // store page's warning, not a choice.
    let soon = rq.env().system.now() + 60 * 86_400;
    let certs: Vec<String> = rq
        .env()
        .certs
        .for_app(&app)
        .into_iter()
        .filter(|(_, expires)| *expires > soon)
        .map(|(label, expires)| format!("{label} (expires {})", rq.stamp(expires).0))
        .collect();
    let certs: Vec<&str> = certs.iter().map(String::as_str).collect();
    let protocols = PROTOCOLS
        .iter()
        .map(|(f, l, w)| format!("{} {}", checkbox(f, !disabled.contains(w)), esc(l)))
        .collect::<Vec<_>>()
        .join("<br>");
    let client = [("none", "None"), ("opt", "Optional"), ("req", "Required")]
        .iter()
        .map(|(v, l)| format!("{} {l}", radio("ca", v, auth == *v)))
        .collect::<Vec<_>>()
        .join("<br>");
    let groups = [
        PropGroup {
            title: Some("Transport Layer Security".into()),
            rows: vec![
                Prop::new(
                    "Enable TLS",
                    format!("{} Use TLS for ports marked TLS on the Ports &amp; addresses page", checkbox("tls", on)),
                )
                .help("instance-tls-enable"),
                Prop::new(
                    "Certificate application",
                    format!(
                        "<span class=\"mono\">{}</span> &nbsp; <a href=\"{}\">Certificate applications</a>",
                        esc(&app),
                        esc(&PRODUCT.url("/certs/apps"))
                    ),
                ),
                Prop::new("Certificate", pick("cert", &certs, "")),
                Prop::new("Certificate store", "<span class=\"mono\">*SYSTEM</span>".into()),
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
        &[Button::new("apply", "Apply"), Button::new("ok", "OK"), Button::new("cancel", "Cancel")],
    );
    rq.page(tab_page(
        v,
        "tls",
        Some("TLS settings. The certificate is chosen from the certificates assigned to the instance's certificate application."),
        &body,
    ))
}

fn post(rq: &Rq<'_>, v: &View) -> Response {
    let form = rq.form();
    if !(form.has("apply") || form.has("ok")) {
        return rq.redirect("/instances");
    }
    if let Err(r) = super::may_change(rq, v, "Change HTTP server TLS") {
        return r;
    }
    let msg = apply(rq, v).unwrap_or_else(|e| e);
    super::after(rq, v, "tls", msg)
}

fn apply(rq: &Rq<'_>, v: &View) -> Result<Message, Message> {
    let form = rq.form();
    let on = form.has("tls");
    let mut disable = vec!["TLSv1".to_string()];
    for (field, _, word) in PROTOCOLS.iter().rev() {
        if !form.has(field) {
            disable.push((*word).to_string());
        }
    }
    if disable.len() == PROTOCOLS.len() + 1 {
        return Err(bad_value("", "Protocol versions", "at least one TLS version"));
    }
    let cipher = form.str("cipher");
    let auth = match form.str("ca") {
        "opt" => Some("Optional"),
        "req" => Some("Required"),
        _ => None,
    };
    let app = app_name(v.conf.as_ref(), v.name());
    edit(rq, v, |c| {
        if !c.items.iter().any(|n| matches!(n, Node::Section(s) if is_tls(s))) {
            if on {
                return Err(bad_value("", "Enable TLS", "used with a TLS port: add one on the Ports & addresses page first"));
            }
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
            s.set("SSLProtocolDisable", &disable.iter().map(String::as_str).collect::<Vec<_>>());
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
