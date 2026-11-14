// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 190's ILE program over `QGLDCLNT`, as `runtime/ldapclnt/tests/ile.rs`
//! calls it: `ldap_init`, `ldap_simple_bind_s`, `ldap_search_s`,
//! `ldap_count_entries`, `ldap_compare_s`, `ldap_msgfree`, `ldap_unbind`.
//! It says what it got with `DSPLY` lines: `LDAPC-BIND`, `LDAPC-SRCH`,
//! `LDAPC-COUNT`, `LDAPC-CMP`, each followed by the number.

/// The prototypes (the LDAP C API's, as `QSYSINC/H(LDAP)` declares them).
const PROTOS: &str = "\
extern proc ldap_init(byval $h: ptr, byval $p: i32) -> ptr
extern proc ldap_simple_bind_s(byval $ld: ptr, byval $d: ptr, byval $pw: ptr) -> i32
extern proc ldap_search_s(byval $ld: ptr, byval $b: ptr, byval $s: i32, byval $f: ptr, byval $a: ptr, byval $o: i32, byval $r: ptr) -> i32
extern proc ldap_count_entries(byval $ld: ptr, byval $r: ptr) -> i32
extern proc ldap_compare_s(byval $ld: ptr, byval $d: ptr, byval $a: ptr, byval $v: ptr) -> i32
extern proc ldap_msgfree(byval $m: ptr) -> i32
extern proc ldap_unbind(byval $ld: ptr) -> i32
";

/// The program `name`: bound as `dn`/`pw` to `host` (`host:port`), a
/// subtree search of `base`, a compare of `cmp_dn`'s `cmp_attr` with
/// `cmp_value`.
pub(crate) fn client(
    name: &str,
    host: &str,
    dn: &str,
    pw: &str,
    base: &str,
    cmp: (&str, &str, &str),
) -> String {
    let (cdn, cattr, cval) = cmp;
    let text = format!(
        "; made by the directory server steps (conformance::dirsrvstep::prog)
module {name}
static $host: bytes(*) inz(z\"{host}\")
static $dn: bytes(*) inz(z\"{dn}\")
static $pw: bytes(*) inz(z\"{pw}\")
static $base: bytes(*) inz(z\"{base}\")
static $filt: bytes(*) inz(z\"(objectClass=*)\")
static $cdn: bytes(*) inz(z\"{cdn}\")
static $cattr: bytes(*) inz(z\"{cattr}\")
static $cval: bytes(*) inz(z\"{cval}\")
static $res: ptr
proc {name}() entry {{
L0:
  %h = addr $host
  %ld = callb ldap_init(%h, 0)
  %d = addr $dn
  %p = addr $pw
  %rc = callb ldap_simple_bind_s(%ld, %d, %p)
  dsply \"LDAPC-BIND\"
  dsply %rc
  %b = addr $base
  %f = addr $filt
  %n = const.ptr *NULL
  %r = addr $res
  %rc2 = callb ldap_search_s(%ld, %b, 2, %f, %n, 0, %r)
  dsply \"LDAPC-SRCH\"
  dsply %rc2
  %chain = load $res
  %c = callb ldap_count_entries(%ld, %chain)
  dsply \"LDAPC-COUNT\"
  dsply %c
  %t = callb ldap_msgfree(%chain)
  %cd = addr $cdn
  %ca = addr $cattr
  %cv = addr $cval
  %rc3 = callb ldap_compare_s(%ld, %cd, %ca, %cv)
  dsply \"LDAPC-CMP\"
  dsply %rc3
  %u = callb ldap_unbind(%ld)
  ret
}}
"
    );
    crate::ifsapi::expand(&text, PROTOS)
}

/// The number after the `DSPLY` line `tag` in a call's output.
pub(crate) fn said(out: &str, tag: &str) -> Option<i64> {
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    let at = lines.iter().position(|l| l.ends_with(tag))?;
    lines[at + 1..].iter().find_map(|l| {
        l.split_whitespace()
            .last()
            .and_then(|w| w.parse::<i64>().ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_text() {
        let t = client(
            "LDAPC",
            "localhost:389",
            "cn=Administrator",
            "pw",
            "cn=localhost",
            ("cn=x,cn=localhost", "sn", "y"),
        );
        assert!(t.contains("extern proc ldap_compare_s"));
        assert!(t.contains("proc LDAPC() entry"));
        assert!(!t.contains("z\""), "literals expanded: {t}");
    }

    #[test]
    fn reads_what_it_said() {
        let out = "DSPLY  LDAPC-BIND\nDSPLY  0\nDSPLY  LDAPC-COUNT\nDSPLY  3\n";
        assert_eq!(said(out, "LDAPC-BIND"), Some(0));
        assert_eq!(said(out, "LDAPC-COUNT"), Some(3));
        assert_eq!(said(out, "LDAPC-CMP"), None);
    }
}
