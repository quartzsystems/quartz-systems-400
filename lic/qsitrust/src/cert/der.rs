// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The few DER elements an identity's certificate and key are made of.

/// DER tags.
pub(crate) const SEQUENCE: u8 = 0x30;
pub(crate) const SET: u8 = 0x31;
pub(crate) const INTEGER: u8 = 0x02;
pub(crate) const BIT_STRING: u8 = 0x03;
pub(crate) const OCTET_STRING: u8 = 0x04;
pub(crate) const OID: u8 = 0x06;
pub(crate) const UTF8_STRING: u8 = 0x0c;
pub(crate) const PRINTABLE_STRING: u8 = 0x13;
pub(crate) const UTC_TIME: u8 = 0x17;
pub(crate) const GENERALIZED_TIME: u8 = 0x18;
pub(crate) const BOOLEAN: u8 = 0x01;

/// One element: its tag, its length and `content`.
pub(crate) fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = content.len();
    if len < 0x80 {
        out.push(u8::try_from(len).unwrap_or(0));
    } else {
        let bytes = len.to_be_bytes();
        let skip = bytes.iter().take_while(|b| **b == 0).count();
        out.push(0x80 | u8::try_from(bytes.len() - skip).unwrap_or(0));
        out.extend_from_slice(&bytes[skip..]);
    }
    out.extend_from_slice(content);
    out
}

/// A sequence of `parts`.
pub(crate) fn sequence(parts: &[Vec<u8>]) -> Vec<u8> {
    der(SEQUENCE, &parts.concat())
}

/// A bit string of whole bytes.
pub(crate) fn bits(bytes: &[u8]) -> Vec<u8> {
    der(BIT_STRING, &[&[0u8][..], bytes].concat())
}

/// The element at the start of `bytes`: its tag, its content and what
/// follows it; `None` when it does not read.
pub(crate) fn open(bytes: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let tag = *bytes.first()?;
    let first = *bytes.get(1)?;
    let (len, head) = if first < 0x80 {
        (usize::from(first), 2)
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 4 {
            return None;
        }
        let len = bytes
            .get(2..2 + n)?
            .iter()
            .fold(0usize, |acc, b| acc << 8 | usize::from(*b));
        (len, 2 + n)
    };
    let end = head.checked_add(len)?;
    Some((tag, bytes.get(head..end)?, bytes.get(end..)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_and_reading_back() {
        assert_eq!(der(OCTET_STRING, &[1; 3]), [4, 3, 1, 1, 1]);
        assert_eq!(&der(OCTET_STRING, &[0; 200])[..3], [4, 0x81, 200]);
        assert_eq!(&der(OCTET_STRING, &[0; 300])[..4], [4, 0x82, 1, 44]);
        let two = [der(INTEGER, &[5]), der(OID, &[1, 2])].concat();
        let (tag, body, rest) = open(&two).unwrap();
        assert_eq!((tag, body), (INTEGER, &[5u8][..]));
        assert_eq!(open(rest).unwrap().1, [1, 2]);
        assert!(open(&[0x30, 0x85, 0, 0, 0, 0, 1]).is_none());
        assert!(open(&[0x30, 5, 1]).is_none());
        assert!(open(&[]).is_none());
    }
}
