// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ldapmodrdn [options] DN newRDN`, or pairs of lines (a DN, then its
//! new RDN) from `-f file`: renames each entry; `-r` removes the old
//! RDN's values, `-c` goes on after an error.

use qsh::sys::UtilCtx;

use super::{CONNECT, Tool, close, connect, controls, input, parse, status_of};

const NAME: &str = "ldapmodrdn";

const USAGE: &str = "\
usage: ldapmodrdn [options] DN newRDN; ldapmodrdn [options] -f file
  -r          remove the old RDN's values
  -c          go on after an error
  -f file     pairs of lines: a DN, then its new RDN
  -n          show what would be done; -v verbose
connection: -h host -p port -D dn -w password|? -V 2|3 -Z (LDAPS) -Y (StartTLS)
  -K keydb -P password -N label -m EXTERNAL -y proxydn -M -R -O hops -C charset
";

fn spec() -> Vec<&'static str> {
    let mut s = CONNECT.to_vec();
    s.extend(["c", "f:", "i:", "l", "r"]);
    s
}

/// Run `ldapmodrdn`.
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
    let pairs: Vec<(String, String)> = match a.operands.as_slice() {
        [dn, rdn] => vec![(dn.clone(), rdn.clone())],
        [] => {
            let b = match input(&t, &a) {
                Ok(b) => b,
                Err(e) => return e,
            };
            let lines: Vec<String> = String::from_utf8_lossy(&b)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            if !lines.len().is_multiple_of(2) {
                return t.usage("The file has a DN without a new RDN.", USAGE);
            }
            lines
                .chunks(2)
                .map(|p| (p[0].clone(), p[1].clone()))
                .collect()
        }
        _ => return t.usage("A DN and a new RDN are needed.", USAGE),
    };
    if a.has("n") {
        for (d, r) in &pairs {
            t.say(&format!("would rename {d} to {r}\n"));
        }
        return 0;
    }
    let mut c = match connect(&t, &a) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let ctrls = controls(&a);
    let mut status = 0;
    for (d, r) in &pairs {
        t.verbose(&a, &format!("renaming {d} to {r}"));
        let s = match c.modify_dn(d, r, a.has("r"), None, &ctrls) {
            Ok(x) if x.result.is_success() => 0,
            Ok(x) => t.fail(
                super::code(&x.result),
                &format!("{d}: {}", super::text(&x.result)),
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
