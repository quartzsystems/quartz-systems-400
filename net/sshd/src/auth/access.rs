// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Who may sign on at all: `DenyUsers`, `AllowUsers`, `DenyGroups`,
//! `AllowGroups` in OpenSSH's order, and `PermitRootLogin` for
//! `QSECOFR`. Names are compared without regard to case.

use super::Account;
use crate::config::{Config, RootLogin, pattern};

/// The profile `PermitRootLogin` governs.
pub const ROOT: &str = "QSECOFR";

/// Whether the configuration lets `account` in from `peer` at all.
#[must_use]
pub fn permitted(cfg: &Config, account: &Account, peer: &str) -> bool {
    let user = account.name.as_str();
    if cfg
        .deny_users
        .iter()
        .any(|e| pattern::user_entry(user, peer, e))
    {
        return false;
    }
    if !cfg.allow_users.is_empty()
        && !cfg
            .allow_users
            .iter()
            .any(|e| pattern::user_entry(user, peer, e))
    {
        return false;
    }
    let in_group = |pats: &[String]| {
        account
            .groups
            .iter()
            .any(|g| pats.iter().any(|p| pattern::wildcard(g, p)))
    };
    if !cfg.deny_groups.is_empty() && in_group(&cfg.deny_groups) {
        return false;
    }
    if !cfg.allow_groups.is_empty() && !in_group(&cfg.allow_groups) {
        return false;
    }
    true
}

/// Whether `QSECOFR` may use a password.
#[must_use]
pub fn root_password_ok(cfg: &Config, account: &Account) -> bool {
    account.name != ROOT || cfg.permit_root_login == RootLogin::Yes
}

/// Whether `QSECOFR` may use a key; `forced` is whether the key carries
/// `command=`.
#[must_use]
pub fn root_key_ok(cfg: &Config, account: &Account, forced: bool) -> bool {
    if account.name != ROOT {
        return true;
    }
    match cfg.permit_root_login {
        RootLogin::Yes | RootLogin::ProhibitPassword => true,
        RootLogin::ForcedCommandsOnly => forced,
        RootLogin::No => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acct(name: &str, groups: &[&str]) -> Account {
        Account {
            name: name.into(),
            home: format!("/home/{name}"),
            groups: groups.iter().map(|g| (*g).to_string()).collect(),
            uid: 100,
        }
    }

    #[test]
    fn order_and_root() {
        let c = Config::parse("AllowUsers qs*\nDenyUsers qsbad\nAllowGroups staff\n");
        assert!(permitted(&c, &acct("QSTEST", &["STAFF"]), "1.2.3.4"));
        assert!(!permitted(&c, &acct("QSBAD", &["STAFF"]), "1.2.3.4"));
        assert!(!permitted(&c, &acct("BOB", &["STAFF"]), "1.2.3.4"));
        assert!(!permitted(&c, &acct("QSTEST", &["OTHER"]), "1.2.3.4"));
        let d = Config::parse("");
        let root = acct("QSECOFR", &[]);
        assert!(!root_password_ok(&d, &root));
        assert!(root_key_ok(&d, &root, false));
        let no = Config::parse("PermitRootLogin no");
        assert!(!root_key_ok(&no, &root, true));
        let fc = Config::parse("PermitRootLogin forced-commands-only");
        assert!(!root_key_ok(&fc, &root, false) && root_key_ok(&fc, &root, true));
    }
}
