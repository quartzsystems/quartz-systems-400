// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ipcs [-qms] [-abcopt]`: the System V message queues, shared memory
//! segments and semaphore sets, a section each with a heading line of
//! the columns asked for (`-b` sizes, `-c` creators, `-o` use, `-p`
//! process IDs, `-t` times, `-a` all of them). `ipcrm [-q id] [-m id]
//! [-s id] [-Q key] [-M key] [-S key] ...` removes them.

use qsh::sys::{IpcInfo, IpcKind, IpcTarget, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::time::{Zone, strftime};
use crate::common::{complain, usage};

/// The mode column: two status characters, then the permissions
/// (`a` for a semaphore's alter permission).
pub fn mode(kind: IpcKind, m: u32) -> String {
    let w = if kind == IpcKind::Semaphore { 'a' } else { 'w' };
    let mut s = String::from("--");
    for shift in [6, 3, 0] {
        let b = (m >> shift) & 7;
        s.push(if b & 4 != 0 { 'r' } else { '-' });
        s.push(if b & 2 != 0 { w } else { '-' });
        s.push('-');
    }
    s
}

#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
struct Cols {
    sizes: bool,
    creator: bool,
    usage: bool,
    pids: bool,
    times: bool,
}

fn name_of(n: Option<String>, id: u32) -> String {
    n.unwrap_or_else(|| id.to_string())
}

fn heading(kind: IpcKind, c: Cols) -> String {
    let mut h = format!(
        "{:<1}{:>10} {:<10} {:<11} {:>8} {:>8}",
        "T", "ID", "KEY", "MODE", "OWNER", "GROUP"
    );
    if c.creator {
        h.push_str(&format!(" {:>8} {:>8}", "CREATOR", "CGROUP"));
    }
    match kind {
        IpcKind::Queue => {
            if c.usage {
                h.push_str(&format!(" {:>8} {:>6}", "CBYTES", "QNUM"));
            }
            if c.sizes {
                h.push_str(&format!(" {:>8}", "QBYTES"));
            }
            if c.pids {
                h.push_str(&format!(" {:>6} {:>6}", "LSPID", "LRPID"));
            }
            if c.times {
                h.push_str(&format!(" {:>9} {:>9} {:>9}", "STIME", "RTIME", "CTIME"));
            }
        }
        IpcKind::Memory => {
            if c.usage {
                h.push_str(&format!(" {:>6}", "NATTCH"));
            }
            if c.sizes {
                h.push_str(&format!(" {:>8}", "SEGSZ"));
            }
            if c.pids {
                h.push_str(&format!(" {:>6} {:>6}", "CPID", "LPID"));
            }
            if c.times {
                h.push_str(&format!(" {:>9} {:>9} {:>9}", "ATIME", "DTIME", "CTIME"));
            }
        }
        IpcKind::Semaphore => {
            if c.sizes {
                h.push_str(&format!(" {:>6}", "NSEMS"));
            }
            if c.times {
                h.push_str(&format!(" {:>9} {:>9}", "OTIME", "CTIME"));
            }
        }
    }
    h
}

fn time(zone: &Zone, t: i64) -> String {
    if t == 0 {
        "no-entry".to_string()
    } else {
        strftime("%H:%M:%S", &zone.local(t))
    }
}

fn row(cx: &UtilCtx<'_>, zone: &Zone, i: &IpcInfo, c: Cols) -> String {
    let t = match i.kind {
        IpcKind::Queue => "q",
        IpcKind::Memory => "m",
        IpcKind::Semaphore => "s",
    };
    let mut r = format!(
        "{t:<1}{:>10} {:<10} {:<11} {:>8} {:>8}",
        i.id,
        format!("0x{:08x}", i.key),
        mode(i.kind, i.mode),
        name_of(cx.sys.user_name(i.uid), i.uid),
        name_of(cx.sys.group_name(i.gid), i.gid)
    );
    if c.creator {
        r.push_str(&format!(
            " {:>8} {:>8}",
            name_of(cx.sys.user_name(i.cuid), i.cuid),
            name_of(cx.sys.group_name(i.cgid), i.cgid)
        ));
    }
    let [t0, t1, t2] = i.times;
    match i.kind {
        IpcKind::Queue => {
            if c.usage {
                r.push_str(&format!(" {:>8} {:>6}", i.used, i.count));
            }
            if c.sizes {
                r.push_str(&format!(" {:>8}", i.size));
            }
            if c.pids {
                r.push_str(&format!(" {:>6} {:>6}", i.pids.0, i.pids.1));
            }
            if c.times {
                r.push_str(&format!(
                    " {:>9} {:>9} {:>9}",
                    time(zone, t0),
                    time(zone, t1),
                    time(zone, t2)
                ));
            }
        }
        IpcKind::Memory => {
            if c.usage {
                r.push_str(&format!(" {:>6}", i.used));
            }
            if c.sizes {
                r.push_str(&format!(" {:>8}", i.size));
            }
            if c.pids {
                r.push_str(&format!(" {:>6} {:>6}", i.pids.0, i.pids.1));
            }
            if c.times {
                r.push_str(&format!(
                    " {:>9} {:>9} {:>9}",
                    time(zone, t0),
                    time(zone, t1),
                    time(zone, t2)
                ));
            }
        }
        IpcKind::Semaphore => {
            if c.sizes {
                r.push_str(&format!(" {:>6}", i.size));
            }
            if c.times {
                r.push_str(&format!(" {:>9} {:>9}", time(zone, t0), time(zone, t2)));
            }
        }
    }
    r
}

/// Run `ipcs`.
pub fn run_ipcs(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "ipcs";
    let p = match getopt(&cx.argv[1..], "qmsabcopt") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    if !p.operands.is_empty() {
        return usage(cx, name, "No operands are taken.");
    }
    let a = p.has('a');
    let c = Cols {
        sizes: a || p.has('b'),
        creator: a || p.has('c'),
        usage: a || p.has('o'),
        pids: a || p.has('p'),
        times: a || p.has('t'),
    };
    let some = p.has('q') || p.has('m') || p.has('s');
    let list = match cx.sys.ipc_list() {
        Ok(l) => l,
        Err(e) => {
            complain(cx, name, &format!("The IPC objects cannot be listed: {}", e.errno.text()));
            return 1;
        }
    };
    let zone = Zone::of(cx);
    let now = i64::try_from(cx.sys.now().as_secs()).unwrap_or(0);
    let mut out = Out::stdout(cx);
    out.text(&format!(
        "IPC status from {} as of {}\n",
        cx.sys.nodename(),
        strftime("%a %b %e %H:%M:%S %Z %Y", &zone.local(now))
    ));
    for (kind, flag, title) in [
        (IpcKind::Queue, 'q', "Message Queues:"),
        (IpcKind::Memory, 'm', "Shared Memory:"),
        (IpcKind::Semaphore, 's', "Semaphores:"),
    ] {
        if some && !p.has(flag) {
            continue;
        }
        out.text(title);
        out.text("\n");
        out.text(&heading(kind, c));
        out.text("\n");
        for i in list.iter().filter(|i| i.kind == kind) {
            out.text(&row(cx, &zone, i, c));
            out.text("\n");
        }
    }
    out.finish(cx, name, 0)
}

/// An identifier or key: decimal, or hexadecimal after `0x`.
pub fn number(s: &str) -> Option<i32> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok().map(|n| n as i32),
        None => s.parse().ok(),
    }
}

/// Run `ipcrm`.
pub fn run_ipcrm(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "ipcrm";
    let p = match getopt(&cx.argv[1..], "q:m:s:Q:M:S:") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    if !p.operands.is_empty() || p.opts.is_empty() {
        return usage(cx, name, "Name objects with -q, -m, -s, -Q, -M or -S.");
    }
    let mut status = 0;
    for (o, v) in &p.opts {
        let v = v.as_deref().unwrap_or_default();
        let Some(n) = number(v) else {
            complain(cx, name, &format!("-{o} {v}: The number is not valid."));
            status = 1;
            continue;
        };
        let kind = match o.to_ascii_lowercase() {
            'q' => IpcKind::Queue,
            'm' => IpcKind::Memory,
            _ => IpcKind::Semaphore,
        };
        let target = if o.is_ascii_uppercase() {
            IpcTarget::Key(n)
        } else {
            IpcTarget::Id(n)
        };
        if let Err(e) = cx.sys.ipc_remove(kind, target) {
            let why = if e.errno == qsh::sys::Errno::EINVAL {
                if o.is_ascii_uppercase() {
                    "No object has the key."
                } else {
                    "The identifier is not valid."
                }
            } else {
                e.errno.text()
            };
            complain(cx, name, &format!("-{o} {v}: {why}"));
            status = 1;
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_and_numbers() {
        assert_eq!(mode(IpcKind::Queue, 0o640), "--rw-r-----");
        assert_eq!(mode(IpcKind::Semaphore, 0o666), "--ra-ra-ra-");
        assert_eq!(number("0x10"), Some(16));
        assert_eq!(number("12"), Some(12));
        assert_eq!(number("x"), None);
    }
}
