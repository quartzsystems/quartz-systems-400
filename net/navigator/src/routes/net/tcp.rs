// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/tcp`: the host, domain and name servers (`CHGTCPDMN`), datagram
//! forwarding (`CHGTCPA`) and TCP/IP's status; `/tcp/hosts`: the host
//! table.

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Button, Cell, Col, List, Prop, PropGroup, input, list_table, mono, props};

use super::{arg, cfg_tabs, need, page, yes};
use crate::env::net::TcpConfig;
use crate::routes::Rq;
use crate::routes::work::{pick, quote, reply, strings};

/// `CHGTCPDMN` and `CHGTCPA` for what the form changed.
fn commands(c: &TcpConfig, f: &qsweb::http::Form) -> Vec<String> {
    let mut dmn = String::new();
    let mut add = |kwd: &str, new: &str, old: &str, val: String| {
        if new != old {
            let _ = write!(dmn, " {kwd}({val})");
        }
    };
    let host = f.str("h").trim();
    add("HOSTNAME", host, &c.host, quote(host));
    let domain = f.str("d").trim();
    add("DMNNAME", domain, &c.domain, quote(domain));
    let search = f.str("ds").split_whitespace().collect::<Vec<_>>().join(" ");
    add("DMNSCHLIST", &search, &c.search, quote(&search));
    let p = if f.str("p") == "*REMOTE" {
        "*REMOTE"
    } else {
        "*LOCAL"
    };
    add("HOSTSCHPTY", p, &c.priority, p.to_string());
    let dns: Vec<String> = ["n1", "n2", "n3"]
        .iter()
        .map(|k| f.str(k).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let list = if dns.is_empty() {
        "*NONE".to_string()
    } else {
        dns.iter().map(|a| arg(a)).collect::<Vec<_>>().join(" ")
    };
    add("INTNETADR", &dns.join(" "), &c.dns.join(" "), list);
    let mut cmds = Vec::new();
    if !dmn.is_empty() {
        cmds.push(format!("CHGTCPDMN{dmn}"));
    }
    let fwd = f.str("f") == "*YES";
    if fwd != c.forwarding {
        cmds.push(format!("CHGTCPA IPDTGFWD({})", yes(fwd)));
    }
    cmds
}

/// `/tcp`: Apply and OK run the commands for what changed.
#[allow(clippy::too_many_lines)]
pub fn overview(rq: &Rq<'_>) -> Response {
    let net = &rq.env().net;
    let c = net.config(rq.user());
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/home");
        }
        let back = if f.has("ok") { "/home" } else { "/tcp" };
        let cmds = commands(&c, &f);
        if cmds.is_empty() {
            return rq.redirect(back);
        }
        if let Err(r) = need(
            rq,
            "Change TCP/IP domain",
            "CHGTCPDMN",
            "TCP/IP configuration",
        ) {
            return r;
        }
        let mut last = None;
        for cmd in &cmds {
            match net.run(rq.user(), cmd) {
                Err(m) => return reply(rq, Err(m), "/tcp"),
                ok => last = Some(ok),
            }
        }
        return match last {
            Some(r) => reply(rq, r, back),
            None => rq.redirect(back),
        };
    }
    let ifcs = net.interfaces(rq.user());
    let routes = net.routes(rq.user());
    let up = ifcs.iter().filter(|i| i.active).count();
    let started = net.servers(rq.user()).iter().filter(|s| s.started).count();
    let dft = routes.iter().find(|r| r.dest == "*DFTROUTE").map_or_else(
        || "None".to_string(),
        |r| {
            let line = ifcs
                .iter()
                .find(|i| i.address == r.interface)
                .map_or("", |i| i.line.as_str());
            format!(
                "{} via {} &nbsp; <a href=\"{}\">Routes</a>",
                mono(&r.next_hop),
                esc(line),
                esc(&rq.ctx.url("/tcp/rte"))
            )
        },
    );
    let dns = |i: usize| c.dns.get(i).cloned().unwrap_or_default();
    let ipv6 = c.ipv6.map_or_else(
        || "Not enabled".to_string(),
        |n| format!("Enabled, {n} interface{}", if n == 1 { "" } else { "s" }),
    );
    let status = c.active_since.map_or_else(
        || qsweb::partials::status("Ended", Some(false)),
        |at| {
            format!(
                "{} since {}",
                qsweb::partials::status("Active", Some(true)),
                rq.when(at)
            )
        },
    );
    let groups = [
        PropGroup {
            title: Some("Host".into()),
            rows: vec![
                Prop::new("Host name", input("h", &c.host, 20)),
                Prop::new("Domain name", input("d", &c.domain, 30)),
                Prop::new("Domain search list", input("ds", &c.search, 40)),
                Prop::new(
                    "Host name search priority",
                    pick("p", &strings(&["*LOCAL", "*REMOTE"]), &c.priority),
                ),
            ],
        },
        PropGroup {
            title: Some("Name servers".into()),
            rows: vec![
                Prop::new("DNS server 1", input("n1", &dns(0), 16)),
                Prop::new("DNS server 2", input("n2", &dns(1), 16)),
                Prop::new("DNS server 3", input("n3", &dns(2), 16)),
            ],
        },
        PropGroup {
            title: Some("Routing".into()),
            rows: vec![
                Prop::new("Default route", dft),
                Prop::new(
                    "IP datagram forwarding",
                    pick("f", &strings(&["*NO", "*YES"]), yes(c.forwarding)),
                ),
                Prop::new("IPv6", ipv6),
            ],
        },
        PropGroup {
            title: Some("Status".into()),
            rows: vec![
                Prop::new("TCP/IP", status),
                Prop::new(
                    "Interfaces",
                    format!(
                        "<a href=\"{}\">{up} active, {} inactive</a>",
                        esc(&rq.ctx.url("/tcp/ifc")),
                        ifcs.len() - up
                    ),
                ),
                Prop::new(
                    "Servers",
                    format!(
                        "<a href=\"{}\">{started} started</a>",
                        esc(&rq.ctx.url("/tcp/servers"))
                    ),
                ),
            ],
        },
    ];
    let body = format!(
        "{}{}",
        cfg_tabs(rq, 0),
        props(
            &rq.ctx,
            &groups,
            &[
                Button::new("apply", "Apply"),
                Button::new("ok", "OK"),
                Button::new("cancel", "Cancel"),
            ],
        )
    );
    page(
        rq,
        &["TCP/IP configuration"],
        "TCP/IP configuration",
        Some("CFGTCP options as pages."),
        body,
        "tcp",
        Some("/tcp"),
    )
}

/// `/tcp/hosts`: the host table.
pub fn hosts(rq: &Rq<'_>) -> Response {
    let rows = rq
        .env()
        .net
        .hosts(rq.user())
        .iter()
        .map(|h| vec![Cell::mono(&h.address), Cell::mono(&h.names.join(" "))])
        .collect();
    let list = List {
        cols: vec![Col::plain("Internet address"), Col::plain("Host names")],
        rows,
        empty: "The host table is empty.".into(),
        keep: Vec::new(),
    };
    let body = format!("{}{}", cfg_tabs(rq, 3), list_table(&list));
    page(
        rq,
        &["Host table"],
        "TCP/IP host table",
        None,
        body,
        "tcp",
        None,
    )
}
