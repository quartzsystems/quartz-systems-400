// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the operations take from the instance's configuration (§10).
//! `crate::config` fills it from `ibmslapd.conf`; the defaults are
//! `QUSRDIR`'s as the primary has them, with `ssha512`.

use ldap::msg::DerefAliases;

use crate::pwd::Scheme;
use crate::schema::Level;

/// The settings the operations obey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `ibm-slapdServerInstanceName`.
    pub instance: String,
    /// `ibm-slapdAdminDN`.
    pub admin_dn: String,
    /// `ibm-slapdAdminPW`, as stored (a `{SSHA512}` hash); `None` unset.
    pub admin_pw: Option<Vec<u8>>,
    /// `ibm-slapdAllowAnon`.
    pub allow_anon: bool,
    /// `ibm-slapdSizeLimit` (0: none).
    pub size_limit: u32,
    /// `ibm-slapdTimeLimit`, seconds (0: none).
    pub time_limit: u32,
    /// `ibm-slapdDerefAliases`: the most dereferencing allowed.
    pub deref: DerefAliases,
    /// `ibm-slapdSchemaCheck`.
    pub schema_check: Level,
    /// `ibm-slapdReadOnly`.
    pub read_only: bool,
    /// `ibm-slapdPwEncryption`.
    pub pw_scheme: Scheme,
    /// `ibm-slapdPagedResAllowNonAdmin`.
    pub paged_non_admin: bool,
    /// `ibm-slapdPagedResLmt`: paged searches open at once on a
    /// connection.
    pub paged_limit: u32,
    /// `ibm-slapdSortSrchAllowNonAdmin`.
    pub sort_non_admin: bool,
    /// `ibm-slapdSortKeyLimit`.
    pub sort_key_limit: u32,
    /// `ibm-slapdEnablePersistentSearch`.
    pub psearch: bool,
    /// `ibm-slapdMaxPersistentSearches`.
    pub psearch_max: u32,
    /// `ibm-slapdChangeLogMaxEntries` (0: no `cn=changelog`).
    pub changelog_max: u64,
    /// `ibm-slapdReferral`: the default referral.
    pub referral: Vec<String>,
    /// The user suffixes (`ibm-slapdSuffix` of the btree backend, beside
    /// `cn=localhost` and `cn=ibmpolicies`).
    pub suffixes: Vec<String>,
    /// `ibm-serverId`.
    pub server_id: String,
    /// The system's DNS name (`ibm-osregistrycontext`, the projected
    /// suffix `os400-sys=NAME`).
    pub system: String,
    /// `ibm-slapdPort`.
    pub port: u16,
    /// `ibm-slapdSecurity`.
    pub security: String,
    /// `vendorVersion`: the release.
    pub release: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            instance: "QUSRDIR".into(),
            admin_dn: "cn=Administrator".into(),
            admin_pw: None,
            allow_anon: true,
            size_limit: 500,
            time_limit: 900,
            deref: DerefAliases::Always,
            schema_check: Level::V3Lenient,
            read_only: false,
            pw_scheme: Scheme::Ssha512,
            paged_non_admin: true,
            paged_limit: 2,
            sort_non_admin: true,
            sort_key_limit: 3,
            psearch: true,
            psearch_max: 100,
            changelog_max: 0,
            referral: Vec::new(),
            suffixes: Vec::new(),
            server_id: String::new(),
            system: "SYSTEM".into(),
            port: 389,
            security: "none".into(),
            release: "V1R2M0".into(),
        }
    }
}
