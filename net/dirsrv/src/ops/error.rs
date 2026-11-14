// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An operation's failure: the result code, the matched DN, Quartz's
//! diagnostic text, referral URLs, a password policy error.

use ldap::ResultCode;
use ldap::control::PolicyError;
use ldap::msg::LdapResult;

use crate::acl::AclError;
use crate::schema::{CheckError, SchemaError};
use crate::store::{DirError, NormError};

/// An operation that fails, as its response says it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct OpError {
    /// The result code.
    pub code: ResultCode,
    /// The diagnostic message.
    pub message: String,
    /// The matched DN of a `noSuchObject`.
    pub matched: String,
    /// The URLs of a referral.
    pub referral: Vec<String>,
    /// The password policy response control's error.
    pub policy: Option<PolicyError>,
}

/// An operation's result.
pub type OpResult<T> = Result<T, OpError>;

/// RFC 4528's assertionFailed.
pub const ASSERTION_FAILED: ResultCode = ResultCode::Other(122);
/// RFC 4370's authorizationDenied.
pub const AUTHORIZATION_DENIED: ResultCode = ResultCode::Other(123);

impl OpError {
    /// A failure with `code` and Quartz's text.
    #[must_use]
    pub fn new(code: ResultCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            matched: String::new(),
            referral: Vec::new(),
            policy: None,
        }
    }

    /// The same failure with a matched DN.
    #[must_use]
    pub fn matched(mut self, dn: impl Into<String>) -> Self {
        self.matched = dn.into();
        self
    }

    /// The response's `LDAPResult`.
    #[must_use]
    pub fn result(&self) -> LdapResult {
        LdapResult {
            code: self.code,
            matched_dn: self.matched.clone(),
            message: self.message.clone(),
            referral: self.referral.clone(),
        }
    }

    /// insufficientAccessRights.
    #[must_use]
    pub fn access() -> Self {
        Self::new(
            ResultCode::InsufficientAccessRights,
            "the bound identity lacks the access the operation needs",
        )
    }

    /// unwillingToPerform.
    #[must_use]
    pub fn unwilling(message: impl Into<String>) -> Self {
        Self::new(ResultCode::UnwillingToPerform, message)
    }
}

impl From<DirError> for OpError {
    fn from(e: DirError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

impl From<NormError> for OpError {
    fn from(e: NormError) -> Self {
        Self::new(e.code, e.text)
    }
}

impl From<CheckError> for OpError {
    fn from(e: CheckError) -> Self {
        Self::new(e.code, e.what)
    }
}

impl From<SchemaError> for OpError {
    fn from(e: SchemaError) -> Self {
        Self::new(e.code(), e.to_string())
    }
}

impl From<AclError> for OpError {
    fn from(e: AclError) -> Self {
        Self::new(ResultCode::InvalidAttributeSyntax, e.0)
    }
}

impl From<store::StoreError> for OpError {
    fn from(e: store::StoreError) -> Self {
        Self::new(ResultCode::OperationsError, e.to_string())
    }
}
