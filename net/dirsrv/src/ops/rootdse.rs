// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The root DSE (docs/DIRSRV.md §7). As on IBM's server its attributes
//! answer as user attributes: an empty list or `*` returns them all, `+`
//! alone none.

use ldap::msg::{Attribute, DerefAliases};

use super::Directory;

fn a(ty: &str, vals: &[&str]) -> Attribute {
    Attribute::new(ty, vals)
}

impl Directory {
    /// The root DSE's attributes.
    #[must_use]
    pub fn root_dse(&self) -> Vec<Attribute> {
        let s = &self.settings;
        let mut ncs: Vec<String> = vec!["CN=SCHEMA".into()];
        for b in &self.backends {
            if b.admin_only() {
                continue;
            }
            for x in b.suffixes() {
                ncs.push(x.to_string().to_ascii_uppercase());
            }
        }
        // IBM lists the local suffixes before the user ones.
        let local: Vec<String> = super::LOCAL_SUFFIXES
            .iter()
            .map(|x| x.to_ascii_uppercase())
            .collect();
        ncs.sort_by_key(|n| {
            if n == "CN=SCHEMA" {
                0
            } else if local.contains(n) {
                1 + local.iter().position(|l| l == n).unwrap_or(0)
            } else {
                10
            }
        });
        ncs.dedup();
        let deref = match s.deref {
            DerefAliases::Never => "never",
            DerefAliases::InSearching => "search",
            DerefAliases::FindingBase => "find",
            DerefAliases::Always => "always",
        };
        let ctls = super::controls::supported(s);
        let ncs_ref: Vec<&str> = ncs.iter().map(String::as_str).collect();
        let classes: Vec<String> = ["+", "++"]
            .into_iter()
            .map(String::from)
            .chain(
                super::attrs::OP_CLASSES
                    .iter()
                    .flat_map(|(n, _)| [format!("+{n}"), format!("++{n}")]),
            )
            .collect();
        let classes_ref: Vec<&str> = classes.iter().map(String::as_str).collect();
        let context = format!("OS400-SYS={}", s.system);
        let mut out = vec![
            a("namingContexts", &ncs_ref),
            a("subschemaSubentry", &[crate::backend::schema::DN]),
            a("supportedLDAPVersion", &["2", "3"]),
            a("supportedControl", &ctls),
            a("supportedExtension", &super::ext::SUPPORTED),
            a("supportedSASLMechanisms", &["EXTERNAL"]),
            a("vendorName", &["Quartz Systems"]),
            a("vendorVersion", &[&s.release]),
            a("ibmdirectoryversion", &["6.1"]),
            a("ibm-slapdServerInstanceName", &[&s.instance]),
            a("ibm-slapdServerBackend", &["QS400"]),
            a("ibm-osregistrycontext", &[&context]),
            a("ibm-osregistrytype", &["1.3.18.0.2.33.2-caseIgnore"]),
            a("ibm-ldapservicename", &[&s.system]),
            a("security", &[&s.security]),
            a("port", &[&s.port.to_string()]),
            a("ibm-slapdSizeLimit", &[&s.size_limit.to_string()]),
            a("ibm-slapdTimeLimit", &[&s.time_limit.to_string()]),
            a("ibm-slapdDerefAliases", &[deref]),
            a("ibm-supportedacimechanisms", &["1.3.18.0.2.26.2"]),
            a("ibm-opAttrClasses", &classes_ref),
            a("ibm-slapdisconfigurationmode", &["FALSE"]),
        ];
        if !s.server_id.is_empty() {
            out.push(a("ibm-serverId", &[&s.server_id]));
        }
        out
    }
}
