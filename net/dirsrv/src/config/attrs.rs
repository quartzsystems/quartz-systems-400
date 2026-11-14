// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The attributes of `ibmslapd.conf` QS/400 knows (docs/DIRSRV.md
//! §10): the ones it honours, each with the values it takes, and IBM's
//! others, taken only at the default the primary's `QUSRDIR` has
//! (`tools/conformance/oracle/dirsrv/ibmslapd-shape.ldif`). Anything
//! else stops the instance's start with `GLD0126`.

/// What a value must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Any text.
    Any,
    /// `TRUE` or `FALSE` (any case).
    Bool,
    /// A whole number in the range.
    Number(u64, u64),
    /// One of these (any case).
    OneOf(&'static [&'static str]),
    /// A distinguished name.
    Dn,
}

/// What QS/400 does with an attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// Honoured, with values that pass the check.
    Honoured(Check),
    /// IBM's, taken only at one of these values (any case);
    /// `{instance}` stands for the instance's name.
    Default(&'static [&'static str]),
    /// IBM's, whose value IBM's server makes up or keeps secret (a
    /// server ID, a key database password): taken as it is, used for
    /// nothing.
    Kept,
}

use Check::{Any, Bool, Dn, Number, OneOf};
use Rule::{Default, Honoured, Kept};

const N: Check = Number(0, u32::MAX as u64);

/// Every attribute, by name (compared ignoring ASCII case).
pub static ATTRS: &[(&str, Rule)] = &[
    ("objectclass", Honoured(Any)),
    ("cn", Honoured(Any)),
    // cn=Configuration
    ("ibm-slapdAdminDN", Honoured(Dn)),
    ("ibm-slapdAdminPW", Honoured(Any)),
    ("ibm-slapdPort", Honoured(Number(1, 65535))),
    ("ibm-slapdSizeLimit", Honoured(N)),
    ("ibm-slapdTimeLimit", Honoured(N)),
    (
        "ibm-slapdDerefAliases",
        Honoured(OneOf(&["never", "search", "find", "always"])),
    ),
    (
        "ibm-slapdPwEncryption",
        Honoured(OneOf(&[
            "none", "crypt", "sha", "ssha", "ssha256", "ssha512",
        ])),
    ),
    ("ibm-slapdServerId", Kept),
    ("ibm-slapdAdminGroupEnabled", Default(&["false"])),
    ("ibm-slapdAuthIntegration", Default(&["0"])),
    ("ibm-slapdCryptoSync", Kept),
    ("ibm-slapdLogMessages", Default(&["TRUE"])),
    ("ibm-slapdPtaEnabled", Default(&["false"])),
    ("ibm-slapdSupportedWebAdmVersion", Default(&["3.0"])),
    ("ibm-slapdVersion", Default(&["6.1"])),
    // QS/400's own (docs/DIRSRV.md §5, §8, §7).
    ("ibm-slapdIndex", Honoured(Any)),
    ("ibm-slapdChangeLogMaxEntries", Honoured(N)),
    ("ibm-slapdReferral", Honoured(Any)),
    // cn=Front End, cn=Connection Management
    ("ibm-slapdIdleTimeOut", Honoured(N)),
    ("ibm-slapdWriteTimeout", Honoured(N)),
    ("ibm-slapdAllowAnon", Honoured(Bool)),
    ("ibm-slapdACLCache", Default(&["TRUE"])),
    ("ibm-slapdACLCacheSize", Default(&["25000"])),
    ("ibm-slapdEntryCacheSize", Default(&["25000"])),
    ("ibm-slapdFilterCacheBypassLimit", Default(&["100"])),
    ("ibm-slapdFilterCacheSize", Default(&["25000"])),
    ("ibm-slapdAllReapingThreshold", Default(&["1200"])),
    ("ibm-slapdAnonReapingThreshold", Default(&["0"])),
    ("ibm-slapdBoundReapingThreshold", Default(&["1100"])),
    // cn=Digest, cn=Event Notification
    ("ibm-slapdDigestEnabled", Default(&["true"])),
    ("ibm-slapdEnableEventNotification", Default(&["FALSE"])),
    ("ibm-slapdMaxEventsPerConnection", Default(&["0"])),
    ("ibm-slapdMaxEventsTotal", Default(&["0"])),
    // cn=Log Management and its children; the audit switches (§12).
    ("ibm-slapdLogOptions", Honoured(Any)),
    ("ibm-audit", Honoured(Bool)),
    ("ibm-auditAdd", Honoured(Bool)),
    ("ibm-auditAttributesOnGroupEvalOp", Honoured(Bool)),
    ("ibm-auditBind", Honoured(Bool)),
    ("ibm-auditCompare", Honoured(Bool)),
    ("ibm-auditDelete", Honoured(Bool)),
    ("ibm-auditExtOp", Honoured(Bool)),
    ("ibm-auditExtOPEvent", Honoured(Bool)),
    ("ibm-auditFailedOPonly", Honoured(Bool)),
    ("ibm-auditGroupsOnGroupControl", Honoured(Bool)),
    ("ibm-auditModify", Honoured(Bool)),
    ("ibm-auditModifyDN", Honoured(Bool)),
    ("ibm-auditPerformance", Honoured(Bool)),
    ("ibm-auditPTABindInfo", Honoured(Bool)),
    ("ibm-auditSearch", Honoured(Bool)),
    ("ibm-auditUnbind", Honoured(Bool)),
    ("ibm-auditVersion", Default(&["3"])),
    (
        "ibm-slapdLog",
        Default(&[
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/logs/audit.log",
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/logs/lostandfound.log",
        ]),
    ),
    (
        "ibm-slapdLogArchivePath",
        Default(&["/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/logs"]),
    ),
    ("ibm-slapdLogMaxArchives", Default(&["3"])),
    ("ibm-slapdLogSizeThreshold", Default(&["10"])),
    // cn=pwdPolicy Admin (§11)
    ("ibm-slapdConfigPwdPolicyOn", Honoured(Bool)),
    ("passwordMaxRepeatedChars", Honoured(N)),
    ("passwordMinAlphaChars", Honoured(N)),
    ("passwordMinDiffChars", Honoured(N)),
    ("passwordMinOtherChars", Honoured(N)),
    ("pwdAllowUserChange", Honoured(Bool)),
    ("pwdCheckSyntax", Honoured(Number(0, 2))),
    ("pwdExpireWarning", Honoured(N)),
    ("pwdFailureCountInterval", Honoured(N)),
    ("pwdGraceLoginLimit", Honoured(N)),
    ("pwdInHistory", Honoured(N)),
    ("pwdLockout", Honoured(Bool)),
    ("pwdLockoutDuration", Honoured(N)),
    ("pwdMaxAge", Honoured(N)),
    ("pwdMaxFailure", Honoured(N)),
    ("pwdMinAge", Honoured(N)),
    ("pwdMinLength", Honoured(N)),
    ("pwdMustChange", Honoured(Bool)),
    ("pwdSafeModify", Honoured(Bool)),
    // cn=Replication: refused (§14), taken at IBM's defaults.
    ("ibm-slapdMaxPendingChangesDisplayed", Default(&["200"])),
    ("ibm-slapdReplContextCacheSize", Default(&["100000"])),
    ("ibm-slapdReplMaxErrors", Default(&["0"])),
    ("ibm-slapdReplRestrictedAccess", Default(&["false"])),
    // cn=Master Server: the update DN CHGDIRSVRA *UPDATEDN changes.
    ("ibm-slapdMasterDN", Honoured(Dn)),
    ("ibm-slapdMasterPW", Honoured(Any)),
    // cn=Schemas and the backends
    (
        "ibm-slapdIncludeSchema",
        Default(&[
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.config.at",
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.config.oc",
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.ibm.at",
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.ibm.oc",
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.system.at",
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.system.oc",
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.user.at",
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.user.oc",
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.ldapsyntaxes",
            "/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.matchingrules",
        ]),
    ),
    (
        "ibm-slapdSchemaAdditions",
        Default(&["/QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.modifiedschema"]),
    ),
    (
        "ibm-slapdSchemaCheck",
        Honoured(OneOf(&["V3", "V3_lenient"])),
    ),
    ("ibm-slapdSuffix", Honoured(Dn)),
    ("ibm-slapdReadOnly", Honoured(Bool)),
    (
        "ibm-slapdPlugin",
        Default(&[
            "database /QSYS.LIB/QGLDBCFG.SRVPGM config_backend_init",
            "audit /QSYS.LIB/QGLDAUDIT.SRVPGM audit_init",
            "database /QSYS.LIB/QGLDRDBM.SRVPGM rdbm_backend_init",
            "replication /QSYS.LIB/QGLDREPL.SRVPGM replInit",
            "database /QSYS.LIB/QGLDPSYS.SRVPGM sysprj_backend_init",
        ]),
    ),
    ("ibm-slapdCachedAttributeAutoAdjust", Default(&["FALSE"])),
    (
        "ibm-slapdCachedAttributeAutoAdjustTime",
        Default(&["T000000"]),
    ),
    (
        "ibm-slapdCachedAttributeAutoAdjustTimeInterval",
        Default(&["2"]),
    ),
    ("ibm-slapdCachedAttributeSize", Default(&["16384"])),
    ("ibm-slapdDbConnections", Default(&["4"])),
    ("ibm-slapdDbInstance", Default(&["/QSYS.LIB/QUSRDIRDB.LIB"])),
    ("ibm-slapdDbName", Default(&["*SYSTEM"])),
    ("ibm-slapdDbUserID", Default(&["none"])),
    ("ibm-slapdDbUserPW", Kept),
    ("ibm-slapdEnableRemotePWPExOps", Kept),
    ("ibm-slapdGroupMembersCacheBypassLimit", Default(&["25000"])),
    ("ibm-slapdGroupMembersCacheSize", Default(&["25"])),
    ("ibm-slapdLanguageTagsEnabled", Default(&["FALSE"])),
    ("ibm-slapdNumRetry", Default(&["5"])),
    ("ibm-slapdPagedResAllowNonAdmin", Honoured(Bool)),
    ("ibm-slapdPagedResLmt", Honoured(N)),
    ("ibm-slapdrepldbconns", Default(&["4"])),
    ("ibm-slapdSortKeyLimit", Honoured(N)),
    ("ibm-slapdSortSrchAllowNonAdmin", Honoured(Bool)),
    ("ibm-slapdOs400UsrprjRead", Honoured(Bool)),
    // cn=SSL: the certificate is DCM's assignment (§4).
    ("ibm-slapdSecurePort", Honoured(Number(1, 65535))),
    (
        "ibm-slapdSecurity",
        Honoured(OneOf(&["none", "SSL", "TLS", "SSLTLS"])),
    ),
    ("ibm-slapdSslAuth", Default(&["serverauth"])),
    ("ibm-slapdSslCertificate", Default(&["none"])),
    ("ibm-slapdSslCipherSpecs", Default(&["0"])),
    ("ibm-slapdSslKeyDatabase", Default(&["none"])),
    ("ibm-slapdSslKeyDatabasePW", Kept),
    // cn=Transaction
    ("ibm-slapdMaxNumOfTransactions", Default(&["20"])),
    ("ibm-slapdMaxOpPerTransaction", Default(&["5"])),
    ("ibm-slapdmaxtimebetweenprepareandcommit", Default(&["2"])),
    ("ibm-slapdMaxTimeLimitOfTransactions", Default(&["300"])),
    ("ibm-slapdTransactionEnable", Default(&["TRUE"])),
    // cn=Persistent Search
    ("ibm-slapdEnablePersistentSearch", Honoured(Bool)),
    ("ibm-slapdMaxPersistentSearches", Honoured(N)),
];

/// The rule of an attribute; `None` for one QS/400 does not know.
#[must_use]
pub fn rule(name: &str) -> Option<Rule> {
    // An attribute description's options (`;binary`) do not count.
    let base = name.split(';').next().unwrap_or(name);
    ATTRS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(base))
        .map(|(_, r)| *r)
}

/// Whether `value` passes `check`.
#[must_use]
pub fn passes(check: Check, value: &str) -> bool {
    let v = value.trim();
    match check {
        Check::Any => true,
        Check::Bool => v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("false"),
        Check::Number(lo, hi) => v.parse::<u64>().is_ok_and(|n| (lo..=hi).contains(&n)),
        Check::OneOf(set) => set.iter().any(|s| s.eq_ignore_ascii_case(v)),
        Check::Dn => ldap::Dn::parse(v).is_ok(),
    }
}

/// Whether `value` is one of a [`Rule::Default`]'s values for instance
/// `instance`.
#[must_use]
pub fn is_default(values: &[&str], instance: &str, value: &str) -> bool {
    values.iter().any(|d| {
        d.replace("{instance}", instance)
            .eq_ignore_ascii_case(value.trim())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        assert_eq!(rule("IBM-SLAPDPORT"), Some(Honoured(Number(1, 65535))));
        assert_eq!(rule("ibm-slapdAdminPW;binary"), Some(Honoured(Any)));
        assert!(rule("ibm-slapdNoSuch").is_none());
        assert!(passes(Bool, "true") && !passes(Bool, "yes"));
        assert!(passes(Number(1, 65535), "389") && !passes(Number(1, 65535), "0"));
        assert!(!passes(OneOf(&["V3", "V3_lenient"]), "V2"));
        assert!(is_default(
            &["/a/idsslapd-{instance}/logs"],
            "QUSRDIR",
            "/A/IDSSLAPD-QUSRDIR/LOGS"
        ));
        // Every name once.
        for (i, (n, _)) in ATTRS.iter().enumerate() {
            assert!(
                !ATTRS[..i].iter().any(|(m, _)| m.eq_ignore_ascii_case(n)),
                "{n} twice"
            );
        }
    }
}
