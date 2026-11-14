// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `aclEntry` and `entryOwner` values checked before they are written
//! (docs/DIRSRV.md §12): a subject (`access-id:`, `group:` or `role:`,
//! `access-id:` when none is given) and its DN, then for an ACL the
//! rights: `object:` with `a` and `d`, an attribute class (`normal`,
//! `sensitive`, `critical`, `system`, `restricted`) or `at.NAME` with
//! `r`, `w`, `s`, `c`, each optionally `grant:` or `deny:`.

use super::{AdminError, ErrorKind};

const CLASSES: &[&str] = &["normal", "sensitive", "critical", "system", "restricted"];

fn bad(v: &str, why: &str) -> AdminError {
    AdminError::plain(
        ErrorKind::Invalid,
        &format!("Access value {v} is not valid: {why}."),
    )
}

/// The subject and the rest: (`access-id` | `group` | `role`, rest).
fn subject(v: &str) -> (&'static str, &str) {
    for s in ["access-id", "group", "role"] {
        if let Some(r) = v
            .get(..s.len() + 1)
            .filter(|p| p.eq_ignore_ascii_case(&format!("{s}:")))
            .map(|_| &v[s.len() + 1..])
        {
            return (s, r);
        }
    }
    ("access-id", v)
}

/// Whether a rights segment starts at `t`.
fn is_keyword(t: &str) -> bool {
    let t = t.trim().to_ascii_lowercase();
    t == "object" || CLASSES.contains(&t.as_str()) || t.starts_with("at.")
}

fn check_dn(v: &str, dn: &str) -> Result<(), AdminError> {
    let d = dn.trim();
    if d.is_empty() || ldap::Dn::parse(d).is_err() {
        return Err(bad(v, "the distinguished name does not read"));
    }
    Ok(())
}

/// Check one `aclEntry` value.
///
/// # Errors
/// `CPF9898` naming the value and what is wrong.
pub fn check_acl(v: &str) -> Result<(), AdminError> {
    let (_, rest) = subject(v.trim());
    let parts: Vec<&str> = rest.split(':').collect();
    let Some(at) = parts.iter().position(|p| is_keyword(p)) else {
        return Err(bad(v, "it gives no rights"));
    };
    check_dn(v, &parts[..at].join(":"))?;
    let mut i = at;
    while i < parts.len() {
        let key = parts[i].trim().to_ascii_lowercase();
        if !is_keyword(&key) {
            return Err(bad(v, "a class or object right was expected"));
        }
        let allowed = if key == "object" { "ad" } else { "rwsc" };
        i += 1;
        loop {
            let mut t = parts.get(i).map_or("", |s| s.trim());
            if t.eq_ignore_ascii_case("grant") || t.eq_ignore_ascii_case("deny") {
                i += 1;
                t = parts.get(i).map_or("", |s| s.trim());
            }
            if t.is_empty() || t.chars().any(|c| !allowed.contains(c.to_ascii_lowercase())) {
                return Err(bad(v, "the rights are not valid"));
            }
            i += 1;
            let next = parts.get(i).map_or("", |s| s.trim());
            if !(next.eq_ignore_ascii_case("grant") || next.eq_ignore_ascii_case("deny")) {
                break;
            }
        }
    }
    Ok(())
}

/// Check one `entryOwner` value.
///
/// # Errors
/// `CPF9898` naming the value.
pub fn check_owner(v: &str) -> Result<(), AdminError> {
    let (_, dn) = subject(v.trim());
    check_dn(v, dn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acl_values() {
        for ok in [
            "group:cn=anybody:normal:rsc",
            "access-id:cn=Admin,o=Example:object:ad:normal:rwsc:sensitive:rsc",
            "cn=x,o=y:at.userPassword:deny:rwsc",
            "role:cn=ops,o=y:critical:grant:rs:deny:w",
        ] {
            assert_eq!(check_acl(ok), Ok(()), "{ok}");
        }
        for no in [
            "group:cn=anybody",
            "group::normal:r",
            "cn=x:normal:q",
            "cn=x:object:rw",
            "cn=x:normal:rsc:junk",
        ] {
            assert!(check_acl(no).is_err(), "{no}");
        }
        assert!(check_owner("group:cn=admins,o=y").is_ok());
        assert!(check_owner("access-id:").is_err());
    }
}
