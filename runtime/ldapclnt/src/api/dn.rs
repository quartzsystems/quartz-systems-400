// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DN functions: `ldap_explode_dn` and `ldap_explode_rdn` (and their
//! `_utf8` forms), `ldap_explode_dns`, `ldap_dn2ufn`. A DN that does not
//! parse gives null with `LDAP_INVALID_DN_SYNTAX`.

use ldap::dn::{Dn, Rdn, escape_value};
use mi::Job;

use super::{give_str, give_strings, null, ptr, with};
use crate::consts::*;
use crate::mem::{self, Codec};

fn value(v: &[u8]) -> String {
    String::from_utf8_lossy(v).into_owned()
}

/// An RDN's parts: `type=value` each, or the values alone.
#[must_use]
pub fn rdn_parts(r: &Rdn, notypes: bool) -> Vec<String> {
    r.0.iter()
        .map(|a| {
            if notypes {
                value(&a.value)
            } else {
                format!("{}={}", a.ty, escape_value(&a.value))
            }
        })
        .collect()
}

/// A DN's components, most specific first.
#[must_use]
pub fn dn_parts(d: &Dn, notypes: bool) -> Vec<String> {
    d.0.iter()
        .map(|r| rdn_parts(r, notypes).join("+"))
        .collect()
}

/// The user friendly name of a DN: its values, most specific first,
/// joined with `, `.
#[must_use]
pub fn ufn(d: &Dn) -> String {
    dn_parts(d, true).join(", ")
}

fn explode(j: &mut Job, p: u128, notypes: i64, codec: Option<Codec>, rdn: bool) -> u128 {
    with(j, |s, j| {
        let c = codec.unwrap_or_else(|| s.codec());
        let Some(t) = c.arg(j, p) else {
            s.errno = LDAP_PARAM_ERROR;
            return null();
        };
        let nt = mem::int(notypes) != 0;
        let parts = if rdn {
            Rdn::parse(&t).map(|r| rdn_parts(&r, nt))
        } else {
            Dn::parse(&t).map(|d| dn_parts(&d, nt))
        };
        match parts {
            Ok(v) => ptr(give_strings(s, j, c, &v)),
            Err(_) => {
                s.errno = LDAP_INVALID_DN_SYNTAX;
                null()
            }
        }
    })
}

const UTF8: Codec = Codec {
    ccsid: 1208,
    utf8: true,
};

/// `char **ldap_explode_dn(const char *dn, int notypes)`.
pub fn explode_dn(j: &mut Job, dn: u128, notypes: i64) -> u128 {
    explode(j, dn, notypes, None, false)
}

/// `char **ldap_explode_dn_utf8(const char *dn, int notypes)`.
pub fn explode_dn_utf8(j: &mut Job, dn: u128, notypes: i64) -> u128 {
    explode(j, dn, notypes, Some(UTF8), false)
}

/// `char **ldap_explode_rdn(const char *rdn, int notypes)`.
pub fn explode_rdn(j: &mut Job, rdn: u128, notypes: i64) -> u128 {
    explode(j, rdn, notypes, None, true)
}

/// `char **ldap_explode_rdn_utf8(const char *rdn, int notypes)`.
pub fn explode_rdn_utf8(j: &mut Job, rdn: u128, notypes: i64) -> u128 {
    explode(j, rdn, notypes, Some(UTF8), true)
}

/// `char **ldap_explode_dns(const char *dns)`: a DNS name's labels.
pub fn explode_dns(j: &mut Job, dns: u128) -> u128 {
    with(j, |s, j| {
        let c = s.codec();
        let Some(t) = c.arg(j, dns) else {
            s.errno = LDAP_PARAM_ERROR;
            return null();
        };
        let parts: Vec<String> = t
            .split('.')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        ptr(give_strings(s, j, c, &parts))
    })
}

/// `char *ldap_dn2ufn(const char *dn)`.
pub fn dn2ufn(j: &mut Job, dn: u128) -> u128 {
    with(j, |s, j| {
        let c = s.codec();
        let Some(t) = c.arg(j, dn) else {
            s.errno = LDAP_PARAM_ERROR;
            return null();
        };
        match Dn::parse(&t) {
            Ok(d) => ptr(give_str(s, j, c, &ufn(&d))),
            Err(_) => {
                s.errno = LDAP_INVALID_DN_SYNTAX;
                null()
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_and_ufn() {
        let d = Dn::parse("cn=Joe Smith+uid=js,ou=Sales,dc=example,dc=com").unwrap();
        assert_eq!(
            dn_parts(&d, false),
            ["cn=Joe Smith+uid=js", "ou=Sales", "dc=example", "dc=com"]
        );
        assert_eq!(dn_parts(&d, true)[0], "Joe Smith+js");
        assert_eq!(ufn(&d), "Joe Smith+js, Sales, example, com");
    }
}
