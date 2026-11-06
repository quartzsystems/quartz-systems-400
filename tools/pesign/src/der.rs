// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The few DER forms the signature needs, written and read by hand:
//! Authenticode's structures are not in the `der` crate's types.

use crate::Error;

/// `SEQUENCE`.
pub const SEQ: u8 = 0x30;
/// `SET`.
pub const SET: u8 = 0x31;
/// `INTEGER`.
pub const INT: u8 = 0x02;
/// `OCTET STRING`.
pub const OCTETS: u8 = 0x04;
/// `NULL`.
pub const NULL: u8 = 0x05;
/// `OBJECT IDENTIFIER`.
pub const OID: u8 = 0x06;
/// `BIT STRING`.
pub const BITS: u8 = 0x03;

/// A tag, its length and `content`.
#[must_use]
pub fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let n = content.len();
    if n < 0x80 {
        out.push(u8::try_from(n).unwrap_or(0));
    } else {
        let bytes: Vec<u8> = n.to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
        out.push(0x80 | u8::try_from(bytes.len()).unwrap_or(0));
        out.extend(bytes);
    }
    out.extend_from_slice(content);
    out
}

/// A constructed value of the concatenated `parts`.
#[must_use]
pub fn cons(tag: u8, parts: &[&[u8]]) -> Vec<u8> {
    tlv(tag, &parts.concat())
}

/// An object identifier from its dotted form.
#[must_use]
pub fn oid(dotted: &str) -> Vec<u8> {
    let arcs: Vec<u64> = dotted.split('.').filter_map(|a| a.parse().ok()).collect();
    let mut body = Vec::new();
    if arcs.len() >= 2 {
        let mut push = |mut v: u64| {
            let mut tmp = vec![u8::try_from(v & 0x7F).unwrap_or(0)];
            v >>= 7;
            while v > 0 {
                tmp.push(0x80 | u8::try_from(v & 0x7F).unwrap_or(0));
                v >>= 7;
            }
            tmp.reverse();
            body.extend(tmp);
        };
        push(arcs[0] * 40 + arcs[1]);
        for a in &arcs[2..] {
            push(*a);
        }
    }
    tlv(OID, &body)
}

/// A small non-negative `INTEGER`.
#[must_use]
pub fn int(v: u8) -> Vec<u8> {
    tlv(INT, &[v])
}

/// An `AlgorithmIdentifier` with `NULL` parameters.
#[must_use]
pub fn alg(dotted: &str) -> Vec<u8> {
    cons(SEQ, &[&oid(dotted), &tlv(NULL, &[])])
}

/// A `SET OF` in DER order: the members sorted by their encodings.
#[must_use]
pub fn set_of(mut members: Vec<Vec<u8>>, tag: u8) -> Vec<u8> {
    members.sort();
    tlv(tag, &members.concat())
}

/// One value read back: its tag, the whole encoding and its content.
#[derive(Clone, Copy, Debug)]
pub struct Tlv<'a> {
    /// The tag byte.
    pub tag: u8,
    /// Tag, length and content.
    pub whole: &'a [u8],
    /// The content.
    pub content: &'a [u8],
}

/// The value at the start of `b` and what follows it.
pub fn read(b: &[u8]) -> Result<(Tlv<'_>, &[u8]), Error> {
    let bad = || Error::Signature("truncated DER".into());
    let tag = *b.first().ok_or_else(bad)?;
    let first = *b.get(1).ok_or_else(bad)?;
    let (len, head) = if first < 0x80 {
        (usize::from(first), 2)
    } else {
        let n = usize::from(first & 0x7F);
        if n == 0 || n > 4 {
            return Err(Error::Signature("unsupported DER length".into()));
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | usize::from(*b.get(2 + i).ok_or_else(bad)?);
        }
        (len, 2 + n)
    };
    let end = head.checked_add(len).filter(|e| *e <= b.len()).ok_or_else(bad)?;
    Ok((Tlv { tag, whole: &b[..end], content: &b[head..end] }, &b[end..]))
}

/// Every value in `b`, in order.
pub fn items(mut b: &[u8]) -> Result<Vec<Tlv<'_>>, Error> {
    let mut out = Vec::new();
    while !b.is_empty() {
        let (t, rest) = read(b)?;
        out.push(t);
        b = rest;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oids_and_lengths() {
        assert_eq!(oid("1.2.840.113549.1.7.2"), [0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02]);
        assert_eq!(oid("2.16.840.1.101.3.4.2.1")[..4], [0x06, 0x09, 0x60, 0x86]);
        let long = tlv(OCTETS, &[0u8; 300]);
        assert_eq!(&long[..4], &[0x04, 0x82, 0x01, 0x2C]);
        let (t, rest) = read(&long).unwrap();
        assert_eq!((t.tag, t.content.len(), rest.len()), (OCTETS, 300, 0));
        assert!(read(&long[..100]).is_err());
    }

    #[test]
    fn sets_sort() {
        let s = set_of(vec![vec![0x02, 0x01, 0x05], vec![0x02, 0x01, 0x01]], SET);
        assert_eq!(s, [0x31, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x05]);
    }
}
