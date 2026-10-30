// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `host [-46aCdlrTv] [-c class] [-N n] [-R n] [-t type] [-W n] name
//! [server]`: one line a record (`x has address a`, `x mail is handled
//! by 10 m.`); an address is looked up by its reverse name. Without
//! `-t` a name is asked for its A, AAAA and MX records. `-a`/`-v` show
//! the sections as `dig` does.

use qsh::sys::services::{rcode, rtype};
use qsh::sys::{DnsAnswer, DnsRecord, UtilCtx};

use super::dig::sections;
use super::dns::{ask, ip, parse_type, reverse};
use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

const NAME: &str = "host";

/// The line `host` prints for a record.
pub fn line(r: &DnsRecord) -> String {
    let n = &r.name;
    let d = &r.data;
    match r.rtype {
        rtype::A => format!("{n} has address {d}"),
        rtype::AAAA => format!("{n} has IPv6 address {d}"),
        rtype::CNAME => format!("{n} is an alias for {d}"),
        rtype::MX => format!("{n} mail is handled by {d}"),
        rtype::NS => format!("{n} name server {d}"),
        rtype::PTR => format!("{n} domain name pointer {d}"),
        rtype::TXT => format!("{n} descriptive text {d}"),
        t => format!("{n} has {} record {d}", rtype::name(t)),
    }
}

/// Run `host`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "46aCdlrTvwc:N:R:t:W:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    let verbose = p.has('a') || p.has('v') || p.has('d');
    let mut qtype = if p.has('a') { Some(rtype::ANY) } else { None };
    if let Some(t) = p.value('t') {
        match parse_type(t) {
            Some(t) => qtype = Some(t),
            None => return usage(cx, NAME, &format!("The record type {t} is not known.")),
        }
    }
    if let Some(c) = p.value('c')
        && !c.eq_ignore_ascii_case("IN")
    {
        return usage(cx, NAME, &format!("The class {c} is not supported."));
    }
    let (name, server) = match p.operands.as_slice() {
        [n] => (n.clone(), None),
        [n, s] => (n.clone(), Some(s.clone())),
        _ => return usage(cx, NAME, "A name and at most one server are taken."),
    };
    let recurse = !p.has('r');
    let mut out = Out::stdout(cx);
    let (qname, types) = match (qtype, ip(&name)) {
        (None, Some(a)) => (reverse(a), vec![rtype::PTR]),
        (Some(rtype::PTR), Some(a)) => (reverse(a), vec![rtype::PTR]),
        (Some(t), _) => (name.clone(), vec![t]),
        (None, None) => (name.clone(), vec![rtype::A, rtype::AAAA, rtype::MX]),
    };
    if let Some(s) = &server {
        out.text(&format!(
            "Using domain server:\nName: {s}\nAddress: {s}#53\nAliases: \n\n"
        ));
    }
    let mut answers: Vec<DnsAnswer> = Vec::new();
    for t in &types {
        match ask(cx, &qname, *t, server.as_deref(), recurse) {
            Ok(a) => {
                let stop = a.rcode != rcode::NOERROR;
                answers.push(a);
                if stop {
                    break;
                }
            }
            Err(e) => {
                out.finish(cx, NAME, 0);
                complain(
                    cx,
                    NAME,
                    &format!("No name server could be reached: {}", e.errno.text()),
                );
                return 1;
            }
        }
    }
    if let Some(bad) = answers.iter().find(|a| a.rcode != rcode::NOERROR) {
        out.text(&format!(
            "Host {qname} not found: {}({})\n",
            bad.rcode,
            rcode::name(bad.rcode)
        ));
        return out.finish(cx, NAME, 1);
    }
    if verbose {
        for (a, t) in answers.iter().zip(&types) {
            out.text(&format!("Trying \"{qname}\"\n"));
            out.text(&sections(a, &qname, *t, &super::dig::Show::all()));
        }
        return out.finish(cx, NAME, 0);
    }
    let mut seen: Vec<&DnsRecord> = Vec::new();
    for a in &answers {
        for r in &a.answers {
            if !seen.contains(&r) {
                seen.push(r);
                out.text(&line(r));
                out.text("\n");
            }
        }
    }
    if seen.is_empty() && qtype.is_some() {
        let t = rtype::name(types[0]);
        out.text(&format!("{} has no {t} record\n", answers[0].qname));
    }
    out.finish(cx, NAME, 0)
}
