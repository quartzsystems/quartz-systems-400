// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An instance's `ibmslapd.conf` (docs/DIRSRV.md §10): IBM's entries
//! under `cn=Configuration`, read with `ldap::ldif`, checked against
//! [`super::attrs`], changed with [`super::edit`] so the file keeps its
//! order and comments.

use ldap::ldif::{self, Body, Ldif, Value};

use super::attrs::{self, Rule};
use super::{ConfigError, dn_key, edit};

/// `cn=Configuration`: the administrator, the port, the limits.
pub const CONFIGURATION: &str = "cn=Configuration";

/// The entry of the update DN `CHGDIRSVRA DN(*UPDATEDN)` changes.
pub const MASTER_SERVER: &str = "cn=Master Server, cn=Configuration";

/// The administrator when the file names none.
pub const DEFAULT_ADMIN: &str = "cn=Administrator";

/// A configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlapdConf {
    path: String,
    text: String,
    ldif: Ldif,
}

fn text(v: &Value) -> String {
    match v {
        Value::Data(d) => String::from_utf8_lossy(d).into_owned(),
        Value::Url(u) => u.clone(),
    }
}

impl SlapdConf {
    /// Read `text`, the file at `path`.
    ///
    /// # Errors
    ///
    /// `GLD0127` when it is not LDIF.
    pub fn parse(path: &str, text: &str) -> Result<Self, ConfigError> {
        let ldif = ldif::parse(text.as_bytes()).map_err(|e| ConfigError::File {
            file: path.into(),
            why: e.to_string(),
        })?;
        Ok(Self {
            path: path.into(),
            text: text.into(),
            ldif,
        })
    }

    /// The file's text, as it is to be written.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Its path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Every entry's DN and `(attribute, value)` lines, in order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, Vec<(&str, String)>)> {
        self.ldif.records.iter().filter_map(|r| match &r.body {
            Body::Content(a) => Some((
                r.dn.as_str(),
                a.iter().map(|v| (v.ty.as_str(), text(&v.value))).collect(),
            )),
            _ => None,
        })
    }

    /// The values of `attr` in entry `entry`.
    #[must_use]
    pub fn values(&self, entry: &str, attr: &str) -> Vec<String> {
        let key = dn_key(entry);
        self.entries()
            .filter(|(dn, _)| dn_key(dn) == key)
            .flat_map(|(_, a)| {
                a.into_iter()
                    .filter(|(t, _)| t.eq_ignore_ascii_case(attr))
                    .map(|(_, v)| v)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The first value of `attr` in entry `entry`.
    #[must_use]
    pub fn get(&self, entry: &str, attr: &str) -> Option<String> {
        self.values(entry, attr).into_iter().next()
    }

    /// Whether the file has entry `entry`.
    #[must_use]
    pub fn has_entry(&self, entry: &str) -> bool {
        let key = dn_key(entry);
        self.entries().any(|(dn, _)| dn_key(dn) == key)
    }

    /// Every problem, in the file's order: a change record, an
    /// attribute QS/400 does not know or takes only at IBM's default
    /// (`GLD0126`), a value it does not take (`GLD0232`).
    #[must_use]
    pub fn problems(&self, instance: &str) -> Vec<ConfigError> {
        let mut out = Vec::new();
        for r in &self.ldif.records {
            let Body::Content(attrs) = &r.body else {
                out.push(ConfigError::Unsupported {
                    what: r.dn.clone(),
                    file: self.path.clone(),
                });
                continue;
            };
            for a in attrs {
                let v = text(&a.value);
                let ok = match attrs::rule(&a.ty) {
                    Some(Rule::Kept) => true,
                    Some(Rule::Honoured(c)) => {
                        if !attrs::passes(c, &v) {
                            out.push(ConfigError::BadValue {
                                attr: a.ty.clone(),
                                value: v.clone(),
                            });
                        }
                        true
                    }
                    Some(Rule::Default(d)) => attrs::is_default(d, instance, &v),
                    None => false,
                };
                if !ok {
                    out.push(ConfigError::Unsupported {
                        what: format!("{}: {v}", a.ty),
                        file: self.path.clone(),
                    });
                }
            }
        }
        out
    }

    /// The first problem, if any ([`SlapdConf::problems`]).
    ///
    /// # Errors
    ///
    /// The first problem.
    pub fn validate(&self, instance: &str) -> Result<(), ConfigError> {
        match self.problems(instance).into_iter().next() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Set `attr` of entry `entry` to `values` (none removes it).
    ///
    /// # Errors
    ///
    /// `GLD0126` naming the entry when the file has none.
    pub fn set(&mut self, entry: &str, attr: &str, values: &[&str]) -> Result<(), ConfigError> {
        let t =
            edit::set(&self.text, entry, attr, values).ok_or_else(|| ConfigError::Unsupported {
                what: entry.into(),
                file: self.path.clone(),
            })?;
        *self = Self::parse(&self.path, &t)?;
        Ok(())
    }

    /// The administrator's DN.
    #[must_use]
    pub fn admin_dn(&self) -> String {
        self.get(CONFIGURATION, "ibm-slapdAdminDN")
            .unwrap_or_else(|| DEFAULT_ADMIN.into())
    }

    /// The administrator's stored password, if one is set.
    #[must_use]
    pub fn admin_pw(&self) -> Option<String> {
        self.get(CONFIGURATION, "ibm-slapdAdminPW")
            .filter(|p| !p.trim().is_empty())
    }

    /// The update DN, if the file names one.
    #[must_use]
    pub fn update_dn(&self) -> Option<String> {
        self.get(MASTER_SERVER, "ibm-slapdMasterDN")
            .filter(|d| !d.trim().is_empty())
    }

    /// Set the administrator (`dn`, unless `None`) and its stored
    /// password.
    ///
    /// # Errors
    ///
    /// `GLD0224` when `dn` is not a DN.
    pub fn set_admin(&mut self, dn: Option<&str>, stored_pw: &str) -> Result<(), ConfigError> {
        if let Some(d) = dn {
            let d = d.trim();
            if ldap::Dn::parse(d).map_or(true, |p| p.is_root()) {
                return Err(ConfigError::BadDn(d.into()));
            }
            self.set(CONFIGURATION, "ibm-slapdAdminDN", &[d])?;
        }
        self.set(CONFIGURATION, "ibm-slapdAdminPW", &[stored_pw])
    }

    /// Set the update DN's stored password.
    ///
    /// # Errors
    ///
    /// `GLD0215` when the file names no update DN.
    pub fn set_update_pw(&mut self, instance: &str, stored_pw: &str) -> Result<(), ConfigError> {
        if self.update_dn().is_none() {
            return Err(ConfigError::NoUpdateDn(instance.into()));
        }
        self.set(MASTER_SERVER, "ibm-slapdMasterPW", &[stored_pw])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc/ibmslapd.conf";

    fn shipped() -> SlapdConf {
        SlapdConf::parse(
            PATH,
            &super::super::shipped::slapd_conf("QUSRDIR", "SYSTEM"),
        )
        .unwrap()
    }

    /// The primary's file, its secrets put back as values a system
    /// could have, passes.
    #[test]
    fn the_primarys_file_passes() {
        let src = include_str!("../../../../tools/conformance/oracle/dirsrv/ibmslapd-shape.ldif")
            .replace("{masked}", "TRUE")
            .replace("ibm-slapdPwEncryption: TRUE", "ibm-slapdPwEncryption: ssha")
            .replace("ibm-slapdAdminPW: TRUE", "ibm-slapdAdminPW: {SSHA}x")
            .replace("ibm-slapdServerId: {server-id}", "ibm-slapdServerId: 0d2f")
            .replace("{host}", "host")
            .replace("{domain}", "example")
            .replace("{system}", "SYSTEM");
        let src = src
            .lines()
            .map(|l| match l.split_once(": ") {
                Some((a, "TRUE")) if a.starts_with("pwd") && a != "pwdLockout" => {
                    format!("{a}: 0")
                }
                Some((a, "TRUE")) if a.starts_with("password") => format!("{a}: 0"),
                _ => l.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        let c = SlapdConf::parse(PATH, &src).unwrap();
        assert_eq!(c.problems("QUSRDIR"), Vec::new());
        assert_eq!(c.admin_dn(), "cn=Administrator");
    }

    #[test]
    fn unknown_and_non_default_attributes_stop_it() {
        let mut c = shipped();
        assert_eq!(c.validate("QUSRDIR"), Ok(()));
        c.set(
            "cn=Front End, cn=Configuration",
            "ibm-slapdACLCacheSize",
            &["1"],
        )
        .unwrap();
        let e = c.validate("QUSRDIR").unwrap_err();
        assert_eq!(e.msgid(), "GLD0126");
        assert!(e.to_string().contains("ibm-slapdACLCacheSize: 1"), "{e}");
        let mut c = shipped();
        c.set("cn=Configuration", "ibm-slapdNoSuch", &["x"])
            .unwrap();
        assert!(
            c.validate("QUSRDIR")
                .unwrap_err()
                .to_string()
                .contains("ibm-slapdNoSuch")
        );
        let mut c = shipped();
        c.set("cn=Configuration", "ibm-slapdPwEncryption", &["aes256"])
            .unwrap();
        let e = c.validate("QUSRDIR").unwrap_err();
        assert_eq!(e.msgid(), "GLD0232");
        // The instance's own paths are its defaults; another's are not.
        let other = SlapdConf::parse(PATH, c.text()).unwrap();
        assert!(
            other
                .problems("OTHER")
                .iter()
                .any(|e| e.msgid() == "GLD0126")
        );
    }

    #[test]
    fn the_administrator() {
        let mut c = shipped();
        assert_eq!(c.admin_dn(), DEFAULT_ADMIN);
        assert_eq!(c.admin_pw(), None);
        c.set_admin(None, "{SSHA512}abc").unwrap();
        assert_eq!(c.admin_pw().as_deref(), Some("{SSHA512}abc"));
        c.set_admin(Some("cn=Boss, o=Example"), "{SSHA512}def")
            .unwrap();
        assert_eq!(c.admin_dn(), "cn=Boss, o=Example");
        assert_eq!(
            c.set_admin(Some("not a dn"), "x"),
            Err(ConfigError::BadDn("not a dn".into()))
        );
        assert_eq!(
            c.set_update_pw("QUSRDIR", "x"),
            Err(ConfigError::NoUpdateDn("QUSRDIR".into()))
        );
        assert_eq!(c.validate("QUSRDIR"), Ok(()));
        // Comments and order stay.
        assert!(c.text().starts_with('#'));
    }
}
