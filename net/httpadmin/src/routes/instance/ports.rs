// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/instance/<name>/ports` (`instance-ports.html`): the `Listen`
//! directives, which of them a `<VirtualHost>` with `SSLEngine On` makes
//! TLS, adding and removing them.

use httpd::conf::{self, Conf, Directive, Node, Scope as _, Section};
use httpd::instance;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Button, Cell, Col, List, Prop, PropGroup, buttons, checkbox, form, input, list_table, props_table};

use super::{Rq, bad_value, edit, tab_page};
use crate::routes::create::low_port;
use crate::routes::instances::View;
use crate::{PRODUCT, msgs};

/// One `Listen` line as the page shows it.
struct Row {
    /// 1-based position among the `Listen` lines.
    index: usize,
    addr: Option<String>,
    port: u16,
    tls: bool,
    host: String,
}

/// Whether a `<VirtualHost>` serves `port` on `addr`.
fn vhost_for(c: &Conf, addr: Option<&str>, port: u16) -> bool {
    c.sections("VirtualHost").iter().any(|v| {
        v.args.iter().any(|a| {
            conf::rules::listen_addr(a).is_ok_and(|(va, vp)| vp == port && (va.is_none() || va.as_deref() == addr))
        })
    })
}

fn rows(v: &View) -> Vec<Row> {
    let Some(c) = &v.conf else { return Vec::new() };
    instance::ports(c)
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            // A Listen on one address with no virtual host of its own is
            // answered by the default host.
            let host = if p.addr.is_some() && !vhost_for(c, p.addr.as_deref(), p.port) {
                "(default)".to_string()
            } else {
                p.server_name.clone().unwrap_or_else(|| "(default)".into())
            };
            Row {
                index: i + 1,
                addr: p.addr,
                port: p.port,
                tls: p.tls,
                host,
            }
        })
        .collect()
}

/// `GET` lists the ports; `POST` adds or removes.
pub fn page(rq: &Rq<'_>, v: &View) -> Response {
    if rq.post() {
        return post(rq, v);
    }
    let mut rows = rows(v);
    match rq.query().str("sort") {
        "addr" => rows.sort_by(|a, b| a.addr.cmp(&b.addr)),
        "port" => rows.sort_by_key(|r| r.port),
        "tls" => rows.sort_by_key(|r| !r.tls),
        _ => {}
    }
    let table = list_table(&List {
        cols: vec![
            Col::plain("Select"),
            Col::sorted("Address", "addr"),
            Col::sorted("Port", "port"),
            Col::sorted("TLS", "tls"),
            Col::plain("Virtual host"),
        ],
        rows: rows
            .iter()
            .map(|r| {
                vec![
                    Cell::html(checkbox(&format!("l{}", r.index), false)),
                    Cell::mono(r.addr.as_deref().unwrap_or("*ALL")),
                    Cell::num(&r.port.to_string()),
                    Cell::text(if r.tls { "Yes" } else { "No" }),
                    Cell::text(&r.host),
                ]
            })
            .collect(),
        empty: "The instance has no Listen entries.".into(),
        keep: Vec::new(),
    });
    let add = props_table(
        &PRODUCT,
        &[PropGroup {
            title: None,
            rows: vec![
                Prop::new("Address", input("addr", "*ALL", 16)),
                Prop::new("Port", input("port", "", 6)),
                Prop::new("TLS", format!("{} Use TLS on this port", checkbox("tls", false))),
            ],
        }],
        false,
    );
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "{table}{}<h3>Add Listen entry</h3>{add}{}",
            buttons(&[Button::new("remove", "Remove")]),
            buttons(&[Button::new("add", "Add")])
        ),
    );
    rq.page(tab_page(v, "ports", Some("Listen directives in the configuration file."), &body))
}

fn post(rq: &Rq<'_>, v: &View) -> Response {
    let f = rq.form();
    if !(f.has("add") || f.has("remove")) {
        return rq.redirect(&format!("/instance/{}/ports", v.name()));
    }
    if let Err(r) = super::may_change(rq, v, "Change HTTP server ports") {
        return r;
    }
    let m = if f.has("add") { add(rq, v) } else { remove(rq, v) }.unwrap_or_else(|e| e);
    rq.done(&format!("/instance/{}/ports", v.name()), m)
}

fn add(rq: &Rq<'_>, v: &View) -> Result<Message, Message> {
    let f = rq.form();
    let a = f.str("addr").trim();
    let addr = if a.is_empty() || a.eq_ignore_ascii_case("*ALL") || a == "*" {
        None
    } else {
        let ip: std::net::IpAddr = a.parse().map_err(|_| bad_value(a, "Address", "*ALL or an IP address of this system"))?;
        if !rq.env().system.interfaces().iter().any(|i| i.parse::<std::net::IpAddr>() == Ok(ip)) {
            return Err(bad_value(a, "Address", "*ALL or an IP address of this system"));
        }
        Some(ip.to_string())
    };
    let ps = f.str("port").trim();
    let port = ps.parse::<u16>().ok().filter(|p| *p > 0).ok_or_else(|| bad_value(ps, "Port", "a number from 1 to 65535"))?;
    let taken = rows(v).iter().any(|r| r.port == port && (r.addr.is_none() || addr.is_none() || r.addr == addr));
    if taken {
        return Err(msgs::message(Kind::Error, "HTP8105", &[&port.to_string(), v.name()]));
    }
    let user = v.conf.as_ref().map(|c| c.value("ServerUserID")).unwrap_or_default();
    low_port(rq, port, &user)?;
    let listen = match &addr {
        Some(a) if a.contains(':') => format!("[{a}]:{port}"),
        Some(a) => format!("{a}:{port}"),
        None => format!("*:{port}"),
    };
    let tls = f.has("tls");
    let name = v.name();
    let app = super::tls::app_name(v.conf.as_ref(), name);
    // The port presents the certificate of the instance's application.
    if tls && app == certs::apps::http_server(name) {
        rq.env()
            .certs
            .register_instance(name)
            .map_err(|e| msgs::message(Kind::Error, "HTP8126", &[&app, name, &e.to_string()]))?;
    }
    edit(rq, v, |c| {
        c.add(Directive::new("Listen", &[&listen]));
        if tls {
            let mut s = Section::new("VirtualHost", &[&listen]);
            s.add(Directive::new("SSLEngine", &["On"]));
            s.add(Directive::new("SSLAppName", &[&app]));
            c.items.push(Node::Blank("\n".into()));
            c.items.push(Node::Section(s));
        }
        Ok(())
    })
}

fn remove(rq: &Rq<'_>, v: &View) -> Result<Message, Message> {
    let f = rq.form();
    let all = rows(v);
    let gone: Vec<&Row> = all.iter().filter(|r| f.has(&format!("l{}", r.index))).collect();
    if gone.is_empty() {
        return Err(bad_value("", "Select", "at least one Listen entry to remove"));
    }
    if gone.len() == all.len() {
        return Err(bad_value("", "Listen entries", "at least one entry left; an instance must listen somewhere"));
    }
    let gone: Vec<(Option<String>, u16)> = gone.iter().map(|r| (r.addr.clone(), r.port)).collect();
    let kept: Vec<u16> = all.iter().filter(|r| !gone.contains(&(r.addr.clone(), r.port))).map(|r| r.port).collect();
    edit(rq, v, |c| {
        c.items.retain(|n| match n {
            Node::Directive(d) if d.is("Listen") => {
                d.args.first().and_then(|a| conf::rules::listen_addr(a).ok()).is_none_or(|p| !gone.contains(&p))
            }
            // A virtual host for a port no longer listened on goes too.
            Node::Section(s) if s.name.eq_ignore_ascii_case("VirtualHost") => s.args.iter().any(|a| {
                conf::rules::listen_addr(a).is_ok_and(|(_, p)| kept.contains(&p))
            }),
            _ => true,
        });
        Ok(())
    })
}
