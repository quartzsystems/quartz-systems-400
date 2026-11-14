// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ldapexop [options] -op operation [operation options]`: the extended
//! operations QS/400 builds: `whoami` (RFC 4532) and `passwd`, the
//! password modify operation (RFC 3062: `-a` the old password, `-s` the
//! new one, else the server makes one, and an optional user operand).
//! IBM's operations (`acctstatus`, `onlineBackup`, the replication and
//! log ones, ...) are refused with a message.

use ldap::ext::PasswordModifyRequest;
use qsh::sys::UtilCtx;

use super::{CONNECT, REFUSED, Tool, close, connect, parse, status_of};

const NAME: &str = "ldapexop";

const USAGE: &str = "\
usage: ldapexop [options] -op operation [operation options]
  -op whoami                     the identity the server knows the session by
  -op passwd [-a old] [-s new] [user]
                                 change a password (the server makes one without -s)
connection: -h host -p port -D dn -w password|? -V 2|3 -Z (LDAPS) -Y (StartTLS)
  -K keydb -P password -N label -m EXTERNAL -y proxydn -M -R -O hops -C charset
";

/// IBM's operation names, none of which QS/400 builds.
pub const IBM_OPS: &[&str] = &[
    "acctstatus",
    "cascrepl",
    "clearlog",
    "controlqueue",
    "controlrepl",
    "controlreplerr",
    "effectpwdpolicy",
    "evaluategroups",
    "getattributes",
    "getlogsize",
    "getusertype",
    "locateEntry",
    "onlineBackup",
    "quiesce",
    "readconfig",
    "readlog",
    "repltopology",
    "resumerole",
    "stopserver",
    "unbind",
    "uniqueattr",
];

/// Run `ldapexop`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let t = Tool { cx, name: NAME };
    let argv = t.cx.argv[1..].to_vec();
    let Some(at) = argv.iter().position(|a| a == "-op") else {
        return t.usage("An operation (-op) is needed.", USAGE);
    };
    let a = match parse(&argv[..at], CONNECT) {
        Ok(a) => a,
        Err(e) => return t.usage(&e, USAGE),
    };
    if !a.operands.is_empty() {
        return t.usage("General options come before -op.", USAGE);
    }
    let Some(op) = argv.get(at + 1) else {
        return t.usage("-op needs an operation.", USAGE);
    };
    let rest = &argv[at + 2..];
    match op.as_str() {
        "whoami" => {
            if !rest.is_empty() {
                return t.usage("whoami takes no options.", USAGE);
            }
            let mut c = match connect(&t, &a) {
                Ok(c) => c,
                Err(e) => return e,
            };
            let s = match c.who_am_i() {
                Ok(id) if id.is_empty() => {
                    t.say("anonymous\n");
                    0
                }
                Ok(id) => {
                    t.say(&format!("{id}\n"));
                    0
                }
                Err(e) => t.fail(status_of(&e), &format!("whoami: {e}")),
            };
            close(c);
            s
        }
        "passwd" => {
            let o = match parse(rest, &["a:", "s:"]) {
                Ok(o) => o,
                Err(e) => return t.usage(&e, USAGE),
            };
            if o.operands.len() > 1 {
                return t.usage("passwd takes at most one user.", USAGE);
            }
            let req = PasswordModifyRequest {
                user: o.operands.first().cloned(),
                old: o.value("a").map(|v| v.as_bytes().to_vec()),
                new: o.value("s").map(|v| v.as_bytes().to_vec()),
            };
            let mut c = match connect(&t, &a) {
                Ok(c) => c,
                Err(e) => return e,
            };
            let s = match c.password_modify(&req) {
                Ok(r) => {
                    if let Some(g) = r.generated {
                        t.say(&format!("New password: {}\n", String::from_utf8_lossy(&g)));
                    }
                    0
                }
                Err(e) => t.fail(status_of(&e), &format!("passwd: {e}")),
            };
            close(c);
            s
        }
        other if IBM_OPS.iter().any(|o| o.eq_ignore_ascii_case(other)) => t.fail(
            REFUSED,
            &format!("The operation {other} is not supported; whoami and passwd are."),
        ),
        other => t.usage(&format!("The operation {other} is not known."), USAGE),
    }
}
