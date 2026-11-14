// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server's administration (docs/DIRSRV.md §2, §4 request
//! 1568 `dir_admin`): what Navigator's Directory server pages and QAP's
//! `Dirsrv` service ask, as one trait, [`DirAdmin`].
//!
//! - [`conf`]: the questions answered from an instance's
//!   `ibmslapd.conf` ([`crate::config::SlapdConf`]): properties,
//!   suffixes, indexes, the administrator; what the storage service and
//!   [`MemAdmin`] share.
//! - [`access`]: `aclEntry` and `entryOwner` values checked.
//! - [`publish`]: the publishing agents over `publish.conf`.
//! - [`schema`]: pages of the schema and its changes.
//! - [`mem`]: [`MemAdmin`], a directory server in memory for tests and
//!   the fixtures.
//!
//! Every method takes the signed-on profile; reads need `*IOSYSCFG`,
//! changes `*ALLOBJ` and `*IOSYSCFG` as `CHGDIRSVRA` does (`CPF222E`
//! naming the first missing). The types cross the storage service's
//! link (`stg::diradmin`), so each is `ipc` wire.

pub mod access;
pub mod conf;
pub mod mem;
pub mod publish;
pub mod schema;

use ipc::{wire_enum, wire_struct};

use crate::config::ConfigError;

pub use mem::MemAdmin;

/// What kind of refusal an [`AdminError`] is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ErrorKind {
    /// The profile lacks an authority.
    NotAuthorized,
    /// No such instance, entry, suffix, index or definition.
    NotFound,
    /// A value that is not valid.
    #[default]
    Invalid,
    /// Already there.
    Exists,
    /// Not available now (the instance is not running, or the function
    /// is not built yet).
    Unavailable,
    /// Anything else (a file that cannot be read or written).
    Failed,
}

wire_enum!(ErrorKind {
    0 => NotAuthorized,
    1 => NotFound,
    2 => Invalid,
    3 => Exists,
    4 => Unavailable,
    5 => Failed,
});

/// A refusal, as the message the system sends for it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdminError {
    /// What kind.
    pub kind: ErrorKind,
    /// The message identifier (`GLD0201`, `CPF222E`, `CPF9898`).
    pub msgid: String,
    /// Its substitution values, as text, in the message's order.
    pub data: Vec<String>,
    /// Its first-level text, the values substituted.
    pub text: String,
}

wire_struct!(AdminError {
    kind,
    msgid,
    data,
    text
});

impl AdminError {
    /// A refusal with `CPF9898`, whose text is its one value.
    #[must_use]
    pub fn plain(kind: ErrorKind, text: &str) -> Self {
        Self {
            kind,
            msgid: "CPF9898".into(),
            data: vec![text.into()],
            text: text.into(),
        }
    }

    /// `CPF222E`: special authority `what` is needed.
    #[must_use]
    pub fn special(what: &str) -> Self {
        Self {
            kind: ErrorKind::NotAuthorized,
            msgid: "CPF222E".into(),
            data: vec![what.into()],
            text: format!("Special authority {what} is needed for this."),
        }
    }
}

impl std::fmt::Display for AdminError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.msgid, self.text)
    }
}

impl std::error::Error for AdminError {}

impl From<ConfigError> for AdminError {
    fn from(e: ConfigError) -> Self {
        let kind = match e {
            ConfigError::NoInstance(_) | ConfigError::NoServerInstance(_) => ErrorKind::NotFound,
            ConfigError::BadValue { .. }
            | ConfigError::BadDn(_)
            | ConfigError::NoPassword
            | ConfigError::NoUpdateDn(_) => ErrorKind::Invalid,
            ConfigError::File { .. } | ConfigError::Unsupported { .. } => ErrorKind::Failed,
        };
        let data = e
            .vars()
            .into_iter()
            .map(|v| match v {
                crate::config::Var::Chars(t, _) | crate::config::Var::Vary(t) => t,
            })
            .collect();
        let mut text = e.to_string();
        if let Some(c) = text.get(..1) {
            text = format!("{}{}.", c.to_ascii_uppercase(), &text[1..]);
        }
        Self {
            kind,
            msgid: e.msgid().into(),
            data,
            text,
        }
    }
}

/// An answer or a refusal.
pub type Result<T> = std::result::Result<T, AdminError>;

/// Whether an instance runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InstanceState {
    /// Its job is not active.
    #[default]
    Stopped,
    /// Its job is starting.
    Starting,
    /// It answers on its ports.
    Running,
    /// Its job is ending.
    Ending,
}

wire_enum!(InstanceState {
    0 => Stopped,
    1 => Starting,
    2 => Running,
    3 => Ending,
});

/// One instance of the instance list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceInfo {
    /// Its name (`QUSRDIR`).
    pub name: String,
    /// Its description.
    pub desc: String,
    /// Started with `INSTANCE(*AUTOSTART)`.
    pub autostart: bool,
    /// Whether it runs.
    pub state: InstanceState,
    /// Its job, `number/user/name`; empty when not active.
    pub job: String,
    /// `ibm-slapdPort`.
    pub port: u16,
    /// `ibm-slapdSecurePort`.
    pub secure_port: u16,
    /// `ibm-slapdSecurity`: `none`, `SSL`, `TLS`, `SSLTLS`.
    pub security: String,
    /// The certificate assigned to the application ID; empty for none.
    pub certificate: String,
    /// IBM's level it was made at (`6.1`).
    pub version: String,
    /// The directory its `idsslapd-NAME` is in.
    pub location: String,
    /// When its job started, seconds since 1970; 0 when not active.
    pub since: u64,
}

wire_struct!(InstanceInfo {
    name,
    desc,
    autostart,
    state,
    job,
    port,
    secure_port,
    security,
    certificate,
    version,
    location,
    since
});

/// The certificate store's application ID of the directory server.
pub const APP_ID: &str = "QS400_DIRECTORY_SERVER";

/// The server's properties: the honoured attributes of `ibmslapd.conf`
/// (docs/DIRSRV.md §10), as [`conf::PROPERTIES`] places them. Numbers
/// are `0` for no limit where IBM's are.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the switches are flags
pub struct Properties {
    /// `ibm-slapdPort`.
    pub port: u16,
    /// `ibm-slapdSecurePort`.
    pub secure_port: u16,
    /// `ibm-slapdSecurity`.
    pub security: String,
    /// `ibm-slapdSizeLimit`: entries a search returns.
    pub size_limit: u32,
    /// `ibm-slapdTimeLimit`: seconds a search runs.
    pub time_limit: u32,
    /// `ibm-slapdIdleTimeOut`: seconds an idle connection stays.
    pub idle_timeout: u32,
    /// `ibm-slapdWriteTimeout`: seconds a write to a client may wait.
    pub write_timeout: u32,
    /// `ibm-slapdAllowAnon`.
    pub allow_anon: bool,
    /// `ibm-slapdDerefAliases`: `never`, `search`, `find`, `always`.
    pub deref_aliases: String,
    /// `ibm-slapdSchemaCheck`: `V3` or `V3_lenient`.
    pub schema_check: String,
    /// `ibm-slapdPwEncryption`.
    pub pw_encryption: String,
    /// `ibm-slapdReadOnly` of the directory's backend.
    pub read_only: bool,
    /// `ibm-slapdLogOptions`.
    pub log_options: String,
    /// `ibm-audit`: operations audited to `QAUDJRN`.
    pub audit: bool,
    /// `ibm-slapdEnablePersistentSearch`.
    pub persistent_search: bool,
    /// `ibm-slapdMaxPersistentSearches`.
    pub max_persistent: u32,
    /// `ibm-slapdPagedResAllowNonAdmin`.
    pub paged_non_admin: bool,
    /// `ibm-slapdPagedResLmt`: paged searches at once per connection.
    pub paged_limit: u32,
    /// `ibm-slapdSortSrchAllowNonAdmin`.
    pub sort_non_admin: bool,
    /// `ibm-slapdSortKeyLimit`.
    pub sort_key_limit: u32,
    /// `ibm-slapdChangeLogMaxEntries`; 0 for no change log.
    pub changelog_max: u32,
    /// `ibm-slapdConfigPwdPolicyOn`.
    pub pwd_policy: bool,
    /// `ibm-slapdOs400UsrprjRead`: the projected backend read only.
    pub projected_read_only: bool,
    /// `ibm-slapdReferral`: the default referrals, LDAP URLs.
    pub referrals: Vec<String>,
}

impl Default for Properties {
    /// IBM's values where the file has none.
    fn default() -> Self {
        Self {
            port: 389,
            secure_port: 636,
            security: "none".into(),
            size_limit: 500,
            time_limit: 900,
            idle_timeout: 300,
            write_timeout: 120,
            allow_anon: true,
            deref_aliases: "always".into(),
            schema_check: "V3_lenient".into(),
            pw_encryption: "ssha512".into(),
            read_only: false,
            log_options: "m".into(),
            audit: false,
            persistent_search: true,
            max_persistent: 100,
            paged_non_admin: true,
            paged_limit: 2,
            sort_non_admin: true,
            sort_key_limit: 3,
            changelog_max: 0,
            pwd_policy: false,
            projected_read_only: true,
            referrals: Vec::new(),
        }
    }
}

wire_struct!(Properties {
    port,
    secure_port,
    security,
    size_limit,
    time_limit,
    idle_timeout,
    write_timeout,
    allow_anon,
    deref_aliases,
    schema_check,
    pw_encryption,
    read_only,
    log_options,
    audit,
    persistent_search,
    max_persistent,
    paged_non_admin,
    paged_limit,
    sort_non_admin,
    sort_key_limit,
    changelog_max,
    pwd_policy,
    projected_read_only,
    referrals
});

/// Which backend a suffix belongs to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SuffixKind {
    /// A user suffix of the directory's backend: can be removed.
    #[default]
    User,
    /// `cn=localhost` or `cn=ibmpolicies`, which the server needs.
    Local,
    /// The projected backend's (`os400-sys=`).
    Projected,
    /// `cn=schema`.
    Schema,
    /// `cn=configuration`.
    Config,
}

wire_enum!(SuffixKind {
    0 => User,
    1 => Local,
    2 => Projected,
    3 => Schema,
    4 => Config,
});

/// One suffix.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suffix {
    /// The DN, as the file has it.
    pub dn: String,
    /// Its backend.
    pub kind: SuffixKind,
    /// Entries under it; `None` when not known (the instance is not
    /// running, or the backend is not the entry store).
    pub entries: Option<u64>,
}

wire_struct!(Suffix { dn, kind, entries });

/// One attribute index (docs/DIRSRV.md §5: `ibm-slapdIndex`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the kinds are flags
pub struct Index {
    /// The attribute type.
    pub attr: String,
    /// Presence keys.
    pub presence: bool,
    /// Equality keys.
    pub equality: bool,
    /// Substring keys.
    pub substring: bool,
    /// One of the six every store has: cannot be removed.
    pub always: bool,
}

wire_struct!(Index {
    attr,
    presence,
    equality,
    substring,
    always
});

/// The administrator and the update DN.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdminInfo {
    /// `ibm-slapdAdminDN`.
    pub dn: String,
    /// `ibm-slapdAdminPW` is set.
    pub password_set: bool,
    /// `ibm-slapdMasterDN`, when the file names one.
    pub update_dn: Option<String>,
}

wire_struct!(AdminInfo {
    dn,
    password_set,
    update_dn
});

/// What a change of the administrator changes: `CHGDIRSVRA DN`'s first
/// element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminChange {
    /// `*ADMINDN`: the administrator's password.
    Password,
    /// A DN: the administrator and its password.
    Dn {
        /// The new administrator.
        dn: String,
    },
    /// `*UPDATEDN`: the update DN's password.
    UpdatePassword,
}

wire_enum!(AdminChange {
    0 => Password,
    1 => Dn { dn },
    2 => UpdatePassword,
});

/// Attribute types or object classes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SchemaKind {
    /// Attribute types.
    #[default]
    Attribute,
    /// Object classes.
    Class,
}

wire_enum!(SchemaKind {
    0 => Attribute,
    1 => Class,
});

/// A search of the schema.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaQuery {
    /// Attribute types or object classes.
    pub kind: SchemaKind,
    /// Only those with a name or OID containing this (case ignored);
    /// empty for all.
    pub contains: String,
    /// Only those added since the schema was shipped.
    pub custom_only: bool,
    /// The first to return, from 0.
    pub start: u32,
    /// At most this many; 0 for all.
    pub max: u32,
}

wire_struct!(SchemaQuery {
    kind,
    contains,
    custom_only,
    start,
    max
});

/// One definition.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // flags
pub struct SchemaDef {
    /// Attribute type or object class.
    pub kind: SchemaKind,
    /// The OID.
    pub oid: String,
    /// Its names, first the primary.
    pub names: Vec<String>,
    /// Its description; empty for none.
    pub desc: String,
    /// The supertype or superclasses.
    pub sup: Vec<String>,
    /// An attribute type's syntax OID, with `{length}` when bounded.
    pub syntax: String,
    /// An attribute type is single-valued.
    pub single_value: bool,
    /// An attribute type's access class (`normal`, `sensitive` ...).
    pub access: String,
    /// An object class's kind: `STRUCTURAL`, `AUXILIARY`, `ABSTRACT`.
    pub class_kind: String,
    /// An object class's required attributes, inherited ones too.
    pub must: Vec<String>,
    /// Its allowed attributes, inherited ones too.
    pub may: Vec<String>,
    /// Added or changed since the schema was shipped.
    pub custom: bool,
    /// The server relies on it: it cannot be removed.
    pub protected: bool,
    /// The RFC 4512 description, as `cn=schema` publishes it.
    pub text: String,
}

wire_struct!(SchemaDef {
    kind,
    oid,
    names,
    desc,
    sup,
    syntax,
    single_value,
    access,
    class_kind,
    must,
    may,
    custom,
    protected,
    text
});

/// A page of definitions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaPage {
    /// How many match in all.
    pub total: u32,
    /// The page, by primary name.
    pub defs: Vec<SchemaDef>,
}

wire_struct!(SchemaPage { total, defs });

/// An entry's access control (docs/DIRSRV.md §12).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // flags
pub struct Access {
    /// The entry.
    pub dn: String,
    /// The entry has its own `aclEntry` values; otherwise it inherits.
    pub explicit_acl: bool,
    /// `aclEntry`, the entry's own or the inherited ones.
    pub acl: Vec<String>,
    /// `aclPropagate`.
    pub acl_propagate: bool,
    /// `aclSource`: the entry whose ACL is in effect, `default` for the
    /// default.
    pub acl_source: String,
    /// The entry has its own `entryOwner` values.
    pub explicit_owner: bool,
    /// `entryOwner`, own or inherited.
    pub owners: Vec<String>,
    /// `ownerPropagate`.
    pub owner_propagate: bool,
    /// `ownerSource`.
    pub owner_source: String,
}

wire_struct!(Access {
    dn,
    explicit_acl,
    acl,
    acl_propagate,
    acl_source,
    explicit_owner,
    owners,
    owner_propagate,
    owner_source
});

/// A publishing agent (docs/DIRSRV.md §10.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentKind {
    /// User profiles, as `inetOrgPerson` entries.
    #[default]
    Users,
    /// Printers with an output queue.
    Printers,
}

wire_enum!(AgentKind {
    0 => Users,
    1 => Printers,
});

impl AgentKind {
    /// Both, in `publish.conf`'s order.
    pub const ALL: [Self; 2] = [Self::Users, Self::Printers];

    /// Its name in `publish.conf` and on the pages.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Users => "Users",
            Self::Printers => "Printers",
        }
    }

    /// The parent DN's first RDN by default.
    #[must_use]
    pub fn default_ou(self) -> &'static str {
        match self {
            Self::Users => "ou=Users",
            Self::Printers => "ou=Printers",
        }
    }

    /// The agent's job (`QGLDPUBE` users, `QGLDPUBA` printers).
    #[must_use]
    pub fn job_name(self) -> &'static str {
        match self {
            Self::Users => "QGLDPUBE",
            Self::Printers => "QGLDPUBA",
        }
    }
}

/// One agent's configuration (`publish.conf`'s entry).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // flags
pub struct PublishAgent {
    /// Which agent.
    pub kind: AgentKind,
    /// It publishes.
    pub enabled: bool,
    /// The target server's host name; empty for this system's default
    /// instance.
    pub server: String,
    /// Its port.
    pub port: u16,
    /// TLS: LDAPS, or `StartTLS` with `start_tls`; plain LDAP otherwise.
    pub tls: bool,
    /// The DN it binds as; empty binds anonymously.
    pub bind_dn: String,
    /// A password for the bind DN is kept (ignored in a change).
    pub password_set: bool,
    /// The parent DN of the published entries; empty for the agent's
    /// default (`ou=Users` or `ou=Printers` over `cn=localhost`).
    pub parent_dn: String,
    /// With `tls`: `StartTLS` after connecting rather than LDAPS.
    pub start_tls: bool,
    /// The parent and its missing ancestors are added.
    pub create_parent: bool,
    /// Seconds before the first retry (10 to 86400).
    pub retry: u32,
    /// Seconds between full resyncs, 0 for none, else 60 or more.
    pub resync: u32,
    /// The `publisherName` value; empty for `QS400 ` and the system
    /// name.
    pub publisher: String,
}

wire_struct!(PublishAgent {
    kind,
    enabled,
    server,
    port,
    tls,
    bind_dn,
    password_set,
    parent_dn,
    start_tls,
    create_parent,
    retry,
    resync,
    publisher
});

/// One agent's status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentStatus {
    /// Which agent.
    pub kind: AgentKind,
    /// Its job, `number/user/name`; empty when not active.
    pub job: String,
    /// It is publishing.
    pub active: bool,
    /// Entries published since it started.
    pub published: u64,
    /// Entries it could not publish.
    pub failed: u64,
    /// When it last published, seconds since 1970; 0 for never.
    pub last: u64,
    /// The last message it logged (`GLD010B`), empty for none.
    pub msgid: String,
    /// That message's text.
    pub text: String,
}

wire_struct!(AgentStatus {
    kind,
    job,
    active,
    published,
    failed,
    last,
    msgid,
    text
});

/// The counters of `cn=monitor` an instance keeps.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counters {
    /// When it started, seconds since 1970.
    pub started: u64,
    /// Connections open now.
    pub connections: u32,
    /// Connections since it started.
    pub total_connections: u64,
    /// Operations started.
    pub ops_initiated: u64,
    /// Operations finished.
    pub ops_completed: u64,
    /// Binds.
    pub binds: u64,
    /// Searches.
    pub searches: u64,
    /// Compares.
    pub compares: u64,
    /// Adds.
    pub adds: u64,
    /// Modifies.
    pub modifies: u64,
    /// Deletes.
    pub deletes: u64,
    /// Modify DNs.
    pub modrdns: u64,
    /// Extended operations.
    pub extended: u64,
    /// Abandons.
    pub abandons: u64,
    /// Entries sent.
    pub entries_sent: u64,
    /// Persistent searches open now.
    pub persistent: u32,
}

wire_struct!(Counters {
    started,
    connections,
    total_connections,
    ops_initiated,
    ops_completed,
    binds,
    searches,
    compares,
    adds,
    modifies,
    deletes,
    modrdns,
    extended,
    abandons,
    entries_sent,
    persistent
});

/// What the administration surfaces ask of the directory server. `user`
/// is the signed-on profile; `instance` an instance's name (case
/// ignored).
pub trait DirAdmin {
    /// The instance list, in its order.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0127` when the list cannot be read.
    fn instances(&self, user: &str) -> Result<Vec<InstanceInfo>>;
    /// Whether `instance` starts with `INSTANCE(*AUTOSTART)`.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0201`.
    fn set_autostart(&self, user: &str, instance: &str, on: bool) -> Result<()>;
    /// Start `instance` (`STRTCPSVR SERVER(*DIRSRV) INSTANCE(name)`).
    ///
    /// # Errors
    /// `CPF222E`; `GLD0203`; `GLD0126` or `GLD0232` when its
    /// configuration stops the start.
    fn start(&self, user: &str, instance: &str) -> Result<()>;
    /// End `instance` (`ENDTCPSVR SERVER(*DIRSRV) INSTANCE(name)`).
    ///
    /// # Errors
    /// `CPF222E`; `GLD0203`.
    fn end(&self, user: &str, instance: &str) -> Result<()>;
    /// End and start `instance` again, so its configuration is read.
    ///
    /// # Errors
    /// As [`DirAdmin::end`] and [`DirAdmin::start`].
    fn restart(&self, user: &str, instance: &str) -> Result<()>;
    /// The properties.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0201`; `GLD0127`.
    fn properties(&self, user: &str, instance: &str) -> Result<Properties>;
    /// Change the properties to `to`: the attributes that differ are
    /// written; the names of those written.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0201`; `GLD0232` for a value not taken.
    fn set_properties(&self, user: &str, instance: &str, to: &Properties) -> Result<Vec<String>>;
    /// The suffixes, in the file's order.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0201`.
    fn suffixes(&self, user: &str, instance: &str) -> Result<Vec<Suffix>>;
    /// Add a user suffix.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0224`; `CPF9898` when it is there or overlaps one.
    fn add_suffix(&self, user: &str, instance: &str, dn: &str) -> Result<()>;
    /// Remove a user suffix; its entries stay in the store.
    ///
    /// # Errors
    /// `CPF222E`; `CPF9898` when it is not a user suffix.
    fn remove_suffix(&self, user: &str, instance: &str, dn: &str) -> Result<()>;
    /// The indexes: the six always there, then the configured ones.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0201`.
    fn indexes(&self, user: &str, instance: &str) -> Result<Vec<Index>>;
    /// Add or change a configured index.
    ///
    /// # Errors
    /// `CPF222E`; `CPF9898` for an attribute the schema lacks or an
    /// index always there.
    fn put_index(&self, user: &str, instance: &str, index: &Index) -> Result<()>;
    /// Remove a configured index.
    ///
    /// # Errors
    /// `CPF222E`; `CPF9898`.
    fn remove_index(&self, user: &str, instance: &str, attr: &str) -> Result<()>;
    /// The administrator and the update DN.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0201`.
    fn admin(&self, user: &str, instance: &str) -> Result<AdminInfo>;
    /// Change the administrator or a password, as `CHGDIRSVRA DN` does.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0202`; `GLD0224`; `GLD0215`.
    fn set_admin(
        &self,
        user: &str,
        instance: &str,
        change: &AdminChange,
        password: &str,
    ) -> Result<()>;
    /// Definitions of the schema.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0201`.
    fn schema(&self, user: &str, instance: &str, q: &SchemaQuery) -> Result<SchemaPage>;
    /// Add a definition (its RFC 4512 description).
    ///
    /// # Errors
    /// `CPF222E`; `CPF9898` with the schema's reason.
    fn add_schema(&self, user: &str, instance: &str, kind: SchemaKind, text: &str) -> Result<()>;
    /// Remove a definition by name or OID.
    ///
    /// # Errors
    /// `CPF222E`; `CPF9898` when protected or in use.
    fn remove_schema(&self, user: &str, instance: &str, kind: SchemaKind, id: &str) -> Result<()>;
    /// An entry's access control.
    ///
    /// # Errors
    /// `CPF222E`; `CPF9898` for no such entry.
    fn access(&self, user: &str, instance: &str, dn: &str) -> Result<Access>;
    /// Change an entry's own ACL and owners (`explicit_*` false: inherit).
    ///
    /// # Errors
    /// `CPF222E`; `CPF9898` for a value that is not valid.
    fn set_access(&self, user: &str, instance: &str, a: &Access) -> Result<()>;
    /// The publishing agents' configuration.
    ///
    /// # Errors
    /// `CPF222E`.
    fn publishing(&self, user: &str) -> Result<Vec<PublishAgent>>;
    /// Change one agent; `password` replaces the bind password when
    /// given (empty: removes it).
    ///
    /// # Errors
    /// `CPF222E`; `GLD0224` for a DN; `CPF9898`.
    fn set_publishing(
        &self,
        user: &str,
        agent: &PublishAgent,
        password: Option<&str>,
    ) -> Result<()>;
    /// The agents' status.
    ///
    /// # Errors
    /// `CPF222E`.
    fn publishing_status(&self, user: &str) -> Result<Vec<AgentStatus>>;
    /// The counters of a running instance.
    ///
    /// # Errors
    /// `CPF222E`; `GLD0201`; `CPF9898` (unavailable) when it is not
    /// running.
    fn counters(&self, user: &str, instance: &str) -> Result<Counters>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_errors_become_messages() {
        let e = AdminError::from(ConfigError::NoInstance("X".into()));
        assert_eq!(
            (e.kind, e.msgid.as_str(), e.data.clone()),
            (ErrorKind::NotFound, "GLD0201", vec!["X".to_string()])
        );
        assert_eq!(e.text, "Directory server instance X not found.");
        let back: AdminError = ipc::decode(&ipc::encode(&e)).unwrap();
        assert_eq!(back, e);
        let p = Properties::default();
        let back: Properties = ipc::decode(&ipc::encode(&p)).unwrap();
        assert_eq!(back, p);
    }
}
