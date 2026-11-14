// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `idsinstances.ldif`: the directory server instances (docs/DIRSRV.md
//! §4). One `ids-instance` entry per instance under `cn=IDS
//! INSTANCES`: its name (`cn`), whether it starts with
//! `STRTCPSVR SERVER(*AUTOSTART)` (`ids-instanceAutostart`), its
//! location and IBM's version.

use ldap::ldif::{self, Body, Value};

use super::{ConfigError, DEFAULT_INSTANCE, INSTANCES, ROOT, conf_path};

/// One instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// The name (upper case, at most 8 characters on `CHGDIRSVRA`).
    pub name: String,
    /// Started with `INSTANCE(*AUTOSTART)` and at `STRTCP`.
    pub autostart: bool,
    /// The description.
    pub desc: String,
    /// The directory its `idsslapd-NAME` is in.
    pub location: String,
    /// IBM's version the instance was made at.
    pub version: String,
}

impl Instance {
    /// Its `ibmslapd.conf`.
    #[must_use]
    pub fn conf_path(&self) -> String {
        conf_path(&self.location, &self.name)
    }
}

/// Which instances a command names (`STRTCPSVR`/`ENDTCPSVR INSTANCE`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Which {
    /// `*DFT`: `QUSRDIR`.
    Default,
    /// `*ALL`.
    All,
    /// `*AUTOSTART`.
    Autostart,
    /// One by name.
    Named(String),
}

/// The instance list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstanceList {
    /// The instances, in the file's order.
    pub instances: Vec<Instance>,
}

fn text(v: &Value) -> String {
    match v {
        Value::Data(d) => String::from_utf8_lossy(d).trim().to_string(),
        Value::Url(u) => u.clone(),
    }
}

impl InstanceList {
    /// Read the file's text.
    ///
    /// # Errors
    ///
    /// `GLD0127` when it is not LDIF.
    pub fn parse(src: &str) -> Result<Self, ConfigError> {
        let l = ldif::parse(src.as_bytes()).map_err(|e| ConfigError::File {
            file: INSTANCES.into(),
            why: e.to_string(),
        })?;
        let mut out = Self::default();
        for r in &l.records {
            let Body::Content(attrs) = &r.body else {
                continue;
            };
            let get = |n: &str| {
                attrs
                    .iter()
                    .find(|a| a.ty.eq_ignore_ascii_case(n))
                    .map(|a| text(&a.value))
            };
            let is_instance = attrs.iter().any(|a| {
                a.ty.eq_ignore_ascii_case("objectclass")
                    && text(&a.value).eq_ignore_ascii_case("ids-instance")
            });
            let Some(name) = get("cn").filter(|_| is_instance) else {
                continue;
            };
            out.instances.push(Instance {
                name: name.to_ascii_uppercase(),
                autostart: get("ids-instanceAutostart").is_some_and(|v| v == "1"),
                desc: get("ids-instanceDesc").unwrap_or_default(),
                location: get("ids-instanceLocation").unwrap_or_else(|| ROOT.into()),
                version: get("ids-instanceVersion").unwrap_or_default(),
            });
        }
        Ok(out)
    }

    /// The instance of `name` (case ignored).
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&Instance> {
        self.instances
            .iter()
            .find(|i| i.name.eq_ignore_ascii_case(name.trim()))
    }

    /// The instances `which` names; a name the list does not have is
    /// `GLD0203`.
    ///
    /// # Errors
    ///
    /// `GLD0203` for an unknown name, or `*DFT` when `QUSRDIR` is gone.
    pub fn resolve(&self, which: &Which) -> Result<Vec<&Instance>, ConfigError> {
        let one = |n: &str| {
            self.find(n)
                .map(|i| vec![i])
                .ok_or_else(|| ConfigError::NoServerInstance(n.trim().to_ascii_uppercase()))
        };
        match which {
            Which::Default => one(DEFAULT_INSTANCE),
            Which::Named(n) => one(n),
            Which::All => Ok(self.instances.iter().collect()),
            Which::Autostart => Ok(self.instances.iter().filter(|i| i.autostart).collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_primarys_list() {
        let src = include_str!("../../../../tools/conformance/oracle/dirsrv/idsinstances.ldif");
        let l = InstanceList::parse(src).unwrap();
        assert_eq!(l.instances.len(), 1);
        let q = l.find("qusrdir").unwrap();
        assert!(q.autostart);
        assert_eq!(q.version, "6.1");
        assert_eq!(
            q.conf_path(),
            "/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc/ibmslapd.conf"
        );
        assert_eq!(l.resolve(&Which::Default).unwrap()[0].name, "QUSRDIR");
        assert_eq!(l.resolve(&Which::Autostart).unwrap().len(), 1);
        assert_eq!(
            l.resolve(&Which::Named("other".into())),
            Err(ConfigError::NoServerInstance("OTHER".into()))
        );
    }
}
