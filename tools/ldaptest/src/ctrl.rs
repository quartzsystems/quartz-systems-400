// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Controls `ldap3` does not build, written from their specifications: the
//! server-side sort request and response (RFC 2891), persistent search and
//! entry change notification (draft-ietf-ldapext-psearch-03), tree delete
//! (draft-armijo-ldap-treedelete). `ldap3` carries them as raw controls.

use ldap3::controls::RawControl;

use crate::Error;
use crate::ber::{self, Reader, tag};

/// Paged results (RFC 2696).
pub const PAGED: &str = "1.2.840.113556.1.4.319";
/// Sort request (RFC 2891).
pub const SORT_REQUEST: &str = "1.2.840.113556.1.4.473";
/// Sort response.
pub const SORT_RESPONSE: &str = "1.2.840.113556.1.4.474";
/// Tree delete.
pub const TREE_DELETE: &str = "1.2.840.113556.1.4.805";
/// manageDsaIT (RFC 3296).
pub const MANAGE_DSA_IT: &str = "2.16.840.1.113730.3.4.2";
/// Persistent search.
pub const PSEARCH: &str = "2.16.840.1.113730.3.4.3";
/// Entry change notification.
pub const ECN: &str = "2.16.840.1.113730.3.4.7";
/// Proxied authorization version 2 (RFC 4370).
pub const PROXY_AUTHZ: &str = "2.16.840.1.113730.3.4.18";
/// Assertion (RFC 4528).
pub const ASSERTION: &str = "1.3.6.1.1.12";
/// Pre-read (RFC 4527).
pub const PRE_READ: &str = "1.3.6.1.1.13.1";
/// Post-read (RFC 4527).
pub const POST_READ: &str = "1.3.6.1.1.13.2";

/// One sort key.
#[derive(Clone, Debug)]
pub struct SortKey {
    /// The attribute.
    pub attr: String,
    /// An ordering rule, when not the attribute's.
    pub rule: Option<String>,
    /// Descending.
    pub reverse: bool,
}

impl SortKey {
    /// Ascending by `attr`.
    pub fn asc(attr: &str) -> Self {
        SortKey {
            attr: attr.into(),
            rule: None,
            reverse: false,
        }
    }

    /// Descending by `attr`.
    pub fn desc(attr: &str) -> Self {
        SortKey {
            attr: attr.into(),
            rule: None,
            reverse: true,
        }
    }
}

/// The sort request control: `SEQUENCE OF SEQUENCE { attributeType,
/// orderingRule [0] OPTIONAL, reverseOrder [1] BOOLEAN DEFAULT FALSE }`.
pub fn sort_request(keys: &[SortKey], critical: bool) -> RawControl {
    let list: Vec<Vec<u8>> = keys
        .iter()
        .map(|k| {
            let mut parts = vec![ber::octets(k.attr.as_bytes())];
            if let Some(r) = &k.rule {
                parts.push(ber::tlv(0x80, r.as_bytes()));
            }
            if k.reverse {
                parts.push(ber::tlv(0x81, &[0xff]));
            }
            ber::seq(tag::SEQUENCE, &parts)
        })
        .collect();
    RawControl {
        ctype: SORT_REQUEST.into(),
        crit: critical,
        val: Some(ber::seq(tag::SEQUENCE, &list)),
    }
}

/// The sort response: its result code and the attribute it names.
#[derive(Debug, PartialEq, Eq)]
pub struct SortResponse {
    /// `sortResult`.
    pub result: i64,
    /// `attributeType`, when the server names one.
    pub attr: Option<String>,
}

/// Decodes a sort response control's value.
pub fn sort_response(val: &[u8]) -> Result<SortResponse, Error> {
    let mut r = Reader::new(Reader::new(val).expect(tag::SEQUENCE)?);
    let result = r.int(tag::ENUMERATED)?;
    let attr = if r.peek() == Some(0x80) {
        Some(String::from_utf8_lossy(r.expect(0x80)?).into_owned())
    } else {
        None
    };
    Ok(SortResponse { result, attr })
}

/// Persistent search change types.
pub mod change {
    /// add.
    pub const ADD: i64 = 1;
    /// delete.
    pub const DELETE: i64 = 2;
    /// modify.
    pub const MODIFY: i64 = 4;
    /// modDN.
    pub const MODDN: i64 = 8;
}

/// The persistent search control: `SEQUENCE { changeTypes INTEGER,
/// changesOnly BOOLEAN, returnECs BOOLEAN }`, always critical.
pub fn psearch(types: i64, changes_only: bool, return_ecs: bool) -> RawControl {
    RawControl {
        ctype: PSEARCH.into(),
        crit: true,
        val: Some(ber::seq(
            tag::SEQUENCE,
            &[
                ber::int(tag::INTEGER, types),
                ber::boolean(changes_only),
                ber::boolean(return_ecs),
            ],
        )),
    }
}

/// An entry change notification.
#[derive(Debug, PartialEq, Eq)]
pub struct EntryChange {
    /// One of [`change`].
    pub kind: i64,
    /// The entry's DN before a modDN.
    pub previous_dn: Option<String>,
    /// The change number, when the server keeps a change log.
    pub number: Option<i64>,
}

/// Decodes an entry change notification's value.
pub fn entry_change(val: &[u8]) -> Result<EntryChange, Error> {
    let mut r = Reader::new(Reader::new(val).expect(tag::SEQUENCE)?);
    let kind = r.int(tag::ENUMERATED)?;
    let mut previous_dn = None;
    let mut number = None;
    if r.peek() == Some(tag::OCTET_STRING) {
        previous_dn = Some(String::from_utf8_lossy(r.expect(tag::OCTET_STRING)?).into_owned());
    }
    if r.peek() == Some(tag::INTEGER) {
        number = Some(r.int(tag::INTEGER)?);
    }
    Ok(EntryChange {
        kind,
        previous_dn,
        number,
    })
}

/// A control with no value.
pub fn bare(oid: &str, critical: bool) -> RawControl {
    RawControl {
        ctype: oid.into(),
        crit: critical,
        val: None,
    }
}

/// The first control of type `oid` among `ctrls`, as raw.
pub fn find<'a>(ctrls: &'a [ldap3::controls::Control], oid: &str) -> Option<&'a RawControl> {
    ctrls.iter().map(|c| &c.1).find(|r| r.ctype == oid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_request_bytes() {
        let c = sort_request(&[SortKey::asc("cn"), SortKey::desc("sn")], true);
        assert_eq!(c.ctype, SORT_REQUEST);
        assert!(c.crit);
        assert_eq!(
            c.val.unwrap(),
            [
                0x30, 0x0f, 0x30, 0x04, 0x04, 0x02, b'c', b'n', 0x30, 0x07, 0x04, 0x02, b's', b'n',
                0x81, 0x01, 0xff
            ]
        );
    }

    #[test]
    fn sort_response_forms() {
        assert_eq!(
            sort_response(&[0x30, 0x03, 0x0a, 0x01, 0x00]).unwrap(),
            SortResponse {
                result: 0,
                attr: None
            }
        );
        assert_eq!(
            sort_response(&[0x30, 0x07, 0x0a, 0x01, 0x10, 0x80, 0x02, b'c', b'n']).unwrap(),
            SortResponse {
                result: 16,
                attr: Some("cn".into())
            }
        );
        assert!(sort_response(&[0x30, 0x02, 0x0a, 0x01]).is_err());
    }

    #[test]
    fn psearch_and_ecn() {
        let c = psearch(15, true, true);
        assert_eq!(
            c.val.unwrap(),
            [
                0x30, 0x09, 0x02, 0x01, 0x0f, 0x01, 0x01, 0xff, 0x01, 0x01, 0xff
            ]
        );
        let ecn = [
            0x30, 0x0b, 0x0a, 0x01, 0x08, 0x04, 0x03, b'o', b'=', b'x', 0x02, 0x01, 0x07,
        ];
        assert_eq!(
            entry_change(&ecn).unwrap(),
            EntryChange {
                kind: 8,
                previous_dn: Some("o=x".into()),
                number: Some(7)
            }
        );
        assert_eq!(
            entry_change(&[0x30, 0x03, 0x0a, 0x01, 0x01]).unwrap(),
            EntryChange {
                kind: 1,
                previous_dn: None,
                number: None
            }
        );
    }
}
