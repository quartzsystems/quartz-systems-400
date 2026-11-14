// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Controls (RFC 4511 §4.1.11) and the OIDs of those QS/400 knows
//! (docs/DIRSRV.md §7). The typed values of each are this module's:
//! each implements [`ControlValue`] over its OID.

use crate::ber::{BerError, Reader, Writer, tag};
use crate::code::ResultCode;
use crate::filter::{Filter, FilterError};
use crate::msg::{Attribute, MsgError, ProtocolOp};

/// A control as carried: its type, criticality and value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    /// The control's OID.
    pub oid: String,
    /// Whether it is critical.
    pub critical: bool,
    /// Its value, if any.
    pub value: Option<Vec<u8>>,
}

/// Simple paged results (RFC 2696).
pub const PAGED_RESULTS: &str = "1.2.840.113556.1.4.319";
/// Server-side sort request (RFC 2891).
pub const SORT_REQUEST: &str = "1.2.840.113556.1.4.473";
/// Server-side sort response (RFC 2891).
pub const SORT_RESPONSE: &str = "1.2.840.113556.1.4.474";
/// manageDsaIT (RFC 3296).
pub const MANAGE_DSA_IT: &str = "2.16.840.1.113730.3.4.2";
/// Persistent search.
pub const PERSISTENT_SEARCH: &str = "2.16.840.1.113730.3.4.3";
/// Entry change notification (persistent search's response).
pub const ENTRY_CHANGE: &str = "2.16.840.1.113730.3.4.7";
/// Proxied authorization v2 (RFC 4370).
pub const PROXIED_AUTHZ: &str = "2.16.840.1.113730.3.4.18";
/// Tree delete.
pub const TREE_DELETE: &str = "1.2.840.113556.1.4.805";
/// Assertion (RFC 4528).
pub const ASSERTION: &str = "1.3.6.1.1.12";
/// Pre-read (RFC 4527).
pub const PRE_READ: &str = "1.3.6.1.1.13.1";
/// Post-read (RFC 4527).
pub const POST_READ: &str = "1.3.6.1.1.13.2";
/// Password policy (draft-behera).
pub const PASSWORD_POLICY: &str = "1.3.6.1.4.1.42.2.27.8.5.1";

/// The password policy request (draft-behera): the same OID, no value.
pub const PASSWORD_POLICY_REQUEST: &str = PASSWORD_POLICY;

/// Why a control's value does not decode.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ControlError {
    /// Not well-formed BER.
    #[error("bad control value: {0}")]
    Ber(#[from] BerError),
    /// A bad filter (assertion).
    #[error("bad control value: {0}")]
    Filter(#[from] FilterError),
    /// A bad entry (pre-read, post-read).
    #[error("bad control value: {0}")]
    Msg(#[from] MsgError),
    /// Well-formed but wrong.
    #[error("bad control value: {0}")]
    Bad(&'static str),
    /// Another control than the one asked for.
    #[error("control {found} is not {expected}")]
    Oid {
        /// The OID asked for.
        expected: &'static str,
        /// The control's OID.
        found: String,
    },
}

/// A control with a typed value.
pub trait ControlValue: Sized {
    /// The control's OID.
    const OID: &'static str;

    /// The value's bytes, none for a control without one.
    fn encode_value(&self) -> Option<Vec<u8>>;

    /// The typed value of a control's bytes.
    ///
    /// # Errors
    ///
    /// A [`ControlError`].
    fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError>;

    /// The control carrying this value.
    fn to_control(&self, critical: bool) -> Control {
        Control {
            oid: Self::OID.to_string(),
            critical,
            value: self.encode_value(),
        }
    }

    /// The typed value of `c`.
    ///
    /// # Errors
    ///
    /// [`ControlError::Oid`] for another control, or the value's error.
    fn from_control(c: &Control) -> Result<Self, ControlError> {
        if c.oid != Self::OID {
            return Err(ControlError::Oid {
                expected: Self::OID,
                found: c.oid.clone(),
            });
        }
        Self::decode_value(c.value.as_deref())
    }

    /// The first control of this type in `list`, decoded.
    ///
    /// # Errors
    ///
    /// The value's error.
    fn find(list: &[Control]) -> Result<Option<Self>, ControlError> {
        list.iter()
            .find(|c| c.oid == Self::OID)
            .map(Self::from_control)
            .transpose()
    }
}

fn need(v: Option<&[u8]>) -> Result<&[u8], ControlError> {
    v.ok_or(ControlError::Bad("the control needs a value"))
}

fn none(v: Option<&[u8]>) -> Result<(), ControlError> {
    match v {
        None => Ok(()),
        Some(_) => Err(ControlError::Bad("the control takes no value")),
    }
}

fn text(b: &[u8]) -> Result<String, ControlError> {
    String::from_utf8(b.to_vec()).map_err(|_| ControlError::Bad("string is not UTF-8"))
}

fn seq(f: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut w = Writer::new();
    w.constructed(tag::SEQUENCE, f);
    w.into_bytes()
}

/// Runs `f` over the one SEQUENCE in `v`.
fn read_seq<T>(
    v: &[u8],
    f: impl FnOnce(&mut Reader<'_>) -> Result<T, ControlError>,
) -> Result<T, ControlError> {
    let mut r = Reader::new(v);
    let mut s = r.sequence(tag::SEQUENCE)?;
    r.finish()?;
    let out = f(&mut s)?;
    s.finish()?;
    Ok(out)
}

/// Simple paged results (RFC 2696), request and response.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PagedResults {
    /// The page size asked for, or the server's estimate of the total.
    pub size: i32,
    /// The cookie (empty on the first request and the last response).
    pub cookie: Vec<u8>,
}

impl ControlValue for PagedResults {
    const OID: &'static str = PAGED_RESULTS;

    fn encode_value(&self) -> Option<Vec<u8>> {
        Some(seq(|w| {
            w.integer(tag::INTEGER, i64::from(self.size));
            w.octets(tag::OCTET_STRING, &self.cookie);
        }))
    }

    fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
        read_seq(need(v)?, |s| {
            Ok(Self {
                size: s.int32(tag::INTEGER)?,
                cookie: s.octets(tag::OCTET_STRING)?.to_vec(),
            })
        })
    }
}

/// One key of a sort request (RFC 2891).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortKey {
    /// The attribute description.
    pub attr: String,
    /// The ordering rule, if not the attribute's own.
    pub rule: Option<String>,
    /// Descending.
    pub reverse: bool,
}

impl SortKey {
    /// An ascending key on `attr`.
    #[must_use]
    pub fn new(attr: &str) -> Self {
        Self {
            attr: attr.to_string(),
            rule: None,
            reverse: false,
        }
    }
}

/// The server-side sort request (RFC 2891).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortRequest(pub Vec<SortKey>);

impl ControlValue for SortRequest {
    const OID: &'static str = SORT_REQUEST;

    fn encode_value(&self) -> Option<Vec<u8>> {
        Some(seq(|w| {
            for k in &self.0 {
                w.constructed(tag::SEQUENCE, |w| {
                    w.string(tag::OCTET_STRING, &k.attr);
                    if let Some(r) = &k.rule {
                        w.string(tag::context(0), r);
                    }
                    if k.reverse {
                        w.boolean(tag::context(1), true);
                    }
                });
            }
        }))
    }

    fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
        read_seq(need(v)?, |s| {
            let mut keys = Vec::new();
            while !s.is_empty() {
                let mut k = s.sequence(tag::SEQUENCE)?;
                let attr = text(k.octets(tag::OCTET_STRING)?)?;
                let rule = k.opt_octets(tag::context(0))?.map(text).transpose()?;
                let reverse = if k.at(tag::context(1)) {
                    k.boolean(tag::context(1))?
                } else {
                    false
                };
                k.finish()?;
                keys.push(SortKey {
                    attr,
                    rule,
                    reverse,
                });
            }
            if keys.is_empty() {
                return Err(ControlError::Bad("a sort request needs a key"));
            }
            Ok(Self(keys))
        })
    }
}

/// The server-side sort response (RFC 2891).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortResponse {
    /// The sort's result.
    pub result: ResultCode,
    /// The attribute that failed, if any.
    pub attr: Option<String>,
}

impl ControlValue for SortResponse {
    const OID: &'static str = SORT_RESPONSE;

    fn encode_value(&self) -> Option<Vec<u8>> {
        Some(seq(|w| {
            w.enumerated(tag::ENUMERATED, i64::from(self.result.number()));
            if let Some(a) = &self.attr {
                w.string(tag::context(0), a);
            }
        }))
    }

    fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
        read_seq(need(v)?, |s| {
            let n = s.enumerated(tag::ENUMERATED)?;
            let n = u32::try_from(n).map_err(|_| ControlError::Bad("negative sort result"))?;
            Ok(Self {
                result: ResultCode::from_number(n),
                attr: s.opt_octets(tag::context(0))?.map(text).transpose()?,
            })
        })
    }
}

/// A control without a value: manageDsaIT, tree delete, the password
/// policy request.
macro_rules! unit_control {
    ($(#[$m:meta])* $name:ident, $oid:expr) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;

        impl ControlValue for $name {
            const OID: &'static str = $oid;

            fn encode_value(&self) -> Option<Vec<u8>> {
                None
            }

            fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
                none(v)?;
                Ok(Self)
            }
        }
    };
}

unit_control!(
    /// manageDsaIT (RFC 3296).
    ManageDsaIt,
    MANAGE_DSA_IT
);
unit_control!(
    /// Tree delete.
    TreeDelete,
    TREE_DELETE
);
unit_control!(
    /// The password policy request.
    PasswordPolicyRequest,
    PASSWORD_POLICY_REQUEST
);

/// The change types of a persistent search (bits).
pub mod change_type {
    /// An entry was added.
    pub const ADD: i32 = 1;
    /// An entry was deleted.
    pub const DELETE: i32 = 2;
    /// An entry was modified.
    pub const MODIFY: i32 = 4;
    /// An entry was renamed.
    pub const MODDN: i32 = 8;
    /// All of them.
    pub const ALL: i32 = 15;
}

/// The persistent search request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistentSearch {
    /// The [`change_type`] bits wanted.
    pub change_types: i32,
    /// Changes only (no initial entries).
    pub changes_only: bool,
    /// Whether entries carry [`EntryChange`] controls.
    pub return_ecs: bool,
}

impl ControlValue for PersistentSearch {
    const OID: &'static str = PERSISTENT_SEARCH;

    fn encode_value(&self) -> Option<Vec<u8>> {
        Some(seq(|w| {
            w.integer(tag::INTEGER, i64::from(self.change_types));
            w.boolean(tag::BOOLEAN, self.changes_only);
            w.boolean(tag::BOOLEAN, self.return_ecs);
        }))
    }

    fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
        read_seq(need(v)?, |s| {
            Ok(Self {
                change_types: s.int32(tag::INTEGER)?,
                changes_only: s.boolean(tag::BOOLEAN)?,
                return_ecs: s.boolean(tag::BOOLEAN)?,
            })
        })
    }
}

/// The entry change notification of a persistent search's entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryChange {
    /// One [`change_type`] value.
    pub change_type: i32,
    /// The DN before a rename.
    pub previous_dn: Option<String>,
    /// The change log number.
    pub change_number: Option<i64>,
}

impl ControlValue for EntryChange {
    const OID: &'static str = ENTRY_CHANGE;

    fn encode_value(&self) -> Option<Vec<u8>> {
        Some(seq(|w| {
            w.enumerated(tag::ENUMERATED, i64::from(self.change_type));
            if let Some(d) = &self.previous_dn {
                w.string(tag::OCTET_STRING, d);
            }
            if let Some(n) = self.change_number {
                w.integer(tag::INTEGER, n);
            }
        }))
    }

    fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
        read_seq(need(v)?, |s| {
            let change_type = i32::try_from(s.enumerated(tag::ENUMERATED)?)
                .map_err(|_| ControlError::Bad("bad change type"))?;
            let previous_dn = s.opt_octets(tag::OCTET_STRING)?.map(text).transpose()?;
            let change_number = if s.at(tag::INTEGER) {
                Some(s.integer(tag::INTEGER)?)
            } else {
                None
            };
            Ok(Self {
                change_type,
                previous_dn,
                change_number,
            })
        })
    }
}

/// Proxied authorization v2 (RFC 4370): the authorization identity
/// (`dn:...`, `u:...`, or empty for anonymous), carried as the value
/// itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxiedAuthz(pub String);

impl ControlValue for ProxiedAuthz {
    const OID: &'static str = PROXIED_AUTHZ;

    fn encode_value(&self) -> Option<Vec<u8>> {
        Some(self.0.as_bytes().to_vec())
    }

    fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
        Ok(Self(text(need(v)?)?))
    }
}

/// The assertion control (RFC 4528): a filter the target must match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assertion(pub Filter);

impl ControlValue for Assertion {
    const OID: &'static str = ASSERTION;

    fn encode_value(&self) -> Option<Vec<u8>> {
        Some(self.0.to_ber())
    }

    fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
        Ok(Self(Filter::from_ber(need(v)?)?))
    }
}

fn encode_selection(attrs: &[String]) -> Vec<u8> {
    seq(|w| {
        for a in attrs {
            w.string(tag::OCTET_STRING, a);
        }
    })
}

fn decode_selection(v: Option<&[u8]>) -> Result<Vec<String>, ControlError> {
    read_seq(need(v)?, |s| {
        let mut out = Vec::new();
        while !s.is_empty() {
            out.push(text(s.octets(tag::OCTET_STRING)?)?);
        }
        Ok(out)
    })
}

/// An entry as the read controls return it (a `SearchResultEntry`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadEntry {
    /// The entry's DN.
    pub dn: String,
    /// Its attributes.
    pub attrs: Vec<Attribute>,
}

fn encode_entry(e: &ReadEntry) -> Vec<u8> {
    let mut w = Writer::new();
    ProtocolOp::SearchResultEntry {
        dn: e.dn.clone(),
        attrs: e.attrs.clone(),
    }
    .encode(&mut w);
    w.into_bytes()
}

fn decode_entry(v: Option<&[u8]>) -> Result<ReadEntry, ControlError> {
    let mut r = Reader::new(need(v)?);
    let op = ProtocolOp::decode(&mut r)?;
    r.finish()?;
    match op {
        ProtocolOp::SearchResultEntry { dn, attrs } => Ok(ReadEntry { dn, attrs }),
        _ => Err(ControlError::Bad("expected an entry")),
    }
}

macro_rules! read_controls {
    ($(#[$rm:meta])* $req:ident, $(#[$sm:meta])* $resp:ident, $oid:expr) => {
        $(#[$rm])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $req(pub Vec<String>);

        impl ControlValue for $req {
            const OID: &'static str = $oid;

            fn encode_value(&self) -> Option<Vec<u8>> {
                Some(encode_selection(&self.0))
            }

            fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
                Ok(Self(decode_selection(v)?))
            }
        }

        $(#[$sm])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $resp(pub ReadEntry);

        impl ControlValue for $resp {
            const OID: &'static str = $oid;

            fn encode_value(&self) -> Option<Vec<u8>> {
                Some(encode_entry(&self.0))
            }

            fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
                Ok(Self(decode_entry(v)?))
            }
        }
    };
}

read_controls!(
    /// The pre-read request (RFC 4527): the attributes wanted.
    PreReadRequest,
    /// The pre-read response: the entry before the change.
    PreReadResponse,
    PRE_READ
);
read_controls!(
    /// The post-read request (RFC 4527): the attributes wanted.
    PostReadRequest,
    /// The post-read response: the entry after the change.
    PostReadResponse,
    POST_READ
);

/// A password policy warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyWarning {
    /// Seconds until the password expires.
    TimeBeforeExpiration(i32),
    /// Grace binds left.
    GraceAuthNsRemaining(i32),
}

/// A password policy error (draft-behera's values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum PolicyError {
    PasswordExpired,
    AccountLocked,
    ChangeAfterReset,
    PasswordModNotAllowed,
    MustSupplyOldPassword,
    InsufficientPasswordQuality,
    PasswordTooShort,
    PasswordTooYoung,
    PasswordInHistory,
}

impl PolicyError {
    const ALL: [Self; 9] = [
        Self::PasswordExpired,
        Self::AccountLocked,
        Self::ChangeAfterReset,
        Self::PasswordModNotAllowed,
        Self::MustSupplyOldPassword,
        Self::InsufficientPasswordQuality,
        Self::PasswordTooShort,
        Self::PasswordTooYoung,
        Self::PasswordInHistory,
    ];

    /// The wire value.
    #[must_use]
    pub fn number(self) -> i64 {
        Self::ALL
            .iter()
            .position(|e| *e == self)
            .and_then(|p| i64::try_from(p).ok())
            .unwrap_or(0)
    }

    /// The error of a wire value.
    #[must_use]
    pub fn from_number(n: i64) -> Option<Self> {
        usize::try_from(n)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
    }
}

/// The password policy response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PasswordPolicyResponse {
    /// A warning, if any.
    pub warning: Option<PolicyWarning>,
    /// An error, if any.
    pub error: Option<PolicyError>,
}

impl ControlValue for PasswordPolicyResponse {
    const OID: &'static str = PASSWORD_POLICY;

    fn encode_value(&self) -> Option<Vec<u8>> {
        Some(seq(|w| {
            if let Some(warn) = self.warning {
                w.constructed(tag::context_constructed(0), |w| match warn {
                    PolicyWarning::TimeBeforeExpiration(n) => {
                        w.integer(tag::context(0), i64::from(n));
                    }
                    PolicyWarning::GraceAuthNsRemaining(n) => {
                        w.integer(tag::context(1), i64::from(n));
                    }
                });
            }
            if let Some(e) = self.error {
                w.enumerated(tag::context(1), e.number());
            }
        }))
    }

    fn decode_value(v: Option<&[u8]>) -> Result<Self, ControlError> {
        read_seq(need(v)?, |s| {
            let mut out = Self::default();
            if s.at(tag::context_constructed(0)) {
                let mut c = s.sequence(tag::context_constructed(0))?;
                let n = |c: &mut Reader<'_>, t| c.int32(t);
                out.warning = Some(match c.peek_tag() {
                    Some(0x80) => PolicyWarning::TimeBeforeExpiration(n(&mut c, 0x80)?),
                    Some(0x81) => PolicyWarning::GraceAuthNsRemaining(n(&mut c, 0x81)?),
                    _ => return Err(ControlError::Bad("unknown password policy warning")),
                });
                c.finish()?;
            }
            if s.at(tag::context(1)) {
                out.error = Some(
                    PolicyError::from_number(s.enumerated(tag::context(1))?)
                        .ok_or(ControlError::Bad("unknown password policy error"))?,
                );
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Assertion, ControlError, ControlValue, EntryChange, ManageDsaIt, PagedResults,
        PasswordPolicyRequest, PasswordPolicyResponse, PersistentSearch, PolicyError,
        PolicyWarning, PostReadRequest, PostReadResponse, PreReadRequest, PreReadResponse,
        ProxiedAuthz, ReadEntry, SortKey, SortRequest, SortResponse, TreeDelete, change_type,
    };
    use crate::code::ResultCode;
    use crate::filter::Filter;
    use crate::msg::Attribute;

    fn round<T: ControlValue + PartialEq + std::fmt::Debug>(v: &T) {
        let c = v.to_control(true);
        assert_eq!(c.oid, T::OID);
        assert_eq!(&T::from_control(&c).unwrap(), v);
    }

    #[test]
    fn by_hand() {
        // Paged results, size 100, empty cookie: SEQUENCE { INTEGER 100, "" }.
        let p = PagedResults {
            size: 100,
            cookie: vec![],
        };
        assert_eq!(
            p.encode_value().unwrap(),
            [0x30, 0x05, 0x02, 0x01, 0x64, 0x04, 0x00]
        );
        // Sort on cn descending: SEQUENCE { SEQUENCE { "cn", [1] TRUE } }.
        let s = SortRequest(vec![SortKey {
            reverse: true,
            ..SortKey::new("cn")
        }]);
        assert_eq!(
            s.encode_value().unwrap(),
            [
                0x30, 0x09, 0x30, 0x07, 0x04, 0x02, b'c', b'n', 0x81, 0x01, 0xFF
            ]
        );
        // Sort response, noSuchAttribute on "sn".
        let r = SortResponse {
            result: ResultCode::NoSuchAttribute,
            attr: Some("sn".into()),
        };
        assert_eq!(
            r.encode_value().unwrap(),
            [0x30, 0x07, 0x0A, 0x01, 0x10, 0x80, 0x02, b's', b'n']
        );
        // Password policy: grace 3, error passwordExpired.
        let pp = PasswordPolicyResponse {
            warning: Some(PolicyWarning::GraceAuthNsRemaining(3)),
            error: Some(PolicyError::PasswordExpired),
        };
        assert_eq!(
            pp.encode_value().unwrap(),
            [0x30, 0x08, 0xA0, 0x03, 0x81, 0x01, 0x03, 0x81, 0x01, 0x00]
        );
        assert_eq!(
            PasswordPolicyResponse::decode_value(Some(&[0x30, 0x00])).unwrap(),
            PasswordPolicyResponse::default()
        );
        // Proxied authorization: the bare string.
        assert_eq!(
            ProxiedAuthz("dn:cn=a".into()).encode_value().unwrap(),
            b"dn:cn=a"
        );
    }

    #[test]
    fn round_trips() {
        round(&PagedResults {
            size: 5,
            cookie: vec![1, 2, 3],
        });
        round(&SortRequest(vec![
            SortKey::new("sn"),
            SortKey {
                attr: "cn".into(),
                rule: Some("2.5.13.3".into()),
                reverse: true,
            },
        ]));
        round(&SortResponse {
            result: ResultCode::Success,
            attr: None,
        });
        round(&ManageDsaIt);
        round(&TreeDelete);
        round(&PasswordPolicyRequest);
        round(&PersistentSearch {
            change_types: change_type::ALL,
            changes_only: true,
            return_ecs: false,
        });
        round(&EntryChange {
            change_type: change_type::MODDN,
            previous_dn: Some("cn=old".into()),
            change_number: Some(42),
        });
        round(&EntryChange {
            change_type: change_type::ADD,
            previous_dn: None,
            change_number: None,
        });
        round(&ProxiedAuthz(String::new()));
        round(&Assertion(Filter::parse("(cn=a*)").unwrap()));
        round(&PreReadRequest(vec!["cn".into(), "+".into()]));
        round(&PostReadRequest(vec![]));
        let e = ReadEntry {
            dn: "cn=a".into(),
            attrs: vec![Attribute::new("cn", &["a"])],
        };
        round(&PreReadResponse(e.clone()));
        round(&PostReadResponse(e));
        round(&PasswordPolicyResponse {
            warning: Some(PolicyWarning::TimeBeforeExpiration(3600)),
            error: Some(PolicyError::PasswordInHistory),
        });
    }

    #[test]
    fn refusals() {
        assert!(matches!(
            ManageDsaIt::decode_value(Some(b"x")),
            Err(ControlError::Bad(_))
        ));
        assert!(PagedResults::decode_value(None).is_err());
        assert!(PagedResults::decode_value(Some(&[0x30, 0x03, 0x02, 0x01, 0x05])).is_err());
        assert!(
            PagedResults::decode_value(Some(&[0x30, 0x05, 0x02, 0x01, 0x64, 0x04, 0x00, 0x00]))
                .is_err()
        );
        assert!(SortRequest::decode_value(Some(&[0x30, 0x00])).is_err());
        assert!(
            PasswordPolicyResponse::decode_value(Some(&[0x30, 0x03, 0x81, 0x01, 0x09])).is_err()
        );
        let c = TreeDelete.to_control(false);
        assert!(matches!(
            ManageDsaIt::from_control(&c),
            Err(ControlError::Oid { .. })
        ));
        assert_eq!(TreeDelete::find(&[c]).unwrap(), Some(TreeDelete));
        assert_eq!(ManageDsaIt::find(&[]).unwrap(), None);
    }
}
