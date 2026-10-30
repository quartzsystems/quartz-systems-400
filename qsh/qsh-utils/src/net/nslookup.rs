// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `nslookup [-option ...] [name | -] [server]`: ask the name server.
//! With a name it answers and ends; without one it reads commands from
//! standard input (`name [server]`, `server name`, `set type=x`, `set
//! all`, `exit`). The `Server:`, `Address:`, `Name:` and record lines
//! are the forms scripts parse.

use qsh::sys::services::{rcode, rtype};
use qsh::sys::{DnsAnswer, DnsRecord, UtilCtx};

use super::dns::{ask, bare, ip, parse_type, reverse, soa_fields};
use crate::common::io::{Lines, Out, stdin};

#[derive(Clone)]
struct Settings {
    qtype: Option<u16>,
    /// The server named by the user and its address.
    server: Option<(String, String)>,
    port: u16,
    recurse: bool,
}

const NAME: &str = "nslookup";

/// Run `nslookup`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let mut st = Settings {
        qtype: None,
        server: None,
        port: 53,
        recurse: true,
    };
    let mut ops: Vec<String> = Vec::new();
    let mut interactive_dash = false;
    for a in &cx.argv[1..] {
        if a == "-" {
            interactive_dash = true;
        } else if let Some(opt) = a.strip_prefix('-') {
            if let Err(msg) = set_option(&mut st, opt) {
                crate::common::complain(cx, NAME, &msg);
                return 2;
            }
        } else {
            ops.push(a.clone());
        }
    }
    let mut out = Out::stdout(cx);
    let server_op = if interactive_dash {
        ops.first()
    } else {
        ops.get(1)
    };
    if let Some(s) = server_op
        && let Err(msg) = use_server(cx, &mut st, s)
    {
        crate::common::complain(cx, NAME, &msg);
        return 1;
    }
    let status = match ops.first() {
        Some(name) if !interactive_dash => lookup(cx, &mut out, &st, name),
        _ => interactive(cx, &mut out, &mut st),
    };
    out.finish(cx, NAME, status)
}

/// Apply `-opt` or a `set opt` value.
fn set_option(st: &mut Settings, opt: &str) -> Result<(), String> {
    let (k, v) = opt.split_once('=').unwrap_or((opt, ""));
    let k = k.to_ascii_lowercase();
    match k.as_str() {
        "type" | "query" | "querytype" | "q" | "ty" => match parse_type(v) {
            Some(t) => st.qtype = Some(t),
            None => return Err(format!("The record type {v} is not known.")),
        },
        "port" | "po" => match v.parse() {
            Ok(p) => st.port = p,
            Err(_) => return Err(format!("The port {v} is not valid.")),
        },
        "recurse" | "rec" => st.recurse = true,
        "norecurse" | "norec" => st.recurse = false,
        "timeout" | "retry" | "domain" | "class" | "cl" | "ndots" | "debug" | "nodebug" | "d2"
        | "nod2" | "vc" | "novc" | "search" | "nosearch" | "defname" | "nodefname" => {}
        _ => return Err(format!("The option -{opt} is not valid.")),
    }
    Ok(())
}

fn use_server(cx: &UtilCtx<'_>, st: &mut Settings, s: &str) -> Result<(), String> {
    if ip(s).is_some() {
        st.server = Some((s.to_string(), s.to_string()));
        return Ok(());
    }
    let found = ask(cx, s, rtype::A, None, true)
        .ok()
        .and_then(|a| {
            a.answers
                .iter()
                .find(|r| r.rtype == rtype::A)
                .map(|r| r.data.clone())
        });
    match found {
        Some(addr) => {
            st.server = Some((s.to_string(), addr));
            Ok(())
        }
        None => Err(format!("The server {s} cannot be found.")),
    }
}

fn header(out: &mut Out, label: &str, addr: &str, port: u16) {
    out.text(&format!("Server:\t\t{label}\nAddress:\t{addr}#{port}\n\n"));
}

/// The lines of one record.
pub fn record_lines(r: &DnsRecord) -> String {
    let n = &r.name;
    match r.rtype {
        rtype::A | rtype::AAAA => format!("Name:\t{n}\nAddress: {}\n", r.data),
        rtype::CNAME => format!("{n}\tcanonical name = {}\n", r.data),
        rtype::MX => format!("{n}\tmail exchanger = {}\n", r.data),
        rtype::NS => format!("{n}\tnameserver = {}\n", r.data),
        rtype::TXT => format!("{n}\ttext = {}\n", r.data),
        rtype::PTR => format!("{n}\tname = {}\n", r.data),
        rtype::SRV => format!("{n}\tservice = {}\n", r.data),
        rtype::SOA => match soa_fields(&r.data) {
            Some(f) => format!(
                "{n}\n\torigin = {}\n\tmail addr = {}\n\tserial = {}\n\trefresh = {}\n\tretry = {}\n\texpire = {}\n\tminimum = {}\n",
                bare(f[0]),
                bare(f[1]),
                f[2],
                f[3],
                f[4],
                f[5],
                f[6]
            ),
            None => format!("{n}\tSOA = {}\n", r.data),
        },
        t => format!("{n}\t{} = {}\n", rtype::name(t), r.data),
    }
}

/// Look up one name; the status (0 found, 1 not).
fn lookup(cx: &UtilCtx<'_>, out: &mut Out, st: &Settings, name: &str) -> i32 {
    let (qname, types) = match (st.qtype, ip(name)) {
        (None, Some(a)) => (reverse(a), vec![rtype::PTR]),
        (Some(t), Some(a)) if t == rtype::PTR => (reverse(a), vec![t]),
        (Some(t), _) => (name.to_string(), vec![t]),
        (None, None) => (name.to_string(), vec![rtype::A, rtype::AAAA]),
    };
    let server = st.server.as_ref().map(|(_, a)| a.as_str());
    let mut answers: Vec<DnsAnswer> = Vec::new();
    for t in types {
        match ask(cx, &qname, t, server, st.recurse) {
            Ok(a) => {
                let stop = a.rcode != rcode::NOERROR;
                answers.push(a);
                if stop {
                    break;
                }
            }
            Err(e) => {
                crate::common::complain(
                    cx,
                    NAME,
                    &format!("No name server could be reached: {}", e.errno.text()),
                );
                return 1;
            }
        }
    }
    let first = &answers[0];
    match &st.server {
        Some((label, addr)) => header(out, label, addr, st.port),
        None => header(out, &first.server, &first.server, st.port),
    }
    if let Some(bad) = answers.iter().find(|a| a.rcode != rcode::NOERROR) {
        out.text(&format!(
            "** server can't find {qname}: {}\n\n",
            rcode::name(bad.rcode)
        ));
        return 1;
    }
    if !first.authoritative {
        out.text("Non-authoritative answer:\n");
    }
    let mut seen: Vec<&DnsRecord> = Vec::new();
    for a in &answers {
        for r in &a.answers {
            if seen.contains(&r) {
                continue;
            }
            seen.push(r);
            out.text(&record_lines(r));
        }
    }
    if seen.is_empty() {
        out.text(&format!("*** Can't find {qname}: No answer\n"));
    }
    out.text("\n");
    0
}

fn show_all(out: &mut Out, st: &Settings, default: &str) {
    let (label, addr) = st
        .server
        .clone()
        .unwrap_or_else(|| (default.to_string(), default.to_string()));
    out.text(&format!("Default server: {label}\nAddress: {addr}#{}\n\n", st.port));
    out.text("Set options:\n");
    out.text(&format!(
        "  querytype = {}\n",
        st.qtype.map_or_else(|| "A".to_string(), rtype::name)
    ));
    out.text(&format!("  port = {}\n", st.port));
    out.text(if st.recurse { "  recurse\n" } else { "  norecurse\n" });
}

fn default_server(cx: &UtilCtx<'_>) -> String {
    ask(cx, ".", rtype::NS, None, true)
        .map(|a| a.server)
        .unwrap_or_default()
}

fn interactive(cx: &UtilCtx<'_>, out: &mut Out, st: &mut Settings) -> i32 {
    let mut lines = Lines::new(stdin(cx));
    loop {
        out.text("> ");
        out.flush();
        let Ok(Some((line, _))) = lines.next_text() else {
            out.text("\n");
            return 0;
        };
        let line = String::from_utf8_lossy(&line).into_owned();
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            [] => {}
            ["exit"] => return 0,
            ["server" | "lserver", s] => match use_server(cx, st, s) {
                Ok(()) => {
                    let (label, addr) = st.server.clone().unwrap_or_default();
                    out.text(&format!("Default server: {label}\nAddress: {addr}#{}\n\n", st.port));
                }
                Err(msg) => {
                    out.text(&format!("*** {msg}\n"));
                }
            },
            ["set", "all"] => show_all(out, st, &default_server(cx)),
            ["set", opt] => {
                if let Err(msg) = set_option(st, opt) {
                    out.text(&format!("*** {msg}\n"));
                }
            }
            [name] => {
                lookup(cx, out, st, name);
            }
            [name, server] => {
                let mut once = st.clone();
                match use_server(cx, &mut once, server) {
                    Ok(()) => {
                        lookup(cx, out, &once, name);
                    }
                    Err(msg) => {
                        out.text(&format!("*** {msg}\n"));
                    }
                }
            }
            _ => {
                out.text("*** The command is not valid.\n");
            }
        }
    }
}
