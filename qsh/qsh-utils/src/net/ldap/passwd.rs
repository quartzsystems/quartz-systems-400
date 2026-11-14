// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ldapchangepwd -D binddn -w oldpassword -n newpassword [options]`:
//! binds as the entry with its password and replaces its
//! `userPassword` with the new one.

use ldap::msg::{Attribute, ModOp, Modification};
use qsh::sys::UtilCtx;

use super::{CONNECT, Tool, close, connect, controls, parse, status_of};

const NAME: &str = "ldapchangepwd";

const USAGE: &str = "\
usage: ldapchangepwd -D binddn -w password|? -n newpassword [options]
connection: -h host -p port -V 2|3 -Z (LDAPS) -Y (StartTLS) -K keydb -P password
  -N label -m EXTERNAL -y proxydn -M -R -O hops -C charset -v
";

/// Run `ldapchangepwd`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let t = Tool { cx, name: NAME };
    // Here -n is the new password.
    let mut spec = vec!["n:"];
    spec.extend(CONNECT.iter().filter(|s| **s != "n"));
    let a = match parse(&t.cx.argv[1..], &spec) {
        Ok(a) => a,
        Err(e) => return t.usage(&e, USAGE),
    };
    let (Some(dn), Some(new)) = (a.value("D"), a.value("n")) else {
        return t.usage(
            "A bind DN (-D), its password (-w) and a new password (-n) are needed.",
            USAGE,
        );
    };
    if !a.has("w") && a.value("m").is_none() {
        return t.usage("The entry's password (-w) is needed.", USAGE);
    }
    let (dn, new) = (dn.to_string(), new.to_string());
    let mut c = match connect(&t, &a) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let change = Modification {
        op: ModOp::Replace,
        attr: Attribute {
            ty: "userPassword".into(),
            vals: vec![new.into_bytes()],
        },
    };
    t.verbose(&a, &format!("changing the password of {dn}"));
    let status = match c.modify(&dn, vec![change], &controls(&a)) {
        Ok(r) if r.result.is_success() => {
            t.say(&format!("{NAME}: the password of {dn} was changed\n"));
            0
        }
        Ok(r) => t.fail(super::code(&r.result), &super::text(&r.result)),
        Err(e) => t.fail(status_of(&e), &e.to_string()),
    };
    close(c);
    status
}
