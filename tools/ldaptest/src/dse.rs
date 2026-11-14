// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The root DSE: what the target advertises, read once before the
//! scenarios run, which decides the skips.

use std::collections::BTreeMap;

use ldap3::Scope;

use crate::client::Conn;
use crate::conn::Tls;
use crate::{Error, Target};

/// The root DSE's attributes, names lower-cased.
#[derive(Clone, Debug, Default)]
pub struct RootDse {
    /// Name and values.
    pub attrs: BTreeMap<String, Vec<String>>,
}

impl RootDse {
    /// Reads it anonymously (`*` and `+`).
    pub fn read(t: &Target) -> Result<RootDse, Error> {
        let mut c = Conn::open(t, Tls::None)?;
        let f = c.find("", Scope::Base, "(objectClass=*)", &["*", "+"])?;
        c.close();
        crate::client::expect_rc("root DSE", &f.res, 0)?;
        let e = f.entries.into_iter().next().ok_or_else(|| Error::Check("root DSE: no entry".into()))?;
        let mut attrs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (k, v) in e.attrs {
            attrs.entry(k.to_ascii_lowercase()).or_default().extend(v);
        }
        Ok(RootDse { attrs })
    }

    /// The values of `name`.
    pub fn get(&self, name: &str) -> &[String] {
        self.attrs.get(&name.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    /// The first value of `name`.
    pub fn one(&self, name: &str) -> Option<&str> {
        self.get(name).first().map(String::as_str)
    }

    /// Whether `oid` is in `supportedControl`.
    pub fn control(&self, oid: &str) -> bool {
        self.get("supportedControl").iter().any(|v| v == oid)
    }

    /// Whether `oid` is in `supportedExtension`.
    pub fn extension(&self, oid: &str) -> bool {
        self.get("supportedExtension").iter().any(|v| v == oid)
    }

    /// Whether `mech` is in `supportedSASLMechanisms`.
    pub fn sasl(&self, mech: &str) -> bool {
        self.get("supportedSASLMechanisms").iter().any(|v| v.eq_ignore_ascii_case(mech))
    }

    /// The projected backend's `os400-sys=` value, from `ibm-osregistrycontext`.
    pub fn system(&self) -> Option<String> {
        let ctx = self.one("ibm-osregistrycontext")?;
        let (k, v) = ctx.split_once('=')?;
        k.eq_ignore_ascii_case("os400-sys").then(|| v.to_owned())
    }

    /// The naming context, if any, equal to `dn` (case ignored).
    pub fn has_context(&self, dn: &str) -> bool {
        self.get("namingContexts").iter().any(|v| v.eq_ignore_ascii_case(dn))
    }
}
