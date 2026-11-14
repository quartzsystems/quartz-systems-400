// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An instance's `ibmslapd.conf` as the service runs it (docs/DIRSRV.md
//! §4, §10): the operations' [`Settings`], what the job needs for its
//! listeners and connections, the audit switches, the index list and the
//! projected suffix.
//!
//! Entries are found by their object class (`ibm-slapdRdbmBackend` the
//! entry store's suffixes, `ibm-slapdOs400SystemBackend` the projected
//! one) rather than by DN, so a renamed backend entry still counts.

use ldap::msg::DerefAliases;

use super::slapd::CONFIGURATION;
use super::{ConfigError, SlapdConf};
use crate::ops::{AuditOp, LOCAL_SUFFIXES, Settings};
use crate::pwd::Scheme;
use crate::schema::Level;

/// The entry of the connection settings.
pub const CONNECTIONS: &str = "cn=Connection Management, cn=Front End, cn=Configuration";
/// The entry of the TLS settings.
pub const SSL: &str = "cn=SSL, cn=Configuration";
/// The entry of the audit switches.
pub const AUDIT: &str = "cn=Audit, cn=Log Management, cn=Configuration";
/// The entry of the persistent search settings.
pub const PSEARCH: &str = "cn=Persistent Search, cn=Configuration";
/// The entry of the schema settings.
pub const SCHEMA: &str = "cn=IBM Directory, cn=Schemas, cn=Configuration";

/// `cn=Audit`'s switches (§12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct AuditSwitches {
    /// `ibm-audit`: auditing at all.
    pub on: bool,
    /// `ibm-auditBind`.
    pub bind: bool,
    /// `ibm-auditSearch`.
    pub search: bool,
    /// `ibm-auditCompare`.
    pub compare: bool,
    /// `ibm-auditAdd`.
    pub add: bool,
    /// `ibm-auditModify`.
    pub modify: bool,
    /// `ibm-auditDelete`.
    pub delete: bool,
    /// `ibm-auditModifyDN`.
    pub moddn: bool,
    /// `ibm-auditExtOp`.
    pub extop: bool,
    /// `ibm-auditFailedOPonly`: only operations that failed.
    pub failed_only: bool,
}

impl AuditSwitches {
    /// Whether an operation of kind `op` that succeeded (or not) is
    /// audited.
    #[must_use]
    pub fn wants(&self, op: AuditOp, succeeded: bool) -> bool {
        if !self.on || (self.failed_only && succeeded) {
            return false;
        }
        match op {
            AuditOp::Bind => self.bind,
            AuditOp::Search => self.search,
            AuditOp::Compare => self.compare,
            AuditOp::Add => self.add,
            AuditOp::Modify => self.modify,
            AuditOp::Delete => self.delete,
            AuditOp::ModDn => self.moddn,
            AuditOp::Extended => self.extop,
        }
    }
}

/// An instance's configuration, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    /// What the operations obey.
    pub settings: Settings,
    /// `ibm-slapdSecurePort`.
    pub secure_port: u16,
    /// `ibm-slapdIdleTimeOut` of the connections entry, seconds.
    pub idle_timeout: u32,
    /// `ibm-slapdWriteTimeout`, seconds.
    pub write_timeout: u32,
    /// The audit switches.
    pub audit: AuditSwitches,
    /// `ibm-slapdIndex` of the entry store's backend: attributes indexed
    /// beside the ones always indexed (each value an attribute name).
    pub indexes: Vec<String>,
    /// The projected suffix (`os400-sys=NAME`), when the file has the
    /// system backend.
    pub projected: Option<String>,
}

fn value(conf: &SlapdConf, entry: &str, attr: &str) -> Option<String> {
    conf.get(entry, attr).map(|v| v.trim().to_string())
}

fn number<T: TryFrom<u64>>(conf: &SlapdConf, entry: &str, attr: &str, dft: T) -> T {
    value(conf, entry, attr)
        .and_then(|v| v.parse::<u64>().ok())
        .and_then(|n| T::try_from(n).ok())
        .unwrap_or(dft)
}

fn flag(conf: &SlapdConf, entry: &str, attr: &str, dft: bool) -> bool {
    value(conf, entry, attr).map_or(dft, |v| v.eq_ignore_ascii_case("true"))
}

/// The entries whose `objectclass` includes `class`, in the file's order.
fn of_class<'a>(conf: &'a SlapdConf, class: &str) -> Vec<&'a str> {
    conf.entries()
        .filter(|(_, a)| {
            a.iter().any(|(t, v)| {
                t.eq_ignore_ascii_case("objectclass") && v.trim().eq_ignore_ascii_case(class)
            })
        })
        .map(|(dn, _)| dn)
        .collect()
}

/// The first value of `attr` in any entry (QS/400's own attributes,
/// which may sit on `cn=Configuration` or a backend's entry).
fn anywhere(conf: &SlapdConf, attr: &str) -> Vec<String> {
    conf.entries()
        .flat_map(|(_, a)| {
            a.into_iter()
                .filter(|(t, _)| t.eq_ignore_ascii_case(attr))
                .map(|(_, v)| v.trim().to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn deref(v: &str) -> DerefAliases {
    match v.to_ascii_lowercase().as_str() {
        "never" => DerefAliases::Never,
        "search" => DerefAliases::InSearching,
        "find" => DerefAliases::FindingBase,
        _ => DerefAliases::Always,
    }
}

fn is_local(s: &str) -> bool {
    LOCAL_SUFFIXES
        .iter()
        .any(|l| super::dn_key(l) == super::dn_key(s))
}

/// `cn=Audit`'s switches.
fn audit(conf: &SlapdConf) -> AuditSwitches {
    AuditSwitches {
        on: flag(conf, AUDIT, "ibm-audit", false),
        bind: flag(conf, AUDIT, "ibm-auditBind", true),
        search: flag(conf, AUDIT, "ibm-auditSearch", false),
        compare: flag(conf, AUDIT, "ibm-auditCompare", false),
        add: flag(conf, AUDIT, "ibm-auditAdd", false),
        modify: flag(conf, AUDIT, "ibm-auditModify", false),
        delete: flag(conf, AUDIT, "ibm-auditDelete", false),
        moddn: flag(conf, AUDIT, "ibm-auditModifyDN", false),
        extop: flag(conf, AUDIT, "ibm-auditExtOp", false),
        failed_only: flag(conf, AUDIT, "ibm-auditFailedOPonly", true),
    }
}

/// The projected suffix and the system name in it.
fn projected(conf: &SlapdConf) -> Result<(Option<String>, String), ConfigError> {
    let mut projected = None;
    for e in of_class(conf, "ibm-slapdOs400SystemBackend") {
        if let Some(v) = value(conf, e, "ibm-slapdOs400UsrprjRead")
            && !v.eq_ignore_ascii_case("true")
        {
            return Err(ConfigError::BadValue {
                attr: "ibm-slapdOs400UsrprjRead".into(),
                value: v,
            });
        }
        if projected.is_none() {
            projected = conf
                .values(e, "ibm-slapdSuffix")
                .into_iter()
                .map(|s| s.trim().to_string())
                .next();
        }
    }
    let system = projected
        .as_deref()
        .and_then(|p| ldap::Dn::parse(p).ok())
        .and_then(|d| {
            let rdn = d.0.first()?;
            let ava = rdn.0.first()?;
            ava.ty
                .eq_ignore_ascii_case("os400-sys")
                .then(|| String::from_utf8_lossy(&ava.value).into_owned())
        })
        .unwrap_or_else(|| Settings::default().system);
    Ok((projected, system))
}

/// The configuration `conf` of instance `instance` on release `release`.
///
/// # Errors
///
/// The file's first problem ([`SlapdConf::validate`]), or `GLD0232` for
/// a value the checks pass but QS/400 does not run with
/// (`ibm-slapdPwEncryption` refused, `ibm-slapdOs400UsrprjRead: FALSE`:
/// the projected backend is read-only).
pub fn runtime(conf: &SlapdConf, instance: &str, release: &str) -> Result<Runtime, ConfigError> {
    conf.validate(instance)?;
    let bad = |attr: &str, value: &str| ConfigError::BadValue {
        attr: attr.into(),
        value: value.into(),
    };
    let c = CONFIGURATION;
    let pw_scheme = match value(conf, c, "ibm-slapdPwEncryption") {
        None => Scheme::Ssha512,
        Some(v) => Scheme::from_config(&v).map_err(|_| bad("ibm-slapdPwEncryption", &v))?,
    };
    let schema_check = match value(conf, SCHEMA, "ibm-slapdSchemaCheck") {
        None => Level::V3Lenient,
        Some(v) => Level::parse(&v).ok_or_else(|| bad("ibm-slapdSchemaCheck", &v))?,
    };
    let rdbm = of_class(conf, "ibm-slapdRdbmBackend");
    let mut suffixes = Vec::new();
    let mut read_only = false;
    let mut paged = (true, 2u32);
    let mut sort = (true, 3u32);
    let mut indexes = Vec::new();
    for e in &rdbm {
        for s in conf.values(e, "ibm-slapdSuffix") {
            let s = s.trim().to_string();
            if !is_local(&s) && !suffixes.iter().any(|x: &String| x.eq_ignore_ascii_case(&s)) {
                suffixes.push(s);
            }
        }
        read_only |= flag(conf, e, "ibm-slapdReadOnly", false);
        paged = (
            flag(conf, e, "ibm-slapdPagedResAllowNonAdmin", paged.0),
            number(conf, e, "ibm-slapdPagedResLmt", paged.1),
        );
        sort = (
            flag(conf, e, "ibm-slapdSortSrchAllowNonAdmin", sort.0),
            number(conf, e, "ibm-slapdSortKeyLimit", sort.1),
        );
        for v in conf.values(e, "ibm-slapdIndex") {
            for a in v.split([',', ' ']).map(str::trim).filter(|a| !a.is_empty()) {
                if !indexes.iter().any(|x: &String| x.eq_ignore_ascii_case(a)) {
                    indexes.push(a.to_string());
                }
            }
        }
    }
    let (projected, system) = projected(conf)?;
    let settings = Settings {
        instance: instance.to_string(),
        admin_dn: conf.admin_dn(),
        admin_pw: conf.admin_pw().map(String::into_bytes),
        allow_anon: flag(conf, CONNECTIONS, "ibm-slapdAllowAnon", true),
        size_limit: number(conf, c, "ibm-slapdSizeLimit", 500),
        time_limit: number(conf, c, "ibm-slapdTimeLimit", 900),
        deref: value(conf, c, "ibm-slapdDerefAliases").map_or(DerefAliases::Always, |v| deref(&v)),
        schema_check,
        read_only,
        pw_scheme,
        paged_non_admin: paged.0,
        paged_limit: paged.1,
        sort_non_admin: sort.0,
        sort_key_limit: sort.1,
        psearch: flag(conf, PSEARCH, "ibm-slapdEnablePersistentSearch", true),
        psearch_max: number(conf, PSEARCH, "ibm-slapdMaxPersistentSearches", 100),
        changelog_max: anywhere(conf, "ibm-slapdChangeLogMaxEntries")
            .first()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        referral: anywhere(conf, "ibm-slapdReferral"),
        suffixes,
        server_id: value(conf, c, "ibm-slapdServerId").unwrap_or_default(),
        system,
        port: number(conf, c, "ibm-slapdPort", 389),
        security: value(conf, SSL, "ibm-slapdSecurity").unwrap_or_else(|| "none".into()),
        release: release.to_string(),
    };
    Ok(Runtime {
        settings,
        secure_port: number(conf, SSL, "ibm-slapdSecurePort", 636),
        idle_timeout: number(conf, CONNECTIONS, "ibm-slapdIdleTimeOut", 300),
        write_timeout: number(conf, CONNECTIONS, "ibm-slapdWriteTimeout", 120),
        audit: audit(conf),
        indexes,
        projected,
    })
}

#[cfg(test)]
mod tests {
    use super::super::shipped::slapd_conf;
    use super::*;

    const PATH: &str = "/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc/ibmslapd.conf";
    const RDBM: &str =
        "cn=Directory, cn=RDBM Backends, cn=IBM Directory, cn=Schemas, cn=Configuration";
    const SYSTEM: &str =
        "cn=System, cn=System Backends, cn=IBM Directory, cn=Schemas, cn=Configuration";

    fn shipped() -> SlapdConf {
        SlapdConf::parse(PATH, &slapd_conf("QUSRDIR", "MYSYS.EXAMPLE.COM")).unwrap()
    }

    #[test]
    fn the_shipped_file_is_the_defaults() {
        let r = runtime(&shipped(), "QUSRDIR", "V1R2M0").unwrap();
        let want = Settings {
            system: "MYSYS.EXAMPLE.COM".into(),
            ..Settings::default()
        };
        assert_eq!(r.settings, want);
        assert_eq!(r.secure_port, 636);
        assert_eq!((r.idle_timeout, r.write_timeout), (300, 120));
        assert_eq!(r.projected.as_deref(), Some("os400-sys=MYSYS.EXAMPLE.COM"));
        assert!(r.indexes.is_empty());
        assert!(!r.audit.on && r.audit.bind && r.audit.failed_only);
        assert!(!r.audit.wants(AuditOp::Bind, false));
    }

    #[test]
    fn changed_values_are_read() {
        let mut c = shipped();
        c.set(CONFIGURATION, "ibm-slapdSizeLimit", &["20"]).unwrap();
        c.set(CONFIGURATION, "ibm-slapdPwEncryption", &["crypt"])
            .unwrap();
        c.set(CONFIGURATION, "ibm-slapdDerefAliases", &["find"])
            .unwrap();
        c.set(CONFIGURATION, "ibm-slapdAdminPW", &["{SSHA512}abc"])
            .unwrap();
        c.set(
            RDBM,
            "ibm-slapdSuffix",
            &["cn=localhost", "cn=ibmpolicies", "o=Acme", "dc=x,dc=y"],
        )
        .unwrap();
        c.set(RDBM, "ibm-slapdIndex", &["telephoneNumber", "ou, l"])
            .unwrap();
        c.set(RDBM, "ibm-slapdReadOnly", &["TRUE"]).unwrap();
        c.set(CONNECTIONS, "ibm-slapdAllowAnon", &["FALSE"])
            .unwrap();
        c.set(SSL, "ibm-slapdSecurity", &["SSLTLS"]).unwrap();
        c.set(AUDIT, "ibm-audit", &["true"]).unwrap();
        c.set(AUDIT, "ibm-auditFailedOPonly", &["false"]).unwrap();
        c.set(AUDIT, "ibm-auditAdd", &["true"]).unwrap();
        c.set(PSEARCH, "ibm-slapdMaxPersistentSearches", &["7"])
            .unwrap();
        let r = runtime(&c, "QUSRDIR", "V1R2M0").unwrap();
        let s = &r.settings;
        assert_eq!(s.size_limit, 20);
        assert_eq!(s.pw_scheme, Scheme::Crypt);
        assert_eq!(s.deref, DerefAliases::FindingBase);
        assert_eq!(s.admin_pw.as_deref(), Some(&b"{SSHA512}abc"[..]));
        assert_eq!(s.suffixes, ["o=Acme", "dc=x,dc=y"]);
        assert_eq!(r.indexes, ["telephoneNumber", "ou", "l"]);
        assert!(s.read_only && !s.allow_anon);
        assert_eq!(s.security, "SSLTLS");
        assert_eq!(s.psearch_max, 7);
        assert!(r.audit.wants(AuditOp::Add, true));
        assert!(!r.audit.wants(AuditOp::Modify, false));
    }

    #[test]
    fn refused_values() {
        let mut c = shipped();
        c.set(CONFIGURATION, "ibm-slapdPwEncryption", &["aes256"])
            .unwrap();
        assert_eq!(
            runtime(&c, "QUSRDIR", "V1R2M0").unwrap_err().msgid(),
            "GLD0232"
        );
        let mut c = shipped();
        c.set(SYSTEM, "ibm-slapdOs400UsrprjRead", &["FALSE"])
            .unwrap();
        assert_eq!(
            runtime(&c, "QUSRDIR", "V1R2M0").unwrap_err(),
            ConfigError::BadValue {
                attr: "ibm-slapdOs400UsrprjRead".into(),
                value: "FALSE".into()
            }
        );
        let mut c = shipped();
        c.set(CONFIGURATION, "ibm-slapdCryptoNonsense", &["1"]).ok();
        let t = format!(
            "{}\ndn: cn=Bogus, cn=Configuration\ncn: Bogus\nibm-bogus: 1\n",
            c.text()
        );
        let c = SlapdConf::parse(PATH, &t).unwrap();
        assert_eq!(
            runtime(&c, "QUSRDIR", "V1R2M0").unwrap_err().msgid(),
            "GLD0126"
        );
    }
}
