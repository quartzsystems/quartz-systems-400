// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `LDAPMessage` and the protocol operations (RFC 4511 §4). DNs are
//! carried as strings, as on the wire; [`crate::dn`] parses them.

use crate::code::ResultCode;
use crate::control::Control;
use crate::filter::Filter;

/// One message: its ID, the operation and the controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LdapMessage {
    /// The message ID (0 only for unsolicited notifications).
    pub id: i32,
    /// The operation.
    pub op: ProtocolOp,
    /// The controls, in order.
    pub controls: Vec<Control>,
}

/// An attribute with its values (`PartialAttribute`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    /// The attribute description (type and options).
    pub ty: String,
    /// The values.
    pub vals: Vec<Vec<u8>>,
}

/// The result every response carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LdapResult {
    /// The result code.
    pub code: ResultCode,
    /// The matched DN.
    pub matched_dn: String,
    /// The diagnostic message.
    pub message: String,
    /// The referral URLs, if the code is [`ResultCode::Referral`].
    pub referral: Vec<String>,
}

/// How a bind authenticates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindAuth {
    /// Simple: the password (empty for anonymous or unauthenticated).
    Simple(Vec<u8>),
    /// SASL: the mechanism and its credentials.
    Sasl {
        /// The mechanism name.
        mechanism: String,
        /// The credentials, if sent.
        credentials: Option<Vec<u8>>,
    },
}

/// The scope of a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The base entry only.
    Base,
    /// The base's children.
    One,
    /// The base and everything below.
    Sub,
}

/// When aliases are dereferenced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerefAliases {
    /// Never.
    Never,
    /// While searching below the base.
    InSearching,
    /// When finding the base.
    FindingBase,
    /// Always.
    Always,
}

/// A search request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    /// The base DN.
    pub base: String,
    /// The scope.
    pub scope: Scope,
    /// Alias dereferencing.
    pub deref: DerefAliases,
    /// The size limit, 0 for none.
    pub size_limit: i32,
    /// The time limit in seconds, 0 for none.
    pub time_limit: i32,
    /// Attribute types only.
    pub types_only: bool,
    /// The filter.
    pub filter: Filter,
    /// The attributes asked for (`*`, `+`, `1.1` included as written).
    pub attrs: Vec<String>,
}

/// What a modification does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModOp {
    /// Add values.
    Add,
    /// Delete values, or the attribute.
    Delete,
    /// Replace the values.
    Replace,
    /// Increment (RFC 4525).
    Increment,
}

/// One change of a modify request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modification {
    /// The operation.
    pub op: ModOp,
    /// The attribute and its values.
    pub attr: Attribute,
}

/// Every protocol operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolOp {
    /// Bind request.
    BindRequest {
        /// The version (2 or 3).
        version: i32,
        /// The name.
        name: String,
        /// The authentication.
        auth: BindAuth,
    },
    /// Bind response.
    BindResponse {
        /// The result.
        result: LdapResult,
        /// SASL server credentials.
        server_sasl_creds: Option<Vec<u8>>,
    },
    /// Unbind request.
    UnbindRequest,
    /// Search request.
    SearchRequest(SearchRequest),
    /// One entry of a search.
    SearchResultEntry {
        /// The entry's DN.
        dn: String,
        /// Its attributes.
        attrs: Vec<Attribute>,
    },
    /// A continuation reference.
    SearchResultReference(Vec<String>),
    /// The end of a search.
    SearchResultDone(LdapResult),
    /// Modify request.
    ModifyRequest {
        /// The entry.
        dn: String,
        /// The changes, in order.
        changes: Vec<Modification>,
    },
    /// Modify response.
    ModifyResponse(LdapResult),
    /// Add request.
    AddRequest {
        /// The entry's DN.
        dn: String,
        /// Its attributes.
        attrs: Vec<Attribute>,
    },
    /// Add response.
    AddResponse(LdapResult),
    /// Delete request.
    DelRequest(String),
    /// Delete response.
    DelResponse(LdapResult),
    /// Modify DN request.
    ModDnRequest {
        /// The entry.
        dn: String,
        /// The new RDN.
        new_rdn: String,
        /// Whether the old RDN's values go.
        delete_old_rdn: bool,
        /// The new superior, if it moves.
        new_superior: Option<String>,
    },
    /// Modify DN response.
    ModDnResponse(LdapResult),
    /// Compare request.
    CompareRequest {
        /// The entry.
        dn: String,
        /// The attribute description.
        attr: String,
        /// The assertion value.
        value: Vec<u8>,
    },
    /// Compare response.
    CompareResponse(LdapResult),
    /// Abandon request: the message ID.
    AbandonRequest(i32),
    /// Extended request.
    ExtendedRequest {
        /// The request name (an OID).
        name: String,
        /// The request value.
        value: Option<Vec<u8>>,
    },
    /// Extended response.
    ExtendedResponse {
        /// The result.
        result: LdapResult,
        /// The response name.
        name: Option<String>,
        /// The response value.
        value: Option<Vec<u8>>,
    },
    /// Intermediate response.
    IntermediateResponse {
        /// The response name.
        name: Option<String>,
        /// The response value.
        value: Option<Vec<u8>>,
    },
}
