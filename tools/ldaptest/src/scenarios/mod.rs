// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every scenario, in the order they run, and what they share: the
//! context, the sandbox the write scenarios work in.

mod acl;
pub mod controls;
pub mod ext;
mod ldifrt;
mod projected;
pub mod read;
mod write;

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use ldap3::Scope;

use crate::client::{Conn, expect_rc};
use crate::conn::Tls;
use crate::dse::RootDse;
use crate::error::skip;
use crate::target::Suite;
use crate::{Error, Outcome, Report, Target};

/// What a scenario returns: the lines it observed.
pub type Lines = Result<Vec<String>, Error>;

/// One scenario.
#[derive(Clone, Copy)]
pub struct Scenario {
    /// Its name.
    pub name: &'static str,
    /// Its suite.
    pub suite: Suite,
    /// Safe against the primary: anonymous, nothing written, no bind
    /// with a password; these are what `oracle` runs and records.
    pub oracle: bool,
    /// The scenario.
    pub run: fn(&Ctx) -> Lines,
}

impl std::fmt::Debug for Scenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

/// Every scenario.
pub fn all() -> Vec<Scenario> {
    let mut v = Vec::new();
    v.extend(read::LIST);
    v.extend(controls::LIST);
    v.extend(ext::LIST);
    v.extend(write::LIST);
    v.extend(acl::LIST);
    v.extend(projected::LIST);
    v.extend(ldifrt::LIST);
    v
}

/// The names of every scenario, with their suite and `oracle` mark.
pub fn names() -> Vec<String> {
    all()
        .iter()
        .map(|s| format!("{} {}{}", s.name, s.suite.name(), if s.oracle { " oracle" } else { "" }))
        .collect()
}

/// What every scenario sees.
pub struct Ctx {
    /// The command line.
    pub t: Target,
    /// The target's root DSE.
    pub dse: RootDse,
    /// A tag unique to this run, for names the scenarios make.
    pub tag: String,
}

impl Ctx {
    /// Reads the root DSE of `t`.
    pub fn new(t: Target) -> Result<Ctx, Error> {
        let dse = RootDse::read(&t)?;
        Ok(Ctx { t, dse, tag: format!("{}", std::process::id()) })
    }

    /// An anonymous connection.
    pub fn anon(&self) -> Result<Conn, Error> {
        Conn::open(&self.t, Tls::None)
    }

    /// A connection bound as the administrator; skip without its password.
    pub fn admin(&self) -> Result<Conn, Error> {
        let Some(pw) = self.t.admin_pw.as_deref() else {
            return skip("no administrator password (LDAPTEST_ADMIN_PW)");
        };
        self.bound(&self.t.admin_dn, pw)
    }

    /// A connection bound as `dn`.
    pub fn bound(&self, dn: &str, pw: &str) -> Result<Conn, Error> {
        let mut c = self.anon()?;
        let r = c.bind(dn, pw)?;
        expect_rc(&format!("bind {dn}"), &r, 0)?;
        Ok(c)
    }

    /// The write suffix; skip without one.
    pub fn suffix(&self) -> Result<&str, Error> {
        match self.t.suffix.as_deref() {
            Some(s) => Ok(s),
            None => skip("no --suffix to write under"),
        }
    }

    /// Skips unless the root DSE lists control `oid`.
    pub fn need_control(&self, oid: &str, what: &str) -> Result<(), Error> {
        if self.dse.control(oid) { Ok(()) } else { skip(format!("{what} ({oid}) not in supportedControl")) }
    }

    /// Skips unless the root DSE lists extended operation `oid`.
    pub fn need_ext(&self, oid: &str, what: &str) -> Result<(), Error> {
        if self.dse.extension(oid) { Ok(()) } else { skip(format!("{what} ({oid}) not in supportedExtension")) }
    }

    /// A sandbox under the suffix, as the administrator.
    pub fn sandbox(&self, name: &str) -> Result<Sandbox, Error> {
        let suffix = self.suffix()?.to_owned();
        let mut admin = self.admin()?;
        let dn = format!("ou=ldaptest-{name}-{},{suffix}", self.tag);
        let ou = format!("ldaptest-{name}-{}", self.tag);
        let r = admin.add(&dn, &[("objectClass", &["top", "organizationalUnit"]), ("ou", &[&ou])], vec![])?;
        expect_rc(&format!("add {dn}"), &r, 0)?;
        Ok(Sandbox { dn, admin })
    }
}

/// An `organizationalUnit` under the suffix that a write scenario works in,
/// removed with everything below it when dropped.
pub struct Sandbox {
    /// Its DN.
    pub dn: String,
    /// The administrator's connection.
    pub admin: Conn,
}

impl Sandbox {
    /// `rdn,<sandbox>`.
    pub fn child(&self, rdn: &str) -> String {
        format!("{rdn},{}", self.dn)
    }

    /// Adds `rdn,<sandbox>` with `attrs`, which must succeed: its DN.
    pub fn add(&mut self, rdn: &str, attrs: &[(&str, &[&str])]) -> Result<String, Error> {
        let dn = self.child(rdn);
        self.add_dn(dn, attrs)
    }

    /// Adds `dn` (anywhere) with `attrs`, which must succeed: its DN.
    pub fn add_dn(&mut self, dn: String, attrs: &[(&str, &[&str])]) -> Result<String, Error> {
        let r = self.admin.add(&dn, attrs, vec![])?;
        expect_rc(&format!("add {dn}"), &r, 0)?;
        Ok(dn)
    }

    /// Adds an `inetOrgPerson` `cn=NAME` with `userPassword`: its DN.
    pub fn user(&mut self, name: &str, pw: &str) -> Result<String, Error> {
        self.add(
            &format!("cn={name}"),
            &[
                ("objectClass", &["top", "person", "organizationalPerson", "inetOrgPerson"]),
                ("cn", &[name]),
                ("sn", &[name]),
                ("description", &["made by ldaptest"]),
                ("userPassword", &[pw]),
            ],
        )
    }

    /// Adds an `organizationalUnit` `ou=NAME`: its DN.
    pub fn ou(&mut self, name: &str, below: Option<&str>) -> Result<String, Error> {
        let dn = match below {
            Some(b) => format!("ou={name},{b}"),
            None => self.child(&format!("ou={name}")),
        };
        let r = self.admin.add(&dn, &[("objectClass", &["top", "organizationalUnit"]), ("ou", &[name])], vec![])?;
        expect_rc(&format!("add {dn}"), &r, 0)?;
        Ok(dn)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        remove_tree(&mut self.admin, &self.dn);
    }
}

/// Deletes `dn` and everything below it, deepest first; errors ignored.
pub fn remove_tree(c: &mut Conn, dn: &str) {
    let Ok(f) = c.find(dn, Scope::Subtree, "(objectClass=*)", &["1.1"]) else {
        return;
    };
    let mut dns = f.dns();
    dns.sort_by_key(|d| std::cmp::Reverse(rdn_count(d)));
    for d in dns {
        let _ = c.delete(&d, vec![]);
    }
}

/// The number of RDNs in `dn` (commas not escaped).
pub fn rdn_count(dn: &str) -> usize {
    let mut n = usize::from(!dn.is_empty());
    let mut esc = false;
    for ch in dn.chars() {
        match (esc, ch) {
            (false, '\\') => esc = true,
            (false, ',') => n += 1,
            _ => esc = false,
        }
    }
    n
}

/// Runs the scenarios `t` selects (in oracle mode only the oracle-safe ones).
pub fn run(c: &Ctx) -> Report {
    let mut rep = Report::default();
    let only: HashSet<&str> = c.t.only.iter().map(String::as_str).collect();
    for s in all() {
        let chosen = if only.is_empty() { c.t.suites.contains(&s.suite) } else { only.contains(s.name) };
        if !chosen {
            continue;
        }
        if c.t.is_oracle() && !s.oracle {
            continue;
        }
        let outcome = match catch_unwind(AssertUnwindSafe(|| (s.run)(c))) {
            Ok(Ok(lines)) => Outcome::Ok(lines),
            Ok(Err(Error::Skip(why))) => Outcome::Skip(why),
            Ok(Err(e)) => Outcome::Fail(e.to_string()),
            Err(p) => Outcome::Fail(format!(
                "panicked: {}",
                p.downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| p.downcast_ref::<&str>().copied())
                    .unwrap_or("?")
            )),
        };
        rep.push(s.name, outcome);
    }
    rep
}

/// Sorted, case-folded copies of `v` (for order-free comparisons).
pub fn folded(v: &[String]) -> Vec<String> {
    let mut out: Vec<String> = v.iter().map(|s| s.to_ascii_lowercase()).collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique() {
        let mut seen = HashSet::new();
        for s in all() {
            assert!(seen.insert(s.name), "{} twice", s.name);
        }
    }

    #[test]
    fn oracle_scenarios_are_reading_suites() {
        for s in all().iter().filter(|s| s.oracle) {
            assert!(
                matches!(s.suite, Suite::Read | Suite::Controls | Suite::Ext | Suite::Projected),
                "{} writes",
                s.name
            );
        }
    }

    #[test]
    fn rdns_counted() {
        assert_eq!(rdn_count(""), 0);
        assert_eq!(rdn_count("cn=a"), 1);
        assert_eq!(rdn_count("cn=a\\,b,ou=x,dc=y"), 3);
    }
}
