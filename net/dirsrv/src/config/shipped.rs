// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The files a system starts with (docs/DIRSRV.md §10): the instance
//! list with `QUSRDIR`, and `QUSRDIR`'s `ibmslapd.conf` with IBM's
//! entries and the values the primary's has, except
//! `ibm-slapdPwEncryption: ssha512`, no administrator password, no
//! server ID (the server makes one at its first start), the projected
//! suffix named after the system, and no user suffix (one is added with
//! `ibm-slapdSuffix` on `cn=Directory`). The comments are Quartz's.

/// The instance list's text.
pub const INSTANCES: &str = "\
# The directory server instances of this system. Each entry names an
# instance (cn), whether STRTCPSVR SERVER(*AUTOSTART) starts it
# (ids-instanceAutostart 1) and the directory its idsslapd-NAME is in.
charset: ISO-8859-1
version: 1

dn: CN=IDS INSTANCES
cn: IDS INSTANCES
objectClass: TOP
objectClass: CONTAINER

dn: cn=QUSRDIR, CN=IDS INSTANCES
cn: QUSRDIR
ids-instanceAutostart: 1
ids-instanceDesc: QUSRDIR
ids-instanceLocation: /QIBM/UserData/OS400/DirSrv
ids-instanceVersion: 6.1
objectClass: TOP
objectClass: ids-instance
";

/// The configuration file of a new instance `instance` on a system
/// whose DNS name (or, with none, its system name) is `system`.
#[must_use]
pub fn slapd_conf(instance: &str, system: &str) -> String {
    TEMPLATE
        .replace("{instance}", instance)
        .replace("{system}", system)
}

const TEMPLATE: &str = "\
# The configuration of directory server instance {instance}, read when
# the instance starts. CHGDIRSVRA and the cn=configuration suffix
# change it; a change takes effect at the next start, except the
# administrator's password, the limits and logging, which take effect
# at once. An attribute QS/400 does not honour may stay here only at the
# value shipped; anything else stops the start with message GLD0126.
version: 1

# The administrator, the port and the limits.
dn: cn=Configuration
cn: Configuration
ibm-slapdAdminDN: cn=Administrator
ibm-slapdAdminGroupEnabled: false
ibm-slapdAuthIntegration: 0
ibm-slapdDerefAliases: always
ibm-slapdLogMessages: TRUE
ibm-slapdPort: 389
ibm-slapdPtaEnabled: false
ibm-slapdPwEncryption: ssha512
ibm-slapdSizeLimit: 500
ibm-slapdSupportedWebAdmVersion: 3.0
ibm-slapdTimeLimit: 900
ibm-slapdVersion: 6.1
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdTop
objectclass: ibm-slapdOs400Top

dn: cn=AdminGroup, cn=Configuration
cn: AdminGroup
objectclass: top
objectclass: container

dn: cn=Front End, cn=Configuration
cn: Front End
ibm-slapdACLCache: TRUE
ibm-slapdACLCacheSize: 25000
ibm-slapdEntryCacheSize: 25000
ibm-slapdFilterCacheBypassLimit: 100
ibm-slapdFilterCacheSize: 25000
ibm-slapdIdleTimeOut: 300
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdFrontEnd

# Connections: anonymous binds, idle and write time-outs (seconds).
dn: cn=Connection Management, cn=Front End, cn=Configuration
cn: Connection Management
ibm-slapdAllowAnon: TRUE
ibm-slapdAllReapingThreshold: 1200
ibm-slapdAnonReapingThreshold: 0
ibm-slapdBoundReapingThreshold: 1100
ibm-slapdIdleTimeOut: 300
ibm-slapdWriteTimeout: 120
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdConnectionManagement

dn: cn=Digest, cn=Configuration
cn: Digest
ibm-slapdDigestEnabled: true
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdDigest

dn: cn=Event Notification, cn=Configuration
cn: Event Notification
ibm-slapdEnableEventNotification: FALSE
ibm-slapdMaxEventsPerConnection: 0
ibm-slapdMaxEventsTotal: 0
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdEventNotification

dn: cn=Log Management, cn=Configuration
cn: Log Management
objectclass: top
objectclass: container

# Auditing: with ibm-audit TRUE the operations switched on here are
# written to the QAUDJRN journal.
dn: cn=Audit, cn=Log Management, cn=Configuration
cn: audit
ibm-audit: false
ibm-auditAdd: false
ibm-auditAttributesOnGroupEvalOp: false
ibm-auditBind: true
ibm-auditCompare: false
ibm-auditDelete: false
ibm-auditExtOp: false
ibm-auditExtOPEvent: false
ibm-auditFailedOPonly: true
ibm-auditGroupsOnGroupControl: false
ibm-auditModify: false
ibm-auditModifyDN: false
ibm-auditPerformance: false
ibm-auditPTABindInfo: true
ibm-auditSearch: false
ibm-auditUnbind: true
ibm-auditVersion: 3
ibm-slapdLog: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/logs/audit.log
ibm-slapdLogArchivePath: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/logs
ibm-slapdLogMaxArchives: 3
ibm-slapdLogSizeThreshold: 10
objectclass: ibm-auditConfig
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdLogConfig
objectclass: top

dn: cn=Default, cn=Log Management, cn=Configuration
cn: Default
objectclass: ibm-slapdLogConfig
objectclass: ibm-slapdConfigEntry
objectclass: top
objectclass: container

dn: cn=ibmslapd, cn=Log Management, cn=Configuration
cn: ibmslapd
ibm-slapdLogOptions: m
objectclass: ibm-slapdLogConfig
objectclass: ibm-slapdConfigEntry
objectclass: top
objectclass: container

dn: cn=Replication, cn=Log Management, cn=Configuration
cn: Replication
ibm-slapdLog: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/logs/lostandfound.log
ibm-slapdLogArchivePath: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/logs
ibm-slapdLogMaxArchives: 3
ibm-slapdLogSizeThreshold: 10
objectclass: ibm-slapdLogConfig
objectclass: ibm-slapdConfigEntry
objectclass: top
objectclass: container

# Password policy for entries of the directory: off until
# ibm-slapdConfigPwdPolicyOn is TRUE.
dn: cn=pwdPolicy Admin, cn=configuration
cn: pwdPolicy Admin
ibm-slapdConfigPwdPolicyOn: FALSE
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdPwdPolicyAdmin

dn: cn=Replication, cn=configuration
cn: Replication
ibm-slapdMaxPendingChangesDisplayed: 200
ibm-slapdReplContextCacheSize: 100000
ibm-slapdReplMaxErrors: 0
ibm-slapdReplRestrictedAccess: false
objectclass: ibm-slapdReplicationConfiguration
objectclass: ibm-slapdConfigEntry
objectclass: top

dn: cn=Schemas, cn=Configuration
cn: Schemas
objectclass: top
objectclass: container

# The schema: QS/400's own, with the changes made through cn=schema in
# V3.modifiedschema. Schema checking V3_lenient or V3.
dn: cn=IBM Directory, cn=Schemas, cn=Configuration
cn: IBM Directory
ibm-slapdIncludeSchema: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.config.at
ibm-slapdIncludeSchema: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.config.oc
ibm-slapdIncludeSchema: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.ibm.at
ibm-slapdIncludeSchema: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.ibm.oc
ibm-slapdIncludeSchema: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.system.at
ibm-slapdIncludeSchema: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.system.oc
ibm-slapdIncludeSchema: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.user.at
ibm-slapdIncludeSchema: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.user.oc
ibm-slapdIncludeSchema: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.ldapsyntaxes
ibm-slapdIncludeSchema: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.matchingrules
ibm-slapdSchemaAdditions: /QIBM/UserData/OS400/DirSrv/idsslapd-{instance}/etc/V3.modifiedschema
ibm-slapdSchemaCheck: V3_lenient
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdSchema

dn: cn=Config Backends, cn=IBM Directory, cn=Schemas, cn=Configuration
cn: Config Backends
objectclass: top
objectclass: container

dn: cn=ConfigDB, cn=Config Backends, cn=IBM Directory, cn=Schemas, cn=Configuration
cn: ConfigDB
ibm-slapdPlugin: database /QSYS.LIB/QGLDBCFG.SRVPGM config_backend_init
ibm-slapdSuffix: cn=configuration
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdConfigBackend

dn: cn=LDCF Backends, cn=IBM Directory, cn=Schemas, cn=Configuration
cn: LDCF Backends
objectclass: top
objectclass: container

dn: cn=SchemaDB, cn=LDCF Backends, cn=IBM Directory, cn=Schemas, cn=Configuration
cn: SchemaDB
ibm-slapdPlugin: audit /QSYS.LIB/QGLDAUDIT.SRVPGM audit_init
ibm-slapdReadOnly: FALSE
ibm-slapdSuffix: cn=schema
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdLdcfBackend

dn: cn=RDBM Backends, cn=IBM Directory, cn=Schemas, cn=Configuration
cn: RDBM Backends
objectclass: top
objectclass: container

# The entries of the directory, kept in QUSRDIRDB/{instance}. Add a
# suffix of your own with another ibm-slapdSuffix line here.
dn: cn=Directory, cn=RDBM Backends, cn=IBM Directory, cn=Schemas, cn=Configuration
cn: Directory
ibm-slapdCachedAttributeAutoAdjust: FALSE
ibm-slapdCachedAttributeAutoAdjustTime: T000000
ibm-slapdCachedAttributeAutoAdjustTimeInterval: 2
ibm-slapdCachedAttributeSize: 16384
ibm-slapdDbConnections: 4
ibm-slapdDbInstance: /QSYS.LIB/QUSRDIRDB.LIB
ibm-slapdDbName: *SYSTEM
ibm-slapdDbUserID: none
ibm-slapdGroupMembersCacheBypassLimit: 25000
ibm-slapdGroupMembersCacheSize: 25
ibm-slapdLanguageTagsEnabled: FALSE
ibm-slapdNumRetry: 5
ibm-slapdPagedResAllowNonAdmin: TRUE
ibm-slapdPagedResLmt: 2
ibm-slapdPlugin: database /QSYS.LIB/QGLDRDBM.SRVPGM rdbm_backend_init
ibm-slapdPlugin: replication /QSYS.LIB/QGLDREPL.SRVPGM replInit
ibm-slapdReadOnly: FALSE
ibm-slapdrepldbconns: 4
ibm-slapdSortKeyLimit: 3
ibm-slapdSortSrchAllowNonAdmin: TRUE
ibm-slapdSuffix: cn=localhost
ibm-slapdSuffix: cn=ibmpolicies
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdRdbmBackend

dn: cn=System Backends, cn=IBM Directory, cn=Schemas, cn=Configuration
cn: System Backends
objectclass: top
objectclass: container

# The system's user profiles, read only, under os400-sys={system}.
dn: cn=System, cn=System Backends, cn=IBM Directory, cn=Schemas, cn=Configuration
cn: System
ibm-slapdOs400UsrprjRead: TRUE
ibm-slapdPlugin: database /QSYS.LIB/QGLDPSYS.SRVPGM sysprj_backend_init
ibm-slapdReadOnly: FALSE
ibm-slapdSuffix: os400-sys={system}
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdOs400SystemBackend

# TLS: port 636 and StartTLS work once a certificate is assigned to the
# directory server's application ID in the certificate store.
dn: cn=SSL, cn=Configuration
cn: SSL
ibm-slapdSecurePort: 636
ibm-slapdSecurity: none
ibm-slapdSslAuth: serverauth
ibm-slapdSslCertificate: none
ibm-slapdSslCipherSpecs: 0
ibm-slapdSslKeyDatabase: none
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdSSL

dn: cn=Transaction, cn=Configuration
cn: Transaction
ibm-slapdMaxNumOfTransactions: 20
ibm-slapdMaxOpPerTransaction: 5
ibm-slapdmaxtimebetweenprepareandcommit: 2
ibm-slapdMaxTimeLimitOfTransactions: 300
ibm-slapdTransactionEnable: TRUE
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdTransaction

dn: cn=Persistent Search, cn=Configuration
cn: Persistent Search
ibm-slapdEnablePersistentSearch: TRUE
ibm-slapdMaxPersistentSearches: 100
objectclass: top
objectclass: ibm-slapdConfigEntry
objectclass: ibm-slapdPersistentSearch

dn: cn=Passthrough Authentication, cn=Configuration
cn: Passthrough Authentication
objectclass: top
objectclass: container
";

#[cfg(test)]
mod tests {
    use super::super::{InstanceList, SlapdConf, conf_path};
    use super::*;

    #[test]
    fn the_shipped_files_read_back() {
        let l = InstanceList::parse(INSTANCES).unwrap();
        let q = l.find("QUSRDIR").unwrap();
        assert!(q.autostart);
        let t = slapd_conf("QUSRDIR", "MYSYS.EXAMPLE.COM");
        let c = SlapdConf::parse(&conf_path(&q.location, &q.name), &t).unwrap();
        assert_eq!(c.validate("QUSRDIR"), Ok(()));
        assert_eq!(c.admin_pw(), None);
        assert_eq!(
            c.get("cn=Configuration", "ibm-slapdPwEncryption")
                .as_deref(),
            Some("ssha512")
        );
        assert_eq!(
            c.values(
                "cn=System, cn=System Backends, cn=IBM Directory, cn=Schemas, cn=Configuration",
                "ibm-slapdSuffix"
            ),
            ["os400-sys=MYSYS.EXAMPLE.COM"]
        );
        assert!(!t.contains('\r') && !t.contains("{instance}"));
    }

    /// Every entry of the primary's file is in the shipped one, in the
    /// same order (cn=Master Server and the user suffix aside).
    #[test]
    fn the_primarys_entries_in_its_order() {
        let primary =
            include_str!("../../../../tools/conformance/oracle/dirsrv/ibmslapd-shape.ldif");
        let dns = |t: &str| -> Vec<String> {
            t.lines()
                .filter_map(|l| l.strip_prefix("dn: "))
                .map(super::super::dn_key)
                .collect()
        };
        assert_eq!(dns(primary), dns(&slapd_conf("QUSRDIR", "S")));
    }
}
