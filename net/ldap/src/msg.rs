// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `LDAPMessage` and the protocol operations (RFC 4511 §4). DNs are
//! carried as strings, as on the wire; [`crate::dn`] parses them.

use crate::ber::{BerError, Reader, Writer, tag};
use crate::code::ResultCode;
use crate::control::Control;
use crate::filter::{Filter, FilterError};

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

/// Why bytes are not an `LDAPMessage`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MsgError {
    /// Not well-formed BER, or over a limit.
    #[error("bad LDAP message: {0}")]
    Ber(#[from] BerError),
    /// A bad filter in a search request.
    #[error("bad LDAP message: {0}")]
    Filter(#[from] FilterError),
    /// Well-formed BER that is not a valid message.
    #[error("bad LDAP message: {0}")]
    Bad(&'static str),
    /// A protocol operation tag LDAP does not define.
    #[error("unknown LDAP operation tag {0:#04x}")]
    UnknownOp(u8),
}

/// The application tags of the protocol operations.
pub mod op_tag {
    use crate::ber::tag::{application, application_constructed};

    /// `BindRequest`.
    pub const BIND_REQUEST: u8 = application_constructed(0);
    /// `BindResponse`.
    pub const BIND_RESPONSE: u8 = application_constructed(1);
    /// `UnbindRequest`.
    pub const UNBIND_REQUEST: u8 = application(2);
    /// `SearchRequest`.
    pub const SEARCH_REQUEST: u8 = application_constructed(3);
    /// `SearchResultEntry`.
    pub const SEARCH_RESULT_ENTRY: u8 = application_constructed(4);
    /// `SearchResultDone`.
    pub const SEARCH_RESULT_DONE: u8 = application_constructed(5);
    /// `ModifyRequest`.
    pub const MODIFY_REQUEST: u8 = application_constructed(6);
    /// `ModifyResponse`.
    pub const MODIFY_RESPONSE: u8 = application_constructed(7);
    /// `AddRequest`.
    pub const ADD_REQUEST: u8 = application_constructed(8);
    /// `AddResponse`.
    pub const ADD_RESPONSE: u8 = application_constructed(9);
    /// `DelRequest`.
    pub const DEL_REQUEST: u8 = application(10);
    /// `DelResponse`.
    pub const DEL_RESPONSE: u8 = application_constructed(11);
    /// `ModifyDNRequest`.
    pub const MODDN_REQUEST: u8 = application_constructed(12);
    /// `ModifyDNResponse`.
    pub const MODDN_RESPONSE: u8 = application_constructed(13);
    /// `CompareRequest`.
    pub const COMPARE_REQUEST: u8 = application_constructed(14);
    /// `CompareResponse`.
    pub const COMPARE_RESPONSE: u8 = application_constructed(15);
    /// `AbandonRequest`.
    pub const ABANDON_REQUEST: u8 = application(16);
    /// `SearchResultReference`.
    pub const SEARCH_RESULT_REFERENCE: u8 = application_constructed(19);
    /// `ExtendedRequest`.
    pub const EXTENDED_REQUEST: u8 = application_constructed(23);
    /// `ExtendedResponse`.
    pub const EXTENDED_RESPONSE: u8 = application_constructed(24);
    /// `IntermediateResponse`.
    pub const INTERMEDIATE_RESPONSE: u8 = application_constructed(25);
}

/// The largest message ID (RFC 4511 `maxInt`).
pub const MAX_ID: i32 = i32::MAX;

impl LdapResult {
    /// A result of `code` with an empty matched DN and message.
    #[must_use]
    pub const fn new(code: ResultCode) -> Self {
        Self {
            code,
            matched_dn: String::new(),
            message: String::new(),
            referral: Vec::new(),
        }
    }

    /// A success.
    #[must_use]
    pub const fn success() -> Self {
        Self::new(ResultCode::Success)
    }

    /// A result of `code` with a diagnostic message.
    #[must_use]
    pub fn with_message(code: ResultCode, message: &str) -> Self {
        Self {
            message: message.to_string(),
            ..Self::new(code)
        }
    }

    /// Whether the code is success.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.code == ResultCode::Success
    }

    fn encode_fields(&self, w: &mut Writer) {
        w.enumerated(tag::ENUMERATED, i64::from(self.code.number()));
        w.string(tag::OCTET_STRING, &self.matched_dn);
        w.string(tag::OCTET_STRING, &self.message);
        if !self.referral.is_empty() {
            w.constructed(tag::context_constructed(3), |w| {
                for u in &self.referral {
                    w.string(tag::OCTET_STRING, u);
                }
            });
        }
    }

    fn decode_fields(r: &mut Reader<'_>) -> Result<Self, MsgError> {
        let code = r.enumerated(tag::ENUMERATED)?;
        let code = u32::try_from(code).map_err(|_| MsgError::Bad("negative result code"))?;
        let matched_dn = string(r.octets(tag::OCTET_STRING)?)?;
        let message = string(r.octets(tag::OCTET_STRING)?)?;
        let mut referral = Vec::new();
        if r.at(tag::context_constructed(3)) {
            let mut s = r.sequence(tag::context_constructed(3))?;
            while !s.is_empty() {
                referral.push(string(s.octets(tag::OCTET_STRING)?)?);
            }
            if referral.is_empty() {
                return Err(MsgError::Bad("empty referral"));
            }
        }
        Ok(Self {
            code: ResultCode::from_number(code),
            matched_dn,
            message,
            referral,
        })
    }
}

impl Attribute {
    /// An attribute of `ty` with UTF-8 values.
    #[must_use]
    pub fn new(ty: &str, vals: &[&str]) -> Self {
        Self {
            ty: ty.to_string(),
            vals: vals.iter().map(|v| v.as_bytes().to_vec()).collect(),
        }
    }

    fn encode(&self, w: &mut Writer) {
        w.constructed(tag::SEQUENCE, |w| {
            w.string(tag::OCTET_STRING, &self.ty);
            w.constructed(tag::SET, |w| {
                for v in &self.vals {
                    w.octets(tag::OCTET_STRING, v);
                }
            });
        });
    }

    fn decode(r: &mut Reader<'_>) -> Result<Self, MsgError> {
        let mut s = r.sequence(tag::SEQUENCE)?;
        let ty = string(s.octets(tag::OCTET_STRING)?)?;
        let mut set = s.sequence(tag::SET)?;
        s.finish()?;
        let mut vals = Vec::new();
        while !set.is_empty() {
            vals.push(set.octets(tag::OCTET_STRING)?.to_vec());
        }
        Ok(Self { ty, vals })
    }
}

fn string(b: &[u8]) -> Result<String, MsgError> {
    String::from_utf8(b.to_vec()).map_err(|_| MsgError::Bad("string is not UTF-8"))
}

fn encode_attrs(w: &mut Writer, attrs: &[Attribute]) {
    w.constructed(tag::SEQUENCE, |w| attrs.iter().for_each(|a| a.encode(w)));
}

fn decode_attrs(r: &mut Reader<'_>) -> Result<Vec<Attribute>, MsgError> {
    let mut s = r.sequence(tag::SEQUENCE)?;
    let mut v = Vec::new();
    while !s.is_empty() {
        v.push(Attribute::decode(&mut s)?);
    }
    Ok(v)
}

impl Scope {
    /// The wire value.
    #[must_use]
    pub const fn number(self) -> i64 {
        match self {
            Self::Base => 0,
            Self::One => 1,
            Self::Sub => 2,
        }
    }

    /// The scope of a wire value.
    #[must_use]
    pub const fn from_number(n: i64) -> Option<Self> {
        match n {
            0 => Some(Self::Base),
            1 => Some(Self::One),
            2 => Some(Self::Sub),
            _ => None,
        }
    }
}

impl DerefAliases {
    /// The wire value.
    #[must_use]
    pub const fn number(self) -> i64 {
        match self {
            Self::Never => 0,
            Self::InSearching => 1,
            Self::FindingBase => 2,
            Self::Always => 3,
        }
    }

    /// The setting of a wire value.
    #[must_use]
    pub const fn from_number(n: i64) -> Option<Self> {
        match n {
            0 => Some(Self::Never),
            1 => Some(Self::InSearching),
            2 => Some(Self::FindingBase),
            3 => Some(Self::Always),
            _ => None,
        }
    }
}

impl ModOp {
    /// The wire value.
    #[must_use]
    pub const fn number(self) -> i64 {
        match self {
            Self::Add => 0,
            Self::Delete => 1,
            Self::Replace => 2,
            Self::Increment => 3,
        }
    }

    /// The operation of a wire value.
    #[must_use]
    pub const fn from_number(n: i64) -> Option<Self> {
        match n {
            0 => Some(Self::Add),
            1 => Some(Self::Delete),
            2 => Some(Self::Replace),
            3 => Some(Self::Increment),
            _ => None,
        }
    }
}

impl SearchRequest {
    /// A search of `base` with `scope` and `filter`, no limits, every
    /// user attribute.
    #[must_use]
    pub const fn new(base: String, scope: Scope, filter: Filter) -> Self {
        Self {
            base,
            scope,
            deref: DerefAliases::Never,
            size_limit: 0,
            time_limit: 0,
            types_only: false,
            filter,
            attrs: Vec::new(),
        }
    }
}

impl ProtocolOp {
    /// The operation's application tag.
    #[must_use]
    pub const fn tag(&self) -> u8 {
        use op_tag as o;
        match self {
            Self::BindRequest { .. } => o::BIND_REQUEST,
            Self::BindResponse { .. } => o::BIND_RESPONSE,
            Self::UnbindRequest => o::UNBIND_REQUEST,
            Self::SearchRequest(_) => o::SEARCH_REQUEST,
            Self::SearchResultEntry { .. } => o::SEARCH_RESULT_ENTRY,
            Self::SearchResultReference(_) => o::SEARCH_RESULT_REFERENCE,
            Self::SearchResultDone(_) => o::SEARCH_RESULT_DONE,
            Self::ModifyRequest { .. } => o::MODIFY_REQUEST,
            Self::ModifyResponse(_) => o::MODIFY_RESPONSE,
            Self::AddRequest { .. } => o::ADD_REQUEST,
            Self::AddResponse(_) => o::ADD_RESPONSE,
            Self::DelRequest(_) => o::DEL_REQUEST,
            Self::DelResponse(_) => o::DEL_RESPONSE,
            Self::ModDnRequest { .. } => o::MODDN_REQUEST,
            Self::ModDnResponse(_) => o::MODDN_RESPONSE,
            Self::CompareRequest { .. } => o::COMPARE_REQUEST,
            Self::CompareResponse(_) => o::COMPARE_RESPONSE,
            Self::AbandonRequest(_) => o::ABANDON_REQUEST,
            Self::ExtendedRequest { .. } => o::EXTENDED_REQUEST,
            Self::ExtendedResponse { .. } => o::EXTENDED_RESPONSE,
            Self::IntermediateResponse { .. } => o::INTERMEDIATE_RESPONSE,
        }
    }

    /// The result a response carries.
    #[must_use]
    pub const fn result(&self) -> Option<&LdapResult> {
        match self {
            Self::BindResponse { result, .. } | Self::ExtendedResponse { result, .. } => {
                Some(result)
            }
            Self::SearchResultDone(r)
            | Self::ModifyResponse(r)
            | Self::AddResponse(r)
            | Self::DelResponse(r)
            | Self::ModDnResponse(r)
            | Self::CompareResponse(r) => Some(r),
            _ => None,
        }
    }

    /// Whether a client sends this operation.
    #[must_use]
    pub const fn is_request(&self) -> bool {
        matches!(
            self,
            Self::BindRequest { .. }
                | Self::UnbindRequest
                | Self::SearchRequest(_)
                | Self::ModifyRequest { .. }
                | Self::AddRequest { .. }
                | Self::DelRequest(_)
                | Self::ModDnRequest { .. }
                | Self::CompareRequest { .. }
                | Self::AbandonRequest(_)
                | Self::ExtendedRequest { .. }
        )
    }

    /// Writes the operation.
    pub fn encode(&self, w: &mut Writer) {
        let t = self.tag();
        match self {
            Self::BindRequest {
                version,
                name,
                auth,
            } => w.constructed(t, |w| {
                w.integer(tag::INTEGER, i64::from(*version));
                w.string(tag::OCTET_STRING, name);
                match auth {
                    BindAuth::Simple(p) => w.octets(tag::context(0), p),
                    BindAuth::Sasl {
                        mechanism,
                        credentials,
                    } => w.constructed(tag::context_constructed(3), |w| {
                        w.string(tag::OCTET_STRING, mechanism);
                        if let Some(c) = credentials {
                            w.octets(tag::OCTET_STRING, c);
                        }
                    }),
                }
            }),
            Self::BindResponse {
                result,
                server_sasl_creds,
            } => w.constructed(t, |w| {
                result.encode_fields(w);
                if let Some(c) = server_sasl_creds {
                    w.octets(tag::context(7), c);
                }
            }),
            Self::UnbindRequest => w.null(t),
            Self::SearchRequest(s) => w.constructed(t, |w| {
                w.string(tag::OCTET_STRING, &s.base);
                w.enumerated(tag::ENUMERATED, s.scope.number());
                w.enumerated(tag::ENUMERATED, s.deref.number());
                w.integer(tag::INTEGER, i64::from(s.size_limit));
                w.integer(tag::INTEGER, i64::from(s.time_limit));
                w.boolean(tag::BOOLEAN, s.types_only);
                s.filter.encode(w);
                w.constructed(tag::SEQUENCE, |w| {
                    for a in &s.attrs {
                        w.string(tag::OCTET_STRING, a);
                    }
                });
            }),
            Self::SearchResultEntry { dn, attrs } | Self::AddRequest { dn, attrs } => {
                w.constructed(t, |w| {
                    w.string(tag::OCTET_STRING, dn);
                    encode_attrs(w, attrs);
                });
            }
            Self::SearchResultReference(urls) => w.constructed(t, |w| {
                for u in urls {
                    w.string(tag::OCTET_STRING, u);
                }
            }),
            Self::SearchResultDone(r)
            | Self::ModifyResponse(r)
            | Self::AddResponse(r)
            | Self::DelResponse(r)
            | Self::ModDnResponse(r)
            | Self::CompareResponse(r) => w.constructed(t, |w| r.encode_fields(w)),
            Self::ModifyRequest { dn, changes } => w.constructed(t, |w| {
                w.string(tag::OCTET_STRING, dn);
                w.constructed(tag::SEQUENCE, |w| {
                    for c in changes {
                        w.constructed(tag::SEQUENCE, |w| {
                            w.enumerated(tag::ENUMERATED, c.op.number());
                            c.attr.encode(w);
                        });
                    }
                });
            }),
            Self::DelRequest(dn) => w.string(t, dn),
            Self::ModDnRequest {
                dn,
                new_rdn,
                delete_old_rdn,
                new_superior,
            } => w.constructed(t, |w| {
                w.string(tag::OCTET_STRING, dn);
                w.string(tag::OCTET_STRING, new_rdn);
                w.boolean(tag::BOOLEAN, *delete_old_rdn);
                if let Some(s) = new_superior {
                    w.string(tag::context(0), s);
                }
            }),
            Self::CompareRequest { dn, attr, value } => w.constructed(t, |w| {
                w.string(tag::OCTET_STRING, dn);
                w.constructed(tag::SEQUENCE, |w| {
                    w.string(tag::OCTET_STRING, attr);
                    w.octets(tag::OCTET_STRING, value);
                });
            }),
            Self::AbandonRequest(id) => w.integer(t, i64::from(*id)),
            Self::ExtendedRequest { name, value } => w.constructed(t, |w| {
                w.string(tag::context(0), name);
                if let Some(v) = value {
                    w.octets(tag::context(1), v);
                }
            }),
            Self::ExtendedResponse {
                result,
                name,
                value,
            } => w.constructed(t, |w| {
                result.encode_fields(w);
                if let Some(n) = name {
                    w.string(tag::context(10), n);
                }
                if let Some(v) = value {
                    w.octets(tag::context(11), v);
                }
            }),
            Self::IntermediateResponse { name, value } => w.constructed(t, |w| {
                if let Some(n) = name {
                    w.string(tag::context(0), n);
                }
                if let Some(v) = value {
                    w.octets(tag::context(1), v);
                }
            }),
        }
    }

    /// Reads one operation.
    ///
    /// # Errors
    ///
    /// A [`MsgError`].
    pub fn decode(r: &mut Reader<'_>) -> Result<Self, MsgError> {
        use op_tag as o;
        let t = r.peek_tag().ok_or(BerError::Missing)?;
        let op = match t {
            o::UNBIND_REQUEST => {
                r.null(t)?;
                return Ok(Self::UnbindRequest);
            }
            o::DEL_REQUEST => return Ok(Self::DelRequest(string(r.octets(t)?)?)),
            o::ABANDON_REQUEST => {
                let id = r.integer(t)?;
                return Ok(Self::AbandonRequest(message_id(id)?));
            }
            o::BIND_REQUEST
            | o::BIND_RESPONSE
            | o::SEARCH_REQUEST
            | o::SEARCH_RESULT_ENTRY
            | o::SEARCH_RESULT_DONE
            | o::MODIFY_REQUEST
            | o::MODIFY_RESPONSE
            | o::ADD_REQUEST
            | o::ADD_RESPONSE
            | o::DEL_RESPONSE
            | o::MODDN_REQUEST
            | o::MODDN_RESPONSE
            | o::COMPARE_REQUEST
            | o::COMPARE_RESPONSE
            | o::SEARCH_RESULT_REFERENCE
            | o::EXTENDED_REQUEST
            | o::EXTENDED_RESPONSE
            | o::INTERMEDIATE_RESPONSE => {
                let mut s = r.sequence(t)?;
                let op = Self::decode_body(t, &mut s)?;
                s.finish()?;
                op
            }
            _ => return Err(MsgError::UnknownOp(t)),
        };
        Ok(op)
    }

    fn decode_body(t: u8, s: &mut Reader<'_>) -> Result<Self, MsgError> {
        use op_tag as o;
        Ok(match t {
            o::BIND_REQUEST => {
                let version = s.int32(tag::INTEGER)?;
                if !(1..=127).contains(&version) {
                    return Err(MsgError::Bad("bind version out of range"));
                }
                let name = string(s.octets(tag::OCTET_STRING)?)?;
                let auth = match s.peek_tag() {
                    Some(0x80) => BindAuth::Simple(s.octets(0x80)?.to_vec()),
                    Some(0xA3) => {
                        let mut q = s.sequence(0xA3)?;
                        let mechanism = string(q.octets(tag::OCTET_STRING)?)?;
                        let credentials = q.opt_octets(tag::OCTET_STRING)?.map(<[u8]>::to_vec);
                        q.finish()?;
                        BindAuth::Sasl {
                            mechanism,
                            credentials,
                        }
                    }
                    _ => return Err(MsgError::Bad("unknown bind authentication")),
                };
                Self::BindRequest {
                    version,
                    name,
                    auth,
                }
            }
            o::BIND_RESPONSE => {
                let result = LdapResult::decode_fields(s)?;
                let server_sasl_creds = s.opt_octets(tag::context(7))?.map(<[u8]>::to_vec);
                Self::BindResponse {
                    result,
                    server_sasl_creds,
                }
            }
            o::SEARCH_REQUEST => {
                let base = string(s.octets(tag::OCTET_STRING)?)?;
                let scope = Scope::from_number(s.enumerated(tag::ENUMERATED)?)
                    .ok_or(MsgError::Bad("unknown search scope"))?;
                let deref = DerefAliases::from_number(s.enumerated(tag::ENUMERATED)?)
                    .ok_or(MsgError::Bad("unknown alias dereferencing"))?;
                let size_limit = s.int32(tag::INTEGER)?;
                let time_limit = s.int32(tag::INTEGER)?;
                if size_limit < 0 || time_limit < 0 {
                    return Err(MsgError::Bad("negative search limit"));
                }
                let types_only = s.boolean(tag::BOOLEAN)?;
                let filter = Filter::decode(s)?;
                let mut a = s.sequence(tag::SEQUENCE)?;
                let mut attrs = Vec::new();
                while !a.is_empty() {
                    attrs.push(string(a.octets(tag::OCTET_STRING)?)?);
                }
                Self::SearchRequest(SearchRequest {
                    base,
                    scope,
                    deref,
                    size_limit,
                    time_limit,
                    types_only,
                    filter,
                    attrs,
                })
            }
            o::SEARCH_RESULT_ENTRY | o::ADD_REQUEST => {
                let dn = string(s.octets(tag::OCTET_STRING)?)?;
                let attrs = decode_attrs(s)?;
                if t == o::ADD_REQUEST {
                    Self::AddRequest { dn, attrs }
                } else {
                    Self::SearchResultEntry { dn, attrs }
                }
            }
            o::SEARCH_RESULT_REFERENCE => {
                let mut urls = Vec::new();
                while !s.is_empty() {
                    urls.push(string(s.octets(tag::OCTET_STRING)?)?);
                }
                if urls.is_empty() {
                    return Err(MsgError::Bad("empty search reference"));
                }
                Self::SearchResultReference(urls)
            }
            o::SEARCH_RESULT_DONE => Self::SearchResultDone(LdapResult::decode_fields(s)?),
            o::MODIFY_RESPONSE => Self::ModifyResponse(LdapResult::decode_fields(s)?),
            o::ADD_RESPONSE => Self::AddResponse(LdapResult::decode_fields(s)?),
            o::DEL_RESPONSE => Self::DelResponse(LdapResult::decode_fields(s)?),
            o::MODDN_RESPONSE => Self::ModDnResponse(LdapResult::decode_fields(s)?),
            o::COMPARE_RESPONSE => Self::CompareResponse(LdapResult::decode_fields(s)?),
            o::MODIFY_REQUEST => {
                let dn = string(s.octets(tag::OCTET_STRING)?)?;
                let mut list = s.sequence(tag::SEQUENCE)?;
                let mut changes = Vec::new();
                while !list.is_empty() {
                    let mut c = list.sequence(tag::SEQUENCE)?;
                    let op = ModOp::from_number(c.enumerated(tag::ENUMERATED)?)
                        .ok_or(MsgError::Bad("unknown modify operation"))?;
                    let attr = Attribute::decode(&mut c)?;
                    c.finish()?;
                    changes.push(Modification { op, attr });
                }
                Self::ModifyRequest { dn, changes }
            }
            o::MODDN_REQUEST => {
                let dn = string(s.octets(tag::OCTET_STRING)?)?;
                let new_rdn = string(s.octets(tag::OCTET_STRING)?)?;
                let delete_old_rdn = s.boolean(tag::BOOLEAN)?;
                let new_superior = s.opt_octets(tag::context(0))?.map(string).transpose()?;
                Self::ModDnRequest {
                    dn,
                    new_rdn,
                    delete_old_rdn,
                    new_superior,
                }
            }
            o::COMPARE_REQUEST => {
                let dn = string(s.octets(tag::OCTET_STRING)?)?;
                let mut a = s.sequence(tag::SEQUENCE)?;
                let attr = string(a.octets(tag::OCTET_STRING)?)?;
                let value = a.octets(tag::OCTET_STRING)?.to_vec();
                a.finish()?;
                Self::CompareRequest { dn, attr, value }
            }
            o::EXTENDED_REQUEST => {
                let name = string(s.octets(tag::context(0))?)?;
                let value = s.opt_octets(tag::context(1))?.map(<[u8]>::to_vec);
                Self::ExtendedRequest { name, value }
            }
            o::EXTENDED_RESPONSE => {
                let result = LdapResult::decode_fields(s)?;
                let name = s.opt_octets(tag::context(10))?.map(string).transpose()?;
                let value = s.opt_octets(tag::context(11))?.map(<[u8]>::to_vec);
                Self::ExtendedResponse {
                    result,
                    name,
                    value,
                }
            }
            o::INTERMEDIATE_RESPONSE => {
                let name = s.opt_octets(tag::context(0))?.map(string).transpose()?;
                let value = s.opt_octets(tag::context(1))?.map(<[u8]>::to_vec);
                Self::IntermediateResponse { name, value }
            }
            _ => return Err(MsgError::UnknownOp(t)),
        })
    }
}

fn message_id(n: i64) -> Result<i32, MsgError> {
    i32::try_from(n)
        .ok()
        .filter(|v| *v >= 0)
        .ok_or(MsgError::Bad("message ID out of range"))
}

impl Control {
    /// A control of `oid`.
    #[must_use]
    pub fn new(oid: &str, critical: bool, value: Option<Vec<u8>>) -> Self {
        Self {
            oid: oid.to_string(),
            critical,
            value,
        }
    }

    /// Writes the control (criticality only when true, as DEFAULT
    /// FALSE asks).
    pub fn encode(&self, w: &mut Writer) {
        w.constructed(tag::SEQUENCE, |w| {
            w.string(tag::OCTET_STRING, &self.oid);
            if self.critical {
                w.boolean(tag::BOOLEAN, true);
            }
            if let Some(v) = &self.value {
                w.octets(tag::OCTET_STRING, v);
            }
        });
    }

    /// Reads one control.
    ///
    /// # Errors
    ///
    /// A [`MsgError`].
    pub fn decode(r: &mut Reader<'_>) -> Result<Self, MsgError> {
        let mut s = r.sequence(tag::SEQUENCE)?;
        let oid = string(s.octets(tag::OCTET_STRING)?)?;
        let critical = if s.at(tag::BOOLEAN) {
            s.boolean(tag::BOOLEAN)?
        } else {
            false
        };
        let value = s.opt_octets(tag::OCTET_STRING)?.map(<[u8]>::to_vec);
        s.finish()?;
        Ok(Self {
            oid,
            critical,
            value,
        })
    }
}

impl LdapMessage {
    /// A message of `id` with no controls.
    #[must_use]
    pub const fn new(id: i32, op: ProtocolOp) -> Self {
        Self {
            id,
            op,
            controls: Vec::new(),
        }
    }

    /// The control of `oid`, if the message carries it.
    #[must_use]
    pub fn control(&self, oid: &str) -> Option<&Control> {
        self.controls.iter().find(|c| c.oid == oid)
    }

    /// Writes the message.
    pub fn encode_into(&self, w: &mut Writer) {
        w.constructed(tag::SEQUENCE, |w| {
            w.integer(tag::INTEGER, i64::from(self.id));
            self.op.encode(w);
            if !self.controls.is_empty() {
                w.constructed(tag::context_constructed(0), |w| {
                    for c in &self.controls {
                        c.encode(w);
                    }
                });
            }
        });
    }

    /// The message's BER encoding.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.encode_into(&mut w);
        w.into_bytes()
    }

    /// Decodes exactly one message with the default limits.
    ///
    /// # Errors
    ///
    /// A [`MsgError`].
    pub fn decode(b: &[u8]) -> Result<Self, MsgError> {
        Self::decode_with_limit(b, crate::ber::MAX_LEN)
    }

    /// Decodes exactly one message whose elements are at most `max_len`.
    ///
    /// # Errors
    ///
    /// A [`MsgError`].
    pub fn decode_with_limit(b: &[u8], max_len: usize) -> Result<Self, MsgError> {
        let mut r = Reader::with_limits(b, max_len, crate::ber::MAX_DEPTH);
        let m = Self::read(&mut r)?;
        r.finish()?;
        Ok(m)
    }

    /// Reads one message from `r`.
    ///
    /// # Errors
    ///
    /// A [`MsgError`].
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MsgError> {
        let mut s = r.sequence(tag::SEQUENCE)?;
        let id = message_id(s.integer(tag::INTEGER)?)?;
        let op = ProtocolOp::decode(&mut s)?;
        let mut controls = Vec::new();
        if s.at(tag::context_constructed(0)) {
            let mut c = s.sequence(tag::context_constructed(0))?;
            while !c.is_empty() {
                controls.push(Control::decode(&mut c)?);
            }
        }
        s.finish()?;
        Ok(Self { id, op, controls })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Attribute, BindAuth, Control, DerefAliases, LdapMessage, LdapResult, ModOp, Modification,
        MsgError, ProtocolOp, Scope, SearchRequest,
    };
    use crate::code::ResultCode;
    use crate::filter::Filter;

    fn both(bytes: &[u8], m: &LdapMessage) {
        assert_eq!(m.encode(), bytes, "encode {m:?}");
        assert_eq!(&LdapMessage::decode(bytes).unwrap(), m, "decode");
    }

    #[test]
    fn hand_vectors() {
        // Simple bind, v3, "cn=a", "x".
        both(
            &[
                0x30, 0x11, 0x02, 0x01, 0x01, 0x60, 0x0C, 0x02, 0x01, 0x03, 0x04, 0x04, b'c', b'n',
                b'=', b'a', 0x80, 0x01, b'x',
            ],
            &LdapMessage::new(
                1,
                ProtocolOp::BindRequest {
                    version: 3,
                    name: "cn=a".into(),
                    auth: BindAuth::Simple(b"x".to_vec()),
                },
            ),
        );
        // Bind response, success.
        both(
            &[
                0x30, 0x0C, 0x02, 0x01, 0x01, 0x61, 0x07, 0x0A, 0x01, 0x00, 0x04, 0x00, 0x04, 0x00,
            ],
            &LdapMessage::new(
                1,
                ProtocolOp::BindResponse {
                    result: LdapResult::success(),
                    server_sasl_creds: None,
                },
            ),
        );
        // Unbind.
        both(
            &[0x30, 0x05, 0x02, 0x01, 0x03, 0x42, 0x00],
            &LdapMessage::new(3, ProtocolOp::UnbindRequest),
        );
        // Delete "cn=a".
        both(
            &[
                0x30, 0x09, 0x02, 0x01, 0x05, 0x4A, 0x04, b'c', b'n', b'=', b'a',
            ],
            &LdapMessage::new(5, ProtocolOp::DelRequest("cn=a".into())),
        );
        // Abandon 5.
        both(
            &[0x30, 0x06, 0x02, 0x01, 0x06, 0x50, 0x01, 0x05],
            &LdapMessage::new(6, ProtocolOp::AbandonRequest(5)),
        );
        // Root DSE search: base "", base scope, (objectClass=*), "+".
        let mut b = vec![0x30, 0x28, 0x02, 0x01, 0x02, 0x63, 0x23];
        b.extend_from_slice(&[0x04, 0x00, 0x0A, 0x01, 0x00, 0x0A, 0x01, 0x00]);
        b.extend_from_slice(&[0x02, 0x01, 0x00, 0x02, 0x01, 0x00, 0x01, 0x01, 0x00]);
        b.extend_from_slice(&[0x87, 0x0B]);
        b.extend_from_slice(b"objectClass");
        b.extend_from_slice(&[0x30, 0x03, 0x04, 0x01, b'+']);
        let mut req = SearchRequest::new(String::new(), Scope::Base, Filter::any());
        req.attrs = vec!["+".into()];
        both(&b, &LdapMessage::new(2, ProtocolOp::SearchRequest(req)));
        // Delete with manageDsaIT, critical.
        let mut b = vec![
            0x30, 0x29, 0x02, 0x01, 0x05, 0x4A, 0x04, b'c', b'n', b'=', b'a',
        ];
        b.extend_from_slice(&[0xA0, 0x1E, 0x30, 0x1C, 0x04, 0x17]);
        b.extend_from_slice(b"2.16.840.1.113730.3.4.2");
        b.extend_from_slice(&[0x01, 0x01, 0xFF]);
        let mut m = LdapMessage::new(5, ProtocolOp::DelRequest("cn=a".into()));
        m.controls
            .push(Control::new(crate::control::MANAGE_DSA_IT, true, None));
        both(&b, &m);
        // Search done, noSuchObject, matched "dc=x", referral.
        let mut b = vec![0x30, 0x1A, 0x02, 0x01, 0x02, 0x65, 0x15, 0x0A, 0x01, 0x20];
        b.extend_from_slice(&[0x04, 0x04, b'd', b'c', b'=', b'x', 0x04, 0x00]);
        b.extend_from_slice(&[0xA3, 0x08, 0x04, 0x06]);
        b.extend_from_slice(b"ldap:/");
        let mut r = LdapResult::new(ResultCode::NoSuchObject);
        r.matched_dn = "dc=x".into();
        r.referral = vec!["ldap:/".into()];
        both(&b, &LdapMessage::new(2, ProtocolOp::SearchResultDone(r)));
        // StartTLS request.
        let mut b = vec![0x30, 0x1D, 0x02, 0x01, 0x01, 0x77, 0x18, 0x80, 0x16];
        b.extend_from_slice(b"1.3.6.1.4.1.1466.20037");
        both(
            &b,
            &LdapMessage::new(
                1,
                ProtocolOp::ExtendedRequest {
                    name: crate::ext::START_TLS.into(),
                    value: None,
                },
            ),
        );
    }

    fn result() -> LdapResult {
        LdapResult {
            code: ResultCode::Other(4096),
            matched_dn: "dc=example".into(),
            message: "why".into(),
            referral: vec!["ldap://a/".into(), "ldap://b/".into()],
        }
    }

    #[test]
    fn every_op_round_trips() {
        let attrs = vec![
            Attribute::new("cn", &["a", "b"]),
            Attribute {
                ty: "jpegPhoto".into(),
                vals: vec![vec![0, 1, 255]],
            },
            Attribute::new("empty", &[]),
        ];
        let ops = vec![
            ProtocolOp::BindRequest {
                version: 2,
                name: String::new(),
                auth: BindAuth::Simple(Vec::new()),
            },
            ProtocolOp::BindRequest {
                version: 3,
                name: "cn=x".into(),
                auth: BindAuth::Sasl {
                    mechanism: "EXTERNAL".into(),
                    credentials: None,
                },
            },
            ProtocolOp::BindRequest {
                version: 3,
                name: String::new(),
                auth: BindAuth::Sasl {
                    mechanism: "PLAIN".into(),
                    credentials: Some(b"\0u\0p".to_vec()),
                },
            },
            ProtocolOp::BindResponse {
                result: result(),
                server_sasl_creds: Some(b"c".to_vec()),
            },
            ProtocolOp::UnbindRequest,
            ProtocolOp::SearchRequest(SearchRequest {
                base: "dc=example,dc=com".into(),
                scope: Scope::Sub,
                deref: DerefAliases::Always,
                size_limit: 500,
                time_limit: 900,
                types_only: true,
                filter: Filter::parse("(&(objectClass=person)(|(cn=a*)(sn:dn:=b)))").unwrap(),
                attrs: vec!["cn".into(), "*".into(), "+".into(), "1.1".into()],
            }),
            ProtocolOp::SearchResultEntry {
                dn: "cn=a".into(),
                attrs: attrs.clone(),
            },
            ProtocolOp::SearchResultReference(vec!["ldap://h/dc=x".into()]),
            ProtocolOp::SearchResultDone(LdapResult::success()),
            ProtocolOp::ModifyRequest {
                dn: "cn=a".into(),
                changes: [ModOp::Add, ModOp::Delete, ModOp::Replace, ModOp::Increment]
                    .into_iter()
                    .map(|op| Modification {
                        op,
                        attr: Attribute::new("n", &["1"]),
                    })
                    .collect(),
            },
            ProtocolOp::ModifyResponse(result()),
            ProtocolOp::AddRequest {
                dn: "cn=a".into(),
                attrs,
            },
            ProtocolOp::AddResponse(result()),
            ProtocolOp::DelRequest("cn=a".into()),
            ProtocolOp::DelResponse(result()),
            ProtocolOp::ModDnRequest {
                dn: "cn=a,dc=x".into(),
                new_rdn: "cn=b".into(),
                delete_old_rdn: true,
                new_superior: Some("dc=y".into()),
            },
            ProtocolOp::ModDnRequest {
                dn: "cn=a,dc=x".into(),
                new_rdn: "cn=b".into(),
                delete_old_rdn: false,
                new_superior: None,
            },
            ProtocolOp::ModDnResponse(result()),
            ProtocolOp::CompareRequest {
                dn: "cn=a".into(),
                attr: "sn".into(),
                value: b"x".to_vec(),
            },
            ProtocolOp::CompareResponse(LdapResult::new(ResultCode::CompareTrue)),
            ProtocolOp::AbandonRequest(i32::MAX),
            ProtocolOp::ExtendedRequest {
                name: "1.2.3".into(),
                value: Some(vec![1, 2]),
            },
            ProtocolOp::ExtendedResponse {
                result: result(),
                name: Some("1.2.3".into()),
                value: Some(vec![]),
            },
            ProtocolOp::ExtendedResponse {
                result: LdapResult::success(),
                name: None,
                value: None,
            },
            ProtocolOp::IntermediateResponse {
                name: Some("1.2".into()),
                value: Some(vec![9]),
            },
            ProtocolOp::IntermediateResponse {
                name: None,
                value: None,
            },
        ];
        for (i, op) in ops.into_iter().enumerate() {
            let mut m = LdapMessage::new(i32::try_from(i).unwrap(), op);
            if i % 3 == 0 {
                m.controls = vec![
                    Control::new("1.2.3", false, None),
                    Control::new("1.2.4", true, Some(vec![0x30, 0])),
                ];
            }
            let b = m.encode();
            assert_eq!(LdapMessage::decode(&b).unwrap(), m);
        }
    }

    #[test]
    fn refusals() {
        let bad: [&[u8]; 9] = [
            // Negative message ID.
            &[0x30, 0x05, 0x02, 0x01, 0xFF, 0x42, 0x00],
            // Unknown operation tag.
            &[0x30, 0x05, 0x02, 0x01, 0x01, 0x51, 0x00],
            // Trailing byte inside the message.
            &[0x30, 0x06, 0x02, 0x01, 0x01, 0x42, 0x00, 0x00],
            // Bytes after the message.
            &[0x30, 0x05, 0x02, 0x01, 0x01, 0x42, 0x00, 0x00],
            // Unbind with contents.
            &[0x30, 0x06, 0x02, 0x01, 0x01, 0x42, 0x01, 0x00],
            // Constructed DN in a delete.
            &[0x30, 0x07, 0x02, 0x01, 0x01, 0x6A, 0x02, 0x04, 0x00],
            // Bind with version 0.
            &[
                0x30, 0x0C, 0x02, 0x01, 0x01, 0x60, 0x07, 0x02, 0x01, 0x00, 0x04, 0x00, 0x80, 0x00,
            ],
            // Indefinite length.
            &[0x30, 0x80, 0x02, 0x01, 0x01, 0x42, 0x00, 0x00, 0x00],
            // Non-UTF-8 DN.
            &[0x30, 0x06, 0x02, 0x01, 0x01, 0x4A, 0x01, 0xFF],
        ];
        for b in bad {
            assert!(LdapMessage::decode(b).is_err(), "{b:02x?}");
        }
        assert!(matches!(
            LdapMessage::decode(&[0x30, 0x05, 0x02, 0x01, 0x01, 0x51, 0x00]),
            Err(MsgError::UnknownOp(0x51))
        ));
        let big = LdapMessage::new(1, ProtocolOp::DelRequest("x".repeat(100))).encode();
        assert!(LdapMessage::decode_with_limit(&big, 50).is_err());
    }
}
