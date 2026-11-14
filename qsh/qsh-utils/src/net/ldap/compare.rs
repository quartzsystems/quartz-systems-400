// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ldapcompare [options] DN attribute=value`, or pairs of lines (a DN,
//! then `attribute=value`) from `-f file`: prints `compare TRUE` or
//! `compare FALSE` for each. The status is 0 when every comparison is
//! true, 5 (`compareFalse`) when one is false, else the error's.

use qsh::sys::UtilCtx;

use super::{CONNECT, Tool, close, connect, controls, input, parse, status_of};

const NAME: &str = "ldapcompare";

const USAGE: &str = "\
usage: ldapcompare [options] DN attribute=value; ldapcompare [options] -f file
  -c          go on after an error
  -f file     pairs of lines: a DN, then attribute=value
  -n          show what would be done; -v verbose
connection: -h host -p port -D dn -w password|? -V 2|3 -y proxydn -R
";

fn spec() -> Vec<&'static str> {
    let mut s = CONNECT.to_vec();
    s.extend(["c", "f:"]);
    s
}

/// Run `ldapcompare`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let t = Tool { cx, name: NAME };
    let a = match parse(&t.cx.argv[1..], &spec()) {
        Ok(a) => a,
        Err(e) => return t.usage(&e, USAGE),
    };
    let pairs: Vec<(String, String)> = match a.operands.as_slice() {
        [dn, ava] => vec![(dn.clone(), ava.clone())],
        [] if a.has("f") => {
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
                return t.usage("The file has a DN without attribute=value.", USAGE);
            }
            lines
                .chunks(2)
                .map(|p| (p[0].clone(), p[1].clone()))
                .collect()
        }
        _ => return t.usage("A DN and attribute=value are needed.", USAGE),
    };
    let mut asks = Vec::new();
    for (dn, ava) in pairs {
        let Some((attr, value)) = ava.split_once('=') else {
            return t.usage(&format!("{ava} is not attribute=value."), USAGE);
        };
        asks.push((dn, attr.to_string(), value.to_string()));
    }
    if a.has("n") {
        for (d, at, v) in &asks {
            t.say(&format!("would compare {at}={v} in {d}\n"));
        }
        return 0;
    }
    let mut c = match connect(&t, &a) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let ctrls = controls(&a);
    let mut status = 0;
    for (d, at, v) in &asks {
        let s = match c.compare(d, at, v.as_bytes(), &ctrls) {
            Ok(r) => match r.result.code.number() {
                6 => {
                    t.say("compare TRUE\n");
                    0
                }
                5 => {
                    t.say("compare FALSE\n");
                    5
                }
                _ => t.fail(
                    super::code(&r.result),
                    &format!("{d}: {}", super::text(&r.result)),
                ),
            },
            Err(e) => t.fail(status_of(&e), &format!("{d}: {e}")),
        };
        if s != 0 {
            status = s;
            if s != 5 && !a.has("c") {
                break;
            }
        }
    }
    close(c);
    status
}
