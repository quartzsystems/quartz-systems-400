// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Dirsrv` (version 12, draft): the directory server's instances,
//! properties, suffixes, indexes, administrator, schema, entry access,
//! publishing and counters, each as the storage service's directory
//! administration answers it (docs/QAP.md, "Service 14 `Dirsrv`").

use ipc::{wire_enum, wire_struct};

use crate::types::JobId;

/// Whether an instance runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InstanceState {
    /// Its job is not active.
    #[default]
    Stopped,
    /// Starting.
    Starting,
    /// Answering on its ports.
    Running,
    /// Ending.
    Ending,
}

wire_enum!(InstanceState {
    0 => Stopped,
    1 => Starting,
    2 => Running,
    3 => Ending,
});

/// One instance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Instance {
    /// Its name (`QUSRDIR`).
    pub name: String,
    /// Its description.
    pub description: String,
    /// Started with `INSTANCE(*AUTOSTART)`.
    pub autostart: bool,
    /// Whether it runs.
    pub state: InstanceState,
    /// Its job while active.
    pub job: Option<JobId>,
    /// The LDAP port.
    pub port: u16,
    /// The LDAPS port.
    pub secure_port: u16,
    /// `none`, `SSL`, `TLS` or `SSLTLS`.
    pub security: String,
    /// The certificate assigned to the application ID, when one is.
    pub certificate: Option<String>,
    /// IBM's level it follows (`6.1`).
    pub version: String,
    /// When its job started, milliseconds since 1970-01-01 UTC; 0 when
    /// not active.
    pub since: i64,
}

wire_struct!(Instance {
    name,
    description,
    autostart,
    state,
    job,
    port,
    secure_port,
    security,
    certificate,
    version,
    since
});

/// `ListDirInstances`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Instances {
    /// In the instance list's order.
    pub instances: Vec<Instance>,
}

wire_struct!(Instances { instances });

/// A request naming an instance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceRef {
    /// The instance (case ignored).
    pub instance: String,
}

wire_struct!(InstanceRef { instance });

/// `SetDirAutostart`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetDirAutostart {
    /// The instance.
    pub instance: String,
    /// Started with `INSTANCE(*AUTOSTART)`.
    pub autostart: bool,
}

wire_struct!(SetDirAutostart {
    instance,
    autostart
});

/// The server properties: the honoured attributes of `ibmslapd.conf`
/// (docs/DIRSRV.md §10).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the switches are flags
pub struct DirProperties {
    /// `ibm-slapdPort`.
    pub port: u16,
    /// `ibm-slapdSecurePort`.
    pub secure_port: u16,
    /// `ibm-slapdSecurity`.
    pub security: String,
    /// `ibm-slapdSizeLimit`.
    pub size_limit: u32,
    /// `ibm-slapdTimeLimit`, seconds.
    pub time_limit: u32,
    /// `ibm-slapdIdleTimeOut`, seconds.
    pub idle_timeout: u32,
    /// `ibm-slapdWriteTimeout`, seconds.
    pub write_timeout: u32,
    /// `ibm-slapdAllowAnon`.
    pub allow_anonymous: bool,
    /// `ibm-slapdDerefAliases`.
    pub deref_aliases: String,
    /// `ibm-slapdSchemaCheck`.
    pub schema_check: String,
    /// `ibm-slapdPwEncryption`.
    pub password_encryption: String,
    /// `ibm-slapdReadOnly` of the directory's backend.
    pub read_only: bool,
    /// `ibm-slapdLogOptions`.
    pub log_options: String,
    /// `ibm-audit`.
    pub audit: bool,
    /// `ibm-slapdEnablePersistentSearch`.
    pub persistent_search: bool,
    /// `ibm-slapdMaxPersistentSearches`.
    pub max_persistent_searches: u32,
    /// `ibm-slapdPagedResAllowNonAdmin`.
    pub paged_for_all: bool,
    /// `ibm-slapdPagedResLmt`.
    pub paged_limit: u32,
    /// `ibm-slapdSortSrchAllowNonAdmin`.
    pub sorted_for_all: bool,
    /// `ibm-slapdSortKeyLimit`.
    pub sort_key_limit: u32,
    /// `ibm-slapdChangeLogMaxEntries`; 0 for no change log.
    pub changelog_max: u32,
    /// `ibm-slapdConfigPwdPolicyOn`.
    pub password_policy: bool,
    /// `ibm-slapdOs400UsrprjRead`.
    pub profiles_read_only: bool,
    /// `ibm-slapdReferral`.
    pub referrals: Vec<String>,
}

wire_struct!(DirProperties {
    port,
    secure_port,
    security,
    size_limit,
    time_limit,
    idle_timeout,
    write_timeout,
    allow_anonymous,
    deref_aliases,
    schema_check,
    password_encryption,
    read_only,
    log_options,
    audit,
    persistent_search,
    max_persistent_searches,
    paged_for_all,
    paged_limit,
    sorted_for_all,
    sort_key_limit,
    changelog_max,
    password_policy,
    profiles_read_only,
    referrals
});

/// `SetDirProperties`' request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetDirProperties {
    /// The instance.
    pub instance: String,
    /// The properties as they are to be, whole.
    pub properties: DirProperties,
}

wire_struct!(SetDirProperties {
    instance,
    properties
});

/// `SetDirProperties`' reply: the attributes written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changed {
    /// The attribute names.
    pub attributes: Vec<String>,
}

wire_struct!(Changed { attributes });

/// Which backend a suffix belongs to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SuffixKind {
    /// A user suffix: removable.
    #[default]
    User,
    /// `cn=localhost`, `cn=ibmpolicies`.
    Local,
    /// The user profiles (`os400-sys=`).
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
    /// The DN.
    pub dn: String,
    /// Its backend.
    pub kind: SuffixKind,
    /// Entries under it, when known.
    pub entries: Option<u64>,
}

wire_struct!(Suffix { dn, kind, entries });

/// `ListDirSuffixes`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suffixes {
    /// In the configuration's order.
    pub suffixes: Vec<Suffix>,
}

wire_struct!(Suffixes { suffixes });

/// A request naming a suffix or an entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DnRef {
    /// The instance.
    pub instance: String,
    /// The DN.
    pub dn: String,
}

wire_struct!(DnRef { instance, dn });

/// One attribute index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the kinds are flags
pub struct DirIndex {
    /// The attribute type.
    pub attribute: String,
    /// Presence keys.
    pub presence: bool,
    /// Equality keys.
    pub equality: bool,
    /// Substring keys.
    pub substring: bool,
    /// One of the six always there (ignored in requests).
    pub always: bool,
}

wire_struct!(DirIndex {
    attribute,
    presence,
    equality,
    substring,
    always
});

/// `ListDirIndexes`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirIndexes {
    /// The six always there, then the configured ones.
    pub indexes: Vec<DirIndex>,
}

wire_struct!(DirIndexes { indexes });

/// `PutDirIndex`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PutDirIndex {
    /// The instance.
    pub instance: String,
    /// The index as it is to be.
    pub index: DirIndex,
}

wire_struct!(PutDirIndex { instance, index });

/// `RemoveDirIndex`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirIndexRef {
    /// The instance.
    pub instance: String,
    /// The attribute type.
    pub attribute: String,
}

wire_struct!(DirIndexRef {
    instance,
    attribute
});

/// The administrator and the update DN.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirAdmin {
    /// `ibm-slapdAdminDN`.
    pub dn: String,
    /// Its password is set.
    pub password_set: bool,
    /// `ibm-slapdMasterDN`, when named.
    pub update_dn: Option<String>,
}

wire_struct!(DirAdmin {
    dn,
    password_set,
    update_dn
});

/// What `SetDirAdmin` changes (`CHGDIRSVRA DN`'s first element).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AdminTarget {
    /// `*ADMINDN`: the administrator's password.
    #[default]
    Password,
    /// A DN: the administrator and its password.
    NewDn,
    /// `*UPDATEDN`: the update DN's password.
    UpdatePassword,
}

wire_enum!(AdminTarget {
    0 => Password,
    1 => NewDn,
    2 => UpdatePassword,
});

/// `SetDirAdmin`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetDirAdmin {
    /// The instance.
    pub instance: String,
    /// What changes.
    pub target: AdminTarget,
    /// The new administrator for `NewDn`; ignored otherwise.
    pub dn: String,
    /// The password.
    pub password: String,
}

wire_struct!(SetDirAdmin {
    instance,
    target,
    dn,
    password
});

/// Attribute types or object classes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SchemaKind {
    /// Attribute types.
    #[default]
    AttributeType,
    /// Object classes.
    ObjectClass,
}

wire_enum!(SchemaKind {
    0 => AttributeType,
    1 => ObjectClass,
});

/// `ListDirSchema`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListDirSchema {
    /// The instance.
    pub instance: String,
    /// Which.
    pub kind: SchemaKind,
    /// Only names or OIDs containing this (case ignored); empty: all.
    pub contains: String,
    /// Only those added since the schema was shipped.
    pub added_only: bool,
    /// The first to return, from 0.
    pub start: u32,
    /// At most this many; 0 for all.
    pub max: u32,
}

wire_struct!(ListDirSchema {
    instance,
    kind,
    contains,
    added_only,
    start,
    max
});

/// One definition.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // flags
pub struct SchemaDefinition {
    /// Which.
    pub kind: SchemaKind,
    /// The OID.
    pub oid: String,
    /// Its names, the primary first.
    pub names: Vec<String>,
    /// Its description.
    pub description: String,
    /// The supertype or superclasses.
    pub superior: Vec<String>,
    /// An attribute type's syntax OID, `{length}` when bounded.
    pub syntax: String,
    /// An attribute type is single-valued.
    pub single_value: bool,
    /// An attribute type's access class.
    pub access_class: String,
    /// An object class's kind (`STRUCTURAL` ...).
    pub class_kind: String,
    /// Required attributes, inherited too.
    pub must: Vec<String>,
    /// Allowed attributes, inherited too.
    pub may: Vec<String>,
    /// Added since shipped.
    pub added: bool,
    /// Needed by the server.
    pub protected: bool,
    /// The RFC 4512 description.
    pub text: String,
}

wire_struct!(SchemaDefinition {
    kind,
    oid,
    names,
    description,
    superior,
    syntax,
    single_value,
    access_class,
    class_kind,
    must,
    may,
    added,
    protected,
    text
});

/// `ListDirSchema`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaDefinitions {
    /// How many match in all.
    pub total: u32,
    /// The page, by primary name.
    pub definitions: Vec<SchemaDefinition>,
}

wire_struct!(SchemaDefinitions { total, definitions });

/// `AddDirSchema`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddDirSchema {
    /// The instance.
    pub instance: String,
    /// Which.
    pub kind: SchemaKind,
    /// The RFC 4512 description.
    pub description: String,
}

wire_struct!(AddDirSchema {
    instance,
    kind,
    description
});

/// `RemoveDirSchema`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaRef {
    /// The instance.
    pub instance: String,
    /// Which.
    pub kind: SchemaKind,
    /// A name or the OID.
    pub id: String,
}

wire_struct!(SchemaRef { instance, kind, id });

/// An entry's access control.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // flags
pub struct EntryAccess {
    /// The entry.
    pub dn: String,
    /// It has its own ACL.
    pub own_acl: bool,
    /// `aclEntry` in effect.
    pub acl: Vec<String>,
    /// `aclPropagate`.
    pub acl_propagate: bool,
    /// Where the ACL in effect is (`default` for none; ignored in
    /// requests).
    pub acl_source: String,
    /// It has its own owners.
    pub own_owners: bool,
    /// `entryOwner` in effect.
    pub owners: Vec<String>,
    /// `ownerPropagate`.
    pub owner_propagate: bool,
    /// Where the owners in effect are (ignored in requests).
    pub owner_source: String,
}

wire_struct!(EntryAccess {
    dn,
    own_acl,
    acl,
    acl_propagate,
    acl_source,
    own_owners,
    owners,
    owner_propagate,
    owner_source
});

/// `SetDirAccess`' request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetDirAccess {
    /// The instance.
    pub instance: String,
    /// The entry's access as it is to be.
    pub access: EntryAccess,
}

wire_struct!(SetDirAccess { instance, access });

/// A publishing agent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentKind {
    /// User profiles.
    #[default]
    Users,
    /// Printers.
    Printers,
}

wire_enum!(AgentKind {
    0 => Users,
    1 => Printers,
});

/// One agent's configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // flags
pub struct PublishAgent {
    /// Which.
    pub kind: AgentKind,
    /// It publishes.
    pub enabled: bool,
    /// The target's host; empty: this system's default instance.
    pub server: String,
    /// Its port.
    pub port: u16,
    /// TLS: LDAPS, or `StartTLS` with `start_tls`.
    pub tls: bool,
    /// The DN it binds as; empty: anonymous.
    pub bind_dn: String,
    /// A bind password is kept (ignored in requests).
    pub password_set: bool,
    /// The parent DN of what it publishes; empty: the agent's default.
    pub parent_dn: String,
    /// With `tls`: `StartTLS` rather than LDAPS.
    pub start_tls: bool,
    /// The parent and its missing ancestors are added.
    pub create_parent: bool,
    /// Seconds before the first retry.
    pub retry: u32,
    /// Seconds between full resyncs; 0 for none.
    pub resync: u32,
    /// The `publisherName` value; empty: the system's default.
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

/// `GetPublishing`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Publishing {
    /// Users, then printers.
    pub agents: Vec<PublishAgent>,
}

wire_struct!(Publishing { agents });

/// `SetPublishing`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetPublishing {
    /// The agent as it is to be.
    pub agent: PublishAgent,
    /// A new bind password (empty: none kept); `None` keeps it.
    pub password: Option<String>,
}

wire_struct!(SetPublishing { agent, password });

/// One agent's status.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentStatus {
    /// Which.
    pub kind: AgentKind,
    /// Its job while active.
    pub job: Option<JobId>,
    /// Publishing.
    pub active: bool,
    /// Entries published since it started.
    pub published: u64,
    /// Entries it could not publish.
    pub failed: u64,
    /// When it last published, milliseconds since 1970-01-01 UTC; 0
    /// for never.
    pub last: i64,
    /// The last message it logged, when any (`GLD010B`).
    pub message_id: Option<String>,
    /// That message's text.
    pub message_text: String,
}

wire_struct!(AgentStatus {
    kind,
    job,
    active,
    published,
    failed,
    last,
    message_id,
    message_text
});

/// `GetPublishingStatus`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublishingStatus {
    /// Users, then printers.
    pub agents: Vec<AgentStatus>,
}

wire_struct!(PublishingStatus { agents });

/// The counters of `cn=monitor`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirCounters {
    /// When it started, milliseconds since 1970-01-01 UTC.
    pub started: i64,
    /// Connections open.
    pub connections: u32,
    /// Connections since it started.
    pub total_connections: u64,
    /// Operations started.
    pub operations_started: u64,
    /// Operations completed.
    pub operations_completed: u64,
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
    pub modify_dns: u64,
    /// Extended operations.
    pub extended: u64,
    /// Abandons.
    pub abandons: u64,
    /// Entries sent.
    pub entries_sent: u64,
    /// Persistent searches open.
    pub persistent_searches: u32,
}

wire_struct!(DirCounters {
    started,
    connections,
    total_connections,
    operations_started,
    operations_completed,
    binds,
    searches,
    compares,
    adds,
    modifies,
    deletes,
    modify_dns,
    extended,
    abandons,
    entries_sent,
    persistent_searches
});
