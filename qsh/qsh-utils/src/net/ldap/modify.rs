// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ldapmodify [options] [-f file]` and `ldapadd` (`ldapmodify -a`):
//! LDIF records from the file or standard input, each sent as the add,
//! delete, modify or rename it says. A record without a change type is
//! an add with `-a`, else a modify that adds its values (`-r` replaces
//! them). `-c` goes on after an error, `-e file` keeps the records that
//! failed, `-b` reads a value starting with `/` from that file, `-n`
//! shows what would be done.

use ldap::control::Control;
use ldap::ldif::{self, Body, Record, Value};
use ldap::msg::{Attribute, ModOp, Modification};
use qsh::sys::UtilCtx;

use super::{
    Args, CONNECT, PARAM, REFUSED, Tool, close, connect, controls, input, parse, status_of,
};
use crate::common::io::{open_write, read_input};

const USAGE: &str = "\
usage: ldapmodify [options] [-f file]; ldapadd [options] [-f file]
  -a          a record without a change type adds an entry
  -r          a record without a change type replaces values (else adds)
  -c          go on after an error; -e file writes the records that failed
  -b          a value starting with / is read from that file
  -f file     the LDIF (else standard input); -i file the same
  -n          show what would be done; -v verbose
connection: -h host -p port -D dn -w password|? -V 2|3 -Z (LDAPS) -Y (StartTLS)
  -K keydb -P password -N label -m EXTERNAL -y proxydn -M -R -O hops -C charset
";

fn spec() -> Vec<&'static str> {
    let mut s = CONNECT.to_vec();
    s.extend([
        "a", "b", "B", "c", "e:", "f:", "g", "i:", "j", "l", "r", "t",
    ]);
    s
}

/// Run `ldapmodify`.
pub fn run_modify(cx: &mut UtilCtx<'_>) -> i32 {
    run(cx, "ldapmodify", false)
}

/// Run `ldapadd`.
pub fn run_add(cx: &mut UtilCtx<'_>) -> i32 {
    run(cx, "ldapadd", true)
}

/// A value: its bytes, a `file://` URL read, or (`-b`) a file named.
fn value(t: &Tool<'_, '_>, a: &Args, v: &Value) -> Result<Vec<u8>, String> {
    match v {
        Value::Data(d) => {
            if a.has("b") && d.first() == Some(&b'/') {
                let p = String::from_utf8_lossy(d).into_owned();
                return read_input(t.cx, &p).map_err(|e| format!("{p}: {}", e.errno.text()));
            }
            Ok(d.clone())
        }
        Value::Url(u) => {
            let p = u
                .strip_prefix("file://")
                .ok_or_else(|| format!("the URL {u} is not a file URL"))?;
            read_input(t.cx, p).map_err(|e| format!("{p}: {}", e.errno.text()))
        }
    }
}

fn attrs(t: &Tool<'_, '_>, a: &Args, lines: &[ldif::AttrValue]) -> Result<Vec<Attribute>, String> {
    ldif::group(lines, |u| value(t, a, &Value::Url(u.to_string()))).and_then(|mut v| {
        // `-b` applies to inline values too.
        if a.has("b") {
            for at in &mut v {
                for x in &mut at.vals {
                    *x = value(t, a, &Value::Data(x.clone()))?;
                }
            }
        }
        Ok(v)
    })
}

/// Sends one record; `Err` with the status and text when it fails.
fn apply(
    t: &Tool<'_, '_>,
    a: &Args,
    c: &mut ldap::client::Client,
    r: &Record,
    add: bool,
) -> Result<(), (i32, String)> {
    let mut ctrls: Vec<Control> = controls(a);
    for lc in &r.controls {
        let v = match &lc.value {
            Some(v) => Some(value(t, a, v).map_err(|e| (PARAM, e))?),
            None => None,
        };
        ctrls.push(Control::new(&lc.oid, lc.critical, v));
    }
    let bad = |e: String| (PARAM, e);
    let (what, res) = match &r.body {
        Body::Content(lines) if !add => {
            let op = if a.has("r") {
                ModOp::Replace
            } else {
                ModOp::Add
            };
            let changes = attrs(t, a, lines)
                .map_err(bad)?
                .into_iter()
                .map(|attr| Modification { op, attr })
                .collect();
            ("modifying entry", n(a, c.modify(&r.dn, changes, &ctrls)))
        }
        Body::Content(lines) | Body::Add(lines) => {
            let at = attrs(t, a, lines).map_err(bad)?;
            ("adding entry", n(a, c.add(&r.dn, at, &ctrls)))
        }
        Body::Delete => ("deleting entry", n(a, c.delete(&r.dn, &ctrls))),
        Body::Modify(specs) => {
            let mut changes = Vec::new();
            for s in specs {
                let mut vals = Vec::new();
                for v in &s.values {
                    vals.push(value(t, a, v).map_err(bad)?);
                }
                changes.push(Modification {
                    op: s.op,
                    attr: Attribute {
                        ty: s.ty.clone(),
                        vals,
                    },
                });
            }
            ("modifying entry", n(a, c.modify(&r.dn, changes, &ctrls)))
        }
        Body::ModDn {
            new_rdn,
            delete_old_rdn,
            new_superior,
        } => (
            "renaming entry",
            n(
                a,
                c.modify_dn(
                    &r.dn,
                    new_rdn,
                    *delete_old_rdn,
                    new_superior.as_deref(),
                    &ctrls,
                ),
            ),
        ),
    };
    t.say(&format!("{what} {}\n", r.dn));
    match res {
        None => Ok(()),
        Some(Ok(resp)) if resp.result.is_success() => Ok(()),
        Some(Ok(resp)) => Err((super::code(&resp.result), super::text(&resp.result))),
        Some(Err(e)) => Err((status_of(&e), e.to_string())),
    }
}

/// `-n` does nothing: no result.
fn n<T>(a: &Args, r: ldap::client::Result<T>) -> Option<ldap::client::Result<T>> {
    (!a.has("n")).then_some(r)
}

fn run(cx: &mut UtilCtx<'_>, name: &'static str, add: bool) -> i32 {
    let t = Tool { cx, name };
    let a = match parse(&t.cx.argv[1..], &spec()) {
        Ok(a) => a,
        Err(e) => return t.usage(&e, USAGE),
    };
    for (o, why) in [
        ("t", "transactions are not supported"),
        ("B", "it is not supported"),
        ("g", "it is not supported"),
        ("j", "it is not supported"),
        ("l", "replication controls are not supported"),
    ] {
        if a.has(o) {
            return t.fail(REFUSED, &format!("-{o}: {why}."));
        }
    }
    if !a.operands.is_empty() {
        return t.usage("Operands are not taken; give the LDIF with -f.", USAGE);
    }
    let b = match input(&t, &a) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let file = match ldif::parse(&b) {
        Ok(f) => f,
        Err(e) => return t.fail(PARAM, &e.to_string()),
    };
    let add = add || a.has("a");
    if a.has("n") {
        t.say(&format!("{name}: showing what would be done\n"));
    }
    let mut c = if a.has("n") {
        None
    } else {
        match connect(&t, &a) {
            Ok(c) => Some(c),
            Err(e) => return e,
        }
    };
    let mut failed = Vec::new();
    let mut status = 0;
    for r in &file.records {
        let res = match &mut c {
            Some(c) => apply(&t, &a, c, r, add),
            None => {
                t.say(&format!("would send {} for {}\n", kind(r, add), r.dn));
                Ok(())
            }
        };
        if let Err((s, e)) = res {
            t.fail(s, &format!("{} (line {}): {e}", r.dn, r.line));
            status = s;
            failed.push(r.clone());
            if !a.has("c") {
                break;
            }
        }
    }
    if let Some(f) = a.value("e")
        && !failed.is_empty()
    {
        let text = ldif::write(&ldif::Ldif {
            version: Some(1),
            charset: None,
            records: failed,
        });
        match open_write(t.cx, f, false) {
            Ok(fd) => {
                let _ = qsh::sys::stream::write_all(&*fd, text.as_bytes());
            }
            Err(e) => {
                t.fail(PARAM, &format!("{f}: {}", e.errno.text()));
            }
        }
    }
    if let Some(c) = c {
        close(c);
    }
    status
}

fn kind(r: &Record, add: bool) -> &'static str {
    match &r.body {
        Body::Content(_) if !add => "a modify",
        Body::Content(_) | Body::Add(_) => "an add",
        Body::Delete => "a delete",
        Body::Modify(_) => "a modify",
        Body::ModDn { .. } => "a rename",
    }
}
