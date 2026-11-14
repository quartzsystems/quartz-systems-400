// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Who a connection is bound as ([`Session`]), what the job tells of the
//! connection ([`Conn`]), and the service's hooks: binds of DNs the
//! service answers for ([`Identity`]) and auditing ([`Audit`]).

use ldap::ResultCode;
use ldap::control::{PolicyError, PolicyWarning};
use ldap::dn::Dn;
use store::txn::Txn;

use crate::acl::Principal;
use crate::schema::Schema;

/// How a session authenticated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Method {
    /// Not bound, or bound anonymously.
    #[default]
    Anonymous,
    /// A simple bind.
    Simple,
    /// SASL EXTERNAL with a TLS client certificate.
    External,
}

/// A bound identity, kept by the service with the connection's session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    /// The DN as bound, `None` for anonymous.
    pub dn: Option<String>,
    /// The DN normalized.
    pub key: Option<Vec<u8>>,
    /// The groups and roles it belongs to, normalized (static groups
    /// found at bind, and the groups an [`Identity`] reports).
    pub groups: Vec<Vec<u8>>,
    /// The directory administrator, or an administrative profile.
    pub admin: bool,
    /// How it bound.
    pub method: Method,
    /// After a reset under `pwdMustChange`: only its own password may be
    /// changed.
    pub must_change: bool,
}

impl Session {
    /// The anonymous session.
    #[must_use]
    pub fn anonymous() -> Self {
        Self::default()
    }

    /// A session bound as `dn` (the groups to be filled by the caller).
    #[must_use]
    pub fn bound(schema: &Schema, dn: &Dn, method: Method) -> Self {
        Self {
            dn: Some(dn.to_string()),
            key: Some(schema.normalize_parsed_dn(dn)),
            method,
            ..Self::default()
        }
    }

    /// The administrator's session (the service's own work, and tests).
    #[must_use]
    pub fn admin(schema: &Schema, dn: &Dn) -> Self {
        Self {
            admin: true,
            ..Self::bound(schema, dn, Method::Simple)
        }
    }

    /// The identity ACLs see.
    #[must_use]
    pub fn principal(&self) -> Principal<'_> {
        Principal {
            dn: self.key.as_deref(),
            groups: &self.groups,
            admin: self.admin,
        }
    }

    /// The DN `creatorsName` and `modifiersName` record.
    #[must_use]
    pub fn who(&self) -> String {
        self.dn.clone().unwrap_or_else(|| "cn=anybody".into())
    }
}

/// What the job knows of the connection a request came on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conn {
    /// The connection is protected by TLS (LDAPS, or after `StartTLS`).
    pub tls: bool,
    /// The TLS client certificate's subject DN, if one was presented.
    pub client_subject: Option<String>,
}

/// A bind the service accepted for a DN it answers for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Granted {
    /// The groups the identity belongs to (DNs).
    pub groups: Vec<String>,
    /// It binds as an administrator (`QIBM_DIRSRV_ADMIN`).
    pub admin: bool,
    /// The password policy response's warning.
    pub warning: Option<PolicyWarning>,
}

/// A bind the service refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The result code (49 for a wrong password, a disabled profile).
    pub code: ResultCode,
    /// The password policy response's error (`passwordExpired`, ...).
    pub policy: Option<PolicyError>,
}

/// Binds the service answers itself: DNs naming OS profiles (the
/// projected backend's), checked against `sec` with sign-on's rules,
/// `QMAXSIGN` and `QAUDJRN`. The directory asks it before its own
/// entries.
pub trait Identity {
    /// Whether `dn` is one the service binds.
    fn owns(&self, schema: &Schema, dn: &Dn) -> bool;

    /// A simple bind of `dn` with `password`.
    ///
    /// # Errors
    /// [`Refused`] with the answer.
    fn bind(&mut self, txn: &mut Txn<'_>, dn: &Dn, password: &[u8]) -> Result<Granted, Refused>;

    /// SASL EXTERNAL as `dn` (a certificate's subject); `None` when the
    /// service does not know the DN.
    fn external(&mut self, txn: &mut Txn<'_>, dn: &Dn) -> Option<Granted>;
}

/// What an audit record is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOp {
    /// A bind.
    Bind,
    /// A search started.
    Search,
    /// A compare.
    Compare,
    /// An add.
    Add,
    /// A modify.
    Modify,
    /// A delete.
    Delete,
    /// A modify DN.
    ModDn,
    /// An extended operation.
    Extended,
}

/// One operation for the audit journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// The operation.
    pub op: AuditOp,
    /// The DN it named (a bind's name, an extended operation's OID).
    pub target: String,
    /// The session's DN, `None` for anonymous.
    pub who: Option<String>,
    /// Its result.
    pub code: ResultCode,
}

/// Where audit records go: the service writes them to `QAUDJRN` as `DI`
/// entries when `ibm-audit` and the operation's switch are on (§12).
pub trait Audit {
    /// Record one operation.
    fn record(&mut self, ev: &AuditEvent);
}
