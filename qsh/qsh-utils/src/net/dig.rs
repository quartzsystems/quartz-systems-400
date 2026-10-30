// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `dig [@server] [-t type] [-x addr] [-q name] [-p port] [name]
//! [type] [class] [+[no]option ...]`: a question and the answer's
//! sections in master-file form. `+short` prints the data alone;
//! `+noall +answer` the answer lines alone. Status 0 for any answer
//! (`NXDOMAIN` too), 1 for usage, 9 when no server answered.

use qsh::sys::services::{rcode, rtype};
use qsh::sys::{DnsAnswer, UtilCtx};

use super::dns::{absolute, ask, ip, master_line, parse_type, reverse};
use crate::common::io::Out;
use crate::common::time::{Zone, strftime};
use crate::common::usage;

const NAME: &str = "dig";

/// Which parts of an answer to show.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Show {
    /// The `;;` comment lines (header, section titles).
    pub comments: bool,
    /// The question section.
    pub question: bool,
    /// The answer section.
    pub answer: bool,
    /// The authority section.
    pub authority: bool,
    /// The additional section.
    pub additional: bool,
}

impl Show {
    /// Everything.
    pub fn all() -> Self {
        Self {
            comments: true,
            question: true,
            answer: true,
            authority: true,
            additional: true,
        }
    }
}

/// The header and sections of an answer.
pub fn sections(a: &DnsAnswer, qname: &str, qtype: u16, show: &Show) -> String {
    let mut s = String::new();
    if show.comments {
        let mut flags = vec!["qr"];
        if a.authoritative {
            flags.push("aa");
        }
        flags.push("rd");
        flags.push("ra");
        s.push_str(&format!(
            ";; ->>HEADER<<- opcode: QUERY, status: {}, id: 1\n",
            rcode::name(a.rcode)
        ));
        s.push_str(&format!(
            ";; flags: {}; QUERY: 1, ANSWER: {}, AUTHORITY: {}, ADDITIONAL: {}\n\n",
            flags.join(" "),
            a.answers.len(),
            a.authority.len(),
            a.additional.len()
        ));
    }
    if show.question {
        if show.comments {
            s.push_str(";; QUESTION SECTION:\n");
        }
        let q = if a.qname.is_empty() { qname } else { &a.qname };
        s.push_str(&format!(";{}\t\tIN\t{}\n", absolute(q), rtype::name(qtype)));
        if show.comments {
            s.push('\n');
        }
    }
    for (on, title, rs) in [
        (show.answer, "ANSWER", &a.answers),
        (show.authority, "AUTHORITY", &a.authority),
        (show.additional, "ADDITIONAL", &a.additional),
    ] {
        if !on || rs.is_empty() {
            continue;
        }
        if show.comments {
            s.push_str(&format!(";; {title} SECTION:\n"));
        }
        for r in rs {
            s.push_str(&master_line(r));
            s.push('\n');
        }
        if show.comments {
            s.push('\n');
        }
    }
    s
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct Flags {
    show: Show,
    short: bool,
    cmd: bool,
    stats: bool,
    recurse: bool,
}

fn plus(f: &mut Flags, opt: &str) -> bool {
    let (on, word) = match opt.strip_prefix("no") {
        Some(w) => (false, w),
        None => (true, opt),
    };
    let word = word.split('=').next().unwrap_or_default();
    match word {
        "short" => f.short = on,
        "cmd" => f.cmd = on,
        "comments" => f.show.comments = on,
        "question" => f.show.question = on,
        "answer" => f.show.answer = on,
        "authority" => f.show.authority = on,
        "additional" => f.show.additional = on,
        "stats" => f.stats = on,
        "recurse" => f.recurse = on,
        "all" => {
            f.show = Show {
                comments: on,
                question: on,
                answer: on,
                authority: on,
                additional: on,
            };
            f.cmd = on;
            f.stats = on;
        }
        "search" | "defname" | "tcp" | "vc" | "ignore" | "fail" | "besteffort" | "dnssec"
        | "time" | "tries" | "retry" | "ndots" | "bufsize" | "edns" | "nsid" | "ttlid"
        | "class" | "multiline" | "identify" => {}
        _ => return false,
    }
    true
}

/// Run `dig`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let mut f = Flags {
        show: Show::all(),
        short: false,
        cmd: true,
        stats: true,
        recurse: true,
    };
    let mut server: Option<String> = None;
    let mut name: Option<String> = None;
    let mut qtype: Option<u16> = None;
    let args: Vec<String> = cx.argv[1..].to_vec();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        i += 1;
        if let Some(s) = a.strip_prefix('@') {
            server = Some(s.to_string());
        } else if let Some(o) = a.strip_prefix('+') {
            if !plus(&mut f, o) {
                return usage(cx, NAME, &format!("The option +{o} is not valid."));
            }
        } else if a.len() > 1 && a.starts_with('-') {
            let flag = &a[1..2];
            let attached = &a[2..];
            if matches!(flag, "4" | "6") && attached.is_empty() {
                continue;
            }
            if flag == "h" {
                let mut out = Out::stdout(cx);
                out.text(
                    "Usage: dig [@server] [-t type] [-x address] [-q name] [-p port] [name] [type] [+[no]option ...]\n",
                );
                return out.finish(cx, NAME, 0);
            }
            if !matches!(flag, "t" | "x" | "q" | "p" | "c" | "b") {
                return usage(cx, NAME, &format!("The option {a} is not valid."));
            }
            let v = if attached.is_empty() {
                let Some(v) = args.get(i) else {
                    return usage(cx, NAME, &format!("Option {a} needs an argument."));
                };
                i += 1;
                v.clone()
            } else {
                attached.to_string()
            };
            match flag {
                "t" => match parse_type(&v) {
                    Some(t) => qtype = Some(t),
                    None => return usage(cx, NAME, &format!("The record type {v} is not known.")),
                },
                "x" => match ip(&v) {
                    Some(addr) => {
                        name = Some(reverse(addr));
                        qtype = Some(rtype::PTR);
                    }
                    None => return usage(cx, NAME, &format!("{v} is not an address.")),
                },
                "q" => name = Some(v),
                "c" if !v.eq_ignore_ascii_case("IN") => {
                    return usage(cx, NAME, &format!("The class {v} is not supported."));
                }
                _ => {}
            }
        } else if a.eq_ignore_ascii_case("IN") {
        } else if let Some(t) = parse_type(a).filter(|_| qtype.is_none() || name.is_some()) {
            qtype = Some(t);
        } else {
            name = Some(a.clone());
        }
    }
    let (name, qtype) = match name {
        Some(n) => (n, qtype.unwrap_or(rtype::A)),
        None => (".".to_string(), qtype.unwrap_or(rtype::NS)),
    };
    let mut out = Out::stdout(cx);
    if f.cmd && !f.short {
        out.text(&format!(
            "\n; <<>> dig (QS/400 {}) <<>> {}\n;; global options: +cmd\n",
            release::RELEASE,
            cx.argv[1..].join(" ")
        ));
    }
    let a = match ask(cx, &name, qtype, server.as_deref(), f.recurse) {
        Ok(a) => a,
        Err(e) => {
            out.text(&format!(
                ";; no servers could be reached: {}\n",
                e.errno.text()
            ));
            return out.finish(cx, NAME, 9);
        }
    };
    if f.short {
        for r in &a.answers {
            out.text(&r.data);
            out.text("\n");
        }
        return out.finish(cx, NAME, 0);
    }
    if f.show.comments {
        out.text(";; Got answer:\n");
    }
    out.text(&sections(&a, &name, qtype, &f.show));
    if f.stats {
        let now = i64::try_from(cx.sys.now().as_secs()).unwrap_or(0);
        let when = strftime("%a %b %e %H:%M:%S %Z %Y", &Zone::of(cx).local(now));
        out.text(&format!(
            ";; Query time: 0 msec\n;; SERVER: {}#{}({})\n;; WHEN: {when}\n\n",
            a.server, a.port, a.server
        ));
    }
    out.finish(cx, NAME, 0)
}
