// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The instance list and `ibmslapd.conf` (docs/DIRSRV.md §4, §10).
//!
//! - [`instances`]: `idsinstances.ldif`, the instances and their
//!   autostart.
//! - [`slapd`]: an instance's `ibmslapd.conf`: values, validation, the
//!   administrator.
//! - [`attrs`]: the attributes QS/400 honours and IBM's it takes at
//!   their defaults.
//! - [`edit`]: changes to the file's text that keep its order and
//!   comments.
//! - [`shipped`]: the files a system starts with.
//! - [`error`]: what is wrong, as a `GLD` message.
//!
//! Both files are LDIF (`ldap::ldif`) in CCSID 1208 (the instance list
//! is ISO-8859-1 as IBM's is, its text ASCII).

pub mod attrs;
pub mod edit;
pub mod error;
pub mod instances;
pub mod shipped;
pub mod slapd;

pub use error::{ConfigError, Var};
pub use instances::{Instance, InstanceList};
pub use slapd::SlapdConf;

/// The directory server's user data.
pub const ROOT: &str = "/QIBM/UserData/OS400/DirSrv";

/// The instance list.
pub const INSTANCES: &str = "/QIBM/UserData/OS400/DirSrv/idsinstinfo/idsinstances.ldif";

/// The CCSID of the instance list (IBM's `charset: ISO-8859-1`).
pub const INSTANCES_CCSID: u32 = 819;

/// The CCSID of `ibmslapd.conf`.
pub const CONF_CCSID: u32 = 1208;

/// The instance every system has and `INSTANCE(*DFT)` means.
pub const DEFAULT_INSTANCE: &str = "QUSRDIR";

/// The configuration file of instance `name` under `location`
/// (`ids-instanceLocation`, [`ROOT`] for the shipped one).
#[must_use]
pub fn conf_path(location: &str, name: &str) -> String {
    format!(
        "{}/idsslapd-{name}/etc/ibmslapd.conf",
        location.trim_end_matches('/')
    )
}

/// A DN as a key: parsed, printed in RFC 4514 form, lower case (the
/// configuration's names are all `caseIgnoreMatch`). A string that is
/// not a DN is its trimmed lower-case self.
#[must_use]
pub fn dn_key(s: &str) -> String {
    ldap::Dn::parse(s.trim()).map_or_else(
        |_| s.trim().to_ascii_lowercase(),
        |d| d.to_string().to_ascii_lowercase(),
    )
}

/// The `dirsrvconf` fuzz target (`tools/fuzz`): the first byte picks
/// the file; an `ibmslapd.conf` is checked, an attribute set and read
/// back, the text parsed again.
pub fn fuzz(data: &[u8]) {
    let Some((&which, rest)) = data.split_first() else {
        return;
    };
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };
    if which & 1 == 0 {
        let _ =
            InstanceList::parse(text).map(|l| l.resolve(&instances::Which::All).map(|v| v.len()));
        return;
    }
    let Ok(mut c) = SlapdConf::parse("fuzz", text) else {
        return;
    };
    let _ = c.problems(DEFAULT_INSTANCE);
    let _ = (c.admin_dn(), c.admin_pw(), c.update_dn());
    if c.has_entry(slapd::CONFIGURATION) {
        c.set(slapd::CONFIGURATION, "ibm-slapdPort", &["3389"])
            .expect("the entry is there");
        assert_eq!(
            c.get(slapd::CONFIGURATION, "ibm-slapdPort").as_deref(),
            Some("3389"),
            "a value set reads back"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_paths() {
        assert_eq!(
            dn_key("CN=AUDIT, CN=LOG MANAGEMENT, CN=CONFIGURATION"),
            dn_key("cn=Audit,cn=Log Management,cn=Configuration")
        );
        fuzz(b"\x01dn: cn=Configuration\nibm-slapdPort: 389\n");
        fuzz(shipped::INSTANCES.as_bytes());
        // Found by the fuzzer: `version:` directly above the first `dn:`.
        fuzz(
            b"cversion:1
dn:cn=Configuration
n:",
        );
        fuzz(
            b"version:1
dn:CN=CONFIGURATI
 ON
i:
ibm-slapdPort:

dn:cn=Configuration
n:",
        );
        assert_eq!(
            conf_path(ROOT, "QUSRDIR"),
            "/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc/ibmslapd.conf"
        );
    }
}
