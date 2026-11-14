// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ldapdelete [options] [DN...]`: deletes each entry named, or one per
//! line of `-f file` (standard input when neither). `-s` deletes the
//! subtree (the tree delete control), `-c` goes on after an error.

use ldap::control::{ControlValue, TreeDelete};
use qsh::sys::UtilCtx;

use super::{CONNECT, Tool, close, connect, controls, input, parse, status_of};

const NAME: &str = "ldapdelete";

const USAGE: &str = "\
usage: ldapdelete [options] [DN...]
  -s          delete the entry's subtree too
  -c          go on after an error
  -f file     a DN per line (else the operands, else standard input)
  -n          show what would be done; -v verbose
connection: -h host -p port -D dn -w password|? -V 2|3 -Z (LDAPS) -Y (StartTLS)
  -K keydb -P password -N label -m EXTERNAL -y proxydn -M -R -O hops -C charset
";

fn spec() -> Vec<&'static str> {
    let mut s = CONNECT.to_vec();
    s.extend(["c", "f:", "i:", "l", "s"]);
    s
}

/// Run `ldapdelete`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let t = Tool { cx, name: NAME };
    let a = match parse(&t.cx.argv[1..], &spec()) {
        Ok(a) => a,
        Err(e) => return t.usage(&e, USAGE),
    };
    if a.has("l") {
        return t.fail(
            super::REFUSED,
            "-l: replication controls are not supported.",
        );
    }
    let dns: Vec<String> = if a.operands.is_empty() {
        match input(&t, &a) {
            Ok(b) => String::from_utf8_lossy(&b)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect(),
            Err(e) => return e,
        }
    } else {
        a.operands.clone()
    };
    if a.has("n") {
        for d in &dns {
            t.say(&format!("would delete {d}\n"));
        }
        return 0;
    }
    let mut c = match connect(&t, &a) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let mut ctrls = controls(&a);
    if a.has("s") {
        ctrls.push(TreeDelete.to_control(true));
    }
    let mut status = 0;
    for d in &dns {
        t.verbose(&a, &format!("deleting {d}"));
        let s = match c.delete(d, &ctrls) {
            Ok(r) if r.result.is_success() => 0,
            Ok(r) => t.fail(
                super::code(&r.result),
                &format!("{d}: {}", super::text(&r.result)),
            ),
            Err(e) => t.fail(status_of(&e), &format!("{d}: {e}")),
        };
        if s != 0 {
            status = s;
            if !a.has("c") {
                break;
            }
        }
    }
    close(c);
    status
}
