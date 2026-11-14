// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Operations, controls and extended operations (docs/DIRSRV.md §7): the
//! engine the storage service runs per request (§4).
//!
//! A [`Directory`] is one instance: its schema, settings and backends.
//! Every call runs inside the caller's store transaction and takes the
//! caller's [`Session`]:
//!
//! - [`Directory::bind`] (`dir_bind`): a new session, or the failure.
//!   The transaction is committed either way (password policy records
//!   failures).
//! - [`Directory::op`] (`dir_op`): add, modify, delete, modify DN,
//!   compare, an extended operation. Commit on success, abort otherwise;
//!   after a commit pass [`Outcome::changes`] to
//!   [`Directory::publish`] for persistent searches.
//! - [`Directory::search`] and [`Directory::next`] (`dir_search`,
//!   `dir_next`): a cursor in the connection's [`Cursors`] and its
//!   entries, at most a [`Budget`] per call; paged results keep the
//!   cursor between requests.
//! - [`Directory::changes`] (`dir_changes`): what a persistent search
//!   has seen since the last call.

// Short names follow the specifications' notation (RFCs, crypt).
#![allow(clippy::many_single_char_names, clippy::similar_names)]

use std::collections::HashMap;

use ldap::ResultCode;
use ldap::control::Control;
use ldap::dn::Dn;
use ldap::msg::{Attribute, LdapResult, ProtocolOp};
use store::txn::Txn;

use crate::acl::{self, AclEntry, Effective, Subject};
use crate::backend::btree::BtreeBackend;
use crate::backend::changelog::ChangelogBackend;
use crate::backend::monitor::Counters;
use crate::backend::{Backend, DirEntry};
use crate::schema::{AttributeType, Schema};
use crate::store::{ChangeKind, DirDb};

mod access;
mod attrs;
mod bind;
pub mod clock;
mod compare;
mod controls;
pub mod error;
mod ext;
pub mod filter;
mod rootdse;
mod search;
mod seed;
pub mod session;
pub mod settings;
mod watch;
mod write;

pub use clock::{Env, SystemEnv};
pub use error::{OpError, OpResult};
pub use search::{Budget, Cursors, Page, PageState, Started};
pub use session::{Audit, AuditEvent, AuditOp, Conn, Granted, Identity, Method, Refused, Session};
pub use settings::Settings;
pub use watch::Watches;

/// One message to send: an operation and its controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// The operation.
    pub op: ProtocolOp,
    /// Its response controls.
    pub controls: Vec<Control>,
}

impl Reply {
    /// A reply without controls.
    #[must_use]
    pub fn new(op: ProtocolOp) -> Self {
        Self {
            op,
            controls: Vec::new(),
        }
    }
}

/// A change an operation made, for persistent searches and audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// What happened.
    pub kind: ChangeKind,
    /// The entry's DN after the change (before, for a delete).
    pub dn: String,
    /// A modify DN's old DN.
    pub previous_dn: Option<String>,
    /// The change log's number, when the log is on.
    pub number: Option<u64>,
    /// The backend and the entry's ID there.
    pub backend: usize,
    /// The entry's ID.
    pub id: u64,
    /// Its parent's ID.
    pub parent: Option<u64>,
    /// The entry's attributes after the change (before, for a delete).
    pub attrs: Vec<Attribute>,
}

/// What a request comes to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The responses, in order.
    pub replies: Vec<Reply>,
    /// The changes made (for [`Directory::publish`] after the commit).
    pub changes: Vec<Change>,
    /// A bind's new session (absent: the session stays as it was).
    pub session: Option<Session>,
    /// `StartTLS` succeeded: the job starts the TLS handshake after
    /// sending the reply (or answers 2 itself without a certificate).
    pub start_tls: bool,
    /// `cn=schema` changed: the service writes `Schema::modified_text`
    /// to `etc/V3.modifiedschema`.
    pub schema_changed: bool,
    /// The result code of the operation (for the caller's commit).
    pub code: Option<ResultCode>,
}

impl Outcome {
    fn one(op: ProtocolOp, controls: Vec<Control>) -> Self {
        let code = op.result().map(|r| r.code);
        Self {
            replies: vec![Reply { op, controls }],
            code,
            ..Self::default()
        }
    }

    /// Whether the operation succeeded (a compare's true and false
    /// included): the caller commits.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        matches!(
            self.code,
            Some(ResultCode::Success | ResultCode::CompareTrue | ResultCode::CompareFalse) | None
        )
    }
}

/// Where a DN lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    RootDse,
    Schema,
    Monitor,
    /// Below `cn=monitor`.
    UnderMonitor,
    Backend(usize),
    Outside,
}

/// An entry found for an operation.
#[derive(Debug, Clone)]
struct Found {
    backend: usize,
    entry: DirEntry,
}

/// The well-known DNs.
fn dn_of(s: &str) -> Dn {
    Dn::parse(s).expect("a well-known DN")
}

/// One directory server instance's operations.
pub struct Directory {
    /// The instance's schema.
    pub schema: Schema,
    /// Its settings.
    pub settings: Settings,
    /// `cn=monitor`'s counters.
    pub counters: Counters,
    backends: Vec<Box<dyn Backend>>,
    identity: Option<Box<dyn Identity>>,
    audit: Option<Box<dyn Audit>>,
    env: Box<dyn Env>,
    watches: Watches,
    /// The service's own session (administrator rights, for internal
    /// reads).
    sys: Session,
    admin_dn: Dn,
}

/// The suffixes the btree backend always holds.
pub const LOCAL_SUFFIXES: [&str; 2] = ["cn=localhost", "cn=ibmpolicies"];

impl Directory {
    /// An instance over `db` (whose transaction the caller has), with
    /// the btree backend holding the configured suffixes and
    /// `cn=localhost` and `cn=ibmpolicies`, and `cn=changelog` when
    /// `changelog_max` is set.
    ///
    /// # Errors
    /// A suffix that is not a DN, or the store's.
    pub fn new(
        txn: &mut Txn<'_>,
        db: DirDb,
        schema: Schema,
        settings: Settings,
        env: Box<dyn Env>,
    ) -> OpResult<Self> {
        let mut suffixes = Vec::new();
        for s in settings
            .suffixes
            .iter()
            .map(String::as_str)
            .chain(LOCAL_SUFFIXES)
        {
            suffixes.push(
                Dn::parse(s).map_err(|_| {
                    OpError::new(ResultCode::InvalidDnSyntax, format!("suffix {s}"))
                })?,
            );
        }
        let mut btree = BtreeBackend::new(db, suffixes);
        if settings.changelog_max > 0 {
            db.set_log_max(txn, settings.changelog_max)?;
            btree.log_changes = true;
        }
        let admin_dn = Dn::parse(&settings.admin_dn).map_err(|_| {
            OpError::new(
                ResultCode::InvalidDnSyntax,
                "the administrator DN is not a DN",
            )
        })?;
        let sys = Session::admin(&schema, &admin_dn);
        let mut backends: Vec<Box<dyn Backend>> = vec![Box::new(btree)];
        if settings.changelog_max > 0 {
            backends.push(Box::new(ChangelogBackend { db }));
        }
        let counters = Counters {
            start_time: env.now_micros(),
            ..Counters::default()
        };
        Ok(Self {
            schema,
            settings,
            counters,
            backends,
            identity: None,
            audit: None,
            env,
            watches: Watches::default(),
            sys,
            admin_dn,
        })
    }

    /// Add a backend (the projected one, the configuration's).
    pub fn add_backend(&mut self, b: Box<dyn Backend>) {
        self.backends.push(b);
    }

    /// Set the hook for binds the service answers.
    pub fn set_identity(&mut self, i: Box<dyn Identity>) {
        self.identity = Some(i);
    }

    /// Set the audit hook.
    pub fn set_audit(&mut self, a: Box<dyn Audit>) {
        self.audit = Some(a);
    }

    fn now(&self) -> u64 {
        self.env.now_micros()
    }

    fn audit(&mut self, op: AuditOp, target: &str, session: &Session, code: ResultCode) {
        if let Some(a) = self.audit.as_mut() {
            a.record(&AuditEvent {
                op,
                target: target.to_string(),
                who: session.dn.clone(),
                code,
            });
        }
    }

    fn is_admin_dn(&self, dn: &Dn) -> bool {
        self.schema.dn_eq(dn, &self.admin_dn)
    }

    /// Where `dn` lives: the backend with the longest suffix it is under.
    fn place(&self, dn: &Dn) -> Place {
        if dn.is_root() {
            return Place::RootDse;
        }
        if crate::backend::schema::is_schema_dn(&self.schema, dn) {
            return Place::Schema;
        }
        let monitor = dn_of(crate::backend::monitor::DN);
        if self.schema.dn_eq(dn, &monitor) {
            return Place::Monitor;
        }
        if self.schema.dn_within(dn, &monitor) {
            return Place::UnderMonitor;
        }
        let mut best: Option<(usize, usize)> = None;
        for (i, b) in self.backends.iter().enumerate() {
            for s in b.suffixes() {
                if self.schema.dn_within(dn, &s) && best.is_none_or(|(_, n)| s.len() > n) {
                    best = Some((i, s.len()));
                }
            }
        }
        best.map_or(Place::Outside, |(i, _)| Place::Backend(i))
    }

    /// The failure for a DN under no suffix: the default referral, or
    /// noSuchObject.
    fn outside(&self, dn: &Dn) -> OpError {
        if self.settings.referral.is_empty() {
            OpError::new(ResultCode::NoSuchObject, format!("{dn} is under no suffix"))
        } else {
            OpError {
                referral: self.settings.referral.clone(),
                ..OpError::new(ResultCode::Referral, "the entry is held elsewhere")
            }
        }
    }

    fn is_referral(&self, e: &DirEntry) -> bool {
        self.has_class(e, "referral")
    }

    fn has_class(&self, e: &DirEntry, class: &str) -> bool {
        let Some(c) = self.schema.class(class) else {
            return false;
        };
        e.attrs
            .iter()
            .filter(|a| a.ty.eq_ignore_ascii_case("objectClass"))
            .flat_map(|a| &a.vals)
            .any(|v| {
                self.schema
                    .class(String::from_utf8_lossy(v).trim())
                    .is_some_and(|x| x.oid() == c.oid())
            })
    }

    /// The `ref` URLs of referral entry `e` for target `dn`, the DN in
    /// each URL extended by the RDNs below `e` (RFC 3296 §5).
    fn referral_urls(e: &DirEntry, dn: &Dn) -> Vec<String> {
        let edn = Dn::parse(&e.dn).unwrap_or_default();
        let extra = dn.len().saturating_sub(edn.len());
        let below: Vec<String> = dn.0[..extra].iter().map(ToString::to_string).collect();
        e.get("ref")
            .map(|a| {
                a.vals
                    .iter()
                    .map(|v| {
                        let u = String::from_utf8_lossy(v).to_string();
                        if below.is_empty() {
                            return u;
                        }
                        let Some(hostpart) = u
                            .find("://")
                            .and_then(|i| u[i + 3..].find('/').map(|j| i + 3 + j))
                        else {
                            return format!("{u}/{}", below.join(","));
                        };
                        let (head, path) = u.split_at(hostpart);
                        let rest = &path[1..];
                        let (urldn, tail) = rest.split_at(rest.find('?').unwrap_or(rest.len()));
                        let joined = if urldn.is_empty() {
                            below.join(",")
                        } else {
                            format!("{},{urldn}", below.join(","))
                        };
                        format!("{head}/{joined}{tail}")
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[allow(clippy::unused_self)]
    fn referral_error(&self, e: &DirEntry, dn: &Dn) -> OpError {
        OpError {
            referral: Self::referral_urls(e, dn),
            ..OpError::new(ResultCode::Referral, "the entry is held elsewhere")
        }
    }

    /// Find `dn` for an operation: the entry, or noSuchObject with the
    /// matched DN (upper-cased, as IBM's server writes it), or a
    /// referral. `Ok(None)`: the entry exists but is hidden from
    /// `session`.
    fn find(
        &self,
        txn: &mut Txn<'_>,
        session: &Session,
        dn: &Dn,
        manage: bool,
    ) -> OpResult<Option<Found>> {
        let Place::Backend(i) = self.place(dn) else {
            return Err(self.outside(dn));
        };
        let b = &self.backends[i];
        if b.admin_only() && !session.admin {
            return Err(OpError::access());
        }
        let r = b.resolve(txn, &self.schema, session, dn)?;
        let Some(id) = r.entry else {
            let mut matched = String::new();
            if let Some(m) = r.matched
                && let Some(me) = b.entry(txn, &self.schema, &self.sys, m)?
            {
                if !manage && self.is_referral(&me) {
                    return Err(self.referral_error(&me, dn));
                }
                matched = me.dn.to_ascii_uppercase();
            }
            return Err(
                OpError::new(ResultCode::NoSuchObject, format!("{dn} does not exist"))
                    .matched(matched),
            );
        };
        let Some(entry) = b.entry(txn, &self.schema, session, id)? else {
            return Ok(None);
        };
        if !manage && self.is_referral(&entry) {
            return Err(self.referral_error(&entry, dn));
        }
        Ok(Some(Found { backend: i, entry }))
    }

    /// [`find`](Self::find) where a hidden entry is noSuchObject.
    fn find_visible(
        &self,
        txn: &mut Txn<'_>,
        session: &Session,
        dn: &Dn,
        manage: bool,
    ) -> OpResult<Found> {
        self.find(txn, session, dn, manage)?
            .ok_or_else(|| OpError::new(ResultCode::NoSuchObject, format!("{dn} does not exist")))
    }

    fn parse_dn(s: &str) -> OpResult<Dn> {
        Dn::parse(s)
            .map_err(|_| OpError::new(ResultCode::InvalidDnSyntax, format!("{s:?} is not a DN")))
    }

    fn entry_key(&self, dn: &str) -> Vec<u8> {
        Dn::parse(dn).map_or_else(
            |_| dn.as_bytes().to_vec(),
            |d| self.schema.normalize_parsed_dn(&d),
        )
    }

    /// The rights `session` has on attribute `at` of an entry.
    #[allow(clippy::unused_self)]
    fn attr_rights(
        &self,
        eff: &Effective,
        session: &Session,
        key: &[u8],
        at: &AttributeType,
    ) -> u8 {
        eff.attr_rights(&session.principal(), key, at.oid(), at.access)
    }

    fn parse_acls(&self, a: Option<&Attribute>) -> Vec<AclEntry> {
        a.map(|a| {
            a.vals
                .iter()
                .filter_map(|v| {
                    acl::model::parse_acl(&self.schema, &String::from_utf8_lossy(v)).ok()
                })
                .collect()
        })
        .unwrap_or_default()
    }

    fn parse_owners(&self, a: Option<&Attribute>) -> Vec<Subject> {
        a.map(|a| {
            a.vals
                .iter()
                .filter_map(|v| {
                    acl::model::parse_owner(&self.schema, &String::from_utf8_lossy(v)).ok()
                })
                .collect()
        })
        .unwrap_or_default()
    }

    fn result(code: ResultCode, message: &str) -> LdapResult {
        LdapResult::with_message(code, message)
    }
}

/// A cache of what children inherit, per backend and entry, for one
/// operation or one page of a search.
#[derive(Debug, Default)]
pub(crate) struct AclCache {
    inherit: HashMap<(usize, u64), access::Inherit>,
}
