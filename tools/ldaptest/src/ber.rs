// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The few BER values ldaptest builds and reads itself: the control values
//! `ldap3` does not have (sort, persistent search, entry change
//! notification). Definite lengths only, as LDAP requires (RFC 4511 §5.1).

use crate::Error;

/// Universal tags used here.
pub mod tag {
    /// BOOLEAN.
    pub const BOOLEAN: u8 = 0x01;
    /// INTEGER.
    pub const INTEGER: u8 = 0x02;
    /// OCTET STRING.
    pub const OCTET_STRING: u8 = 0x04;
    /// ENUMERATED.
    pub const ENUMERATED: u8 = 0x0a;
    /// SEQUENCE (constructed).
    pub const SEQUENCE: u8 = 0x30;
}

/// The length octets of `n`.
fn length(n: usize, out: &mut Vec<u8>) {
    if n < 0x80 {
        out.push(n as u8);
    } else {
        let bytes = n.to_be_bytes();
        let skip = bytes.iter().take_while(|b| **b == 0).count();
        out.push(0x80 | (bytes.len() - skip) as u8);
        out.extend_from_slice(&bytes[skip..]);
    }
}

/// One TLV.
pub fn tlv(t: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![t];
    length(body.len(), &mut out);
    out.extend_from_slice(body);
    out
}

/// A constructed value of `parts`.
pub fn seq(t: u8, parts: &[Vec<u8>]) -> Vec<u8> {
    tlv(t, &parts.concat())
}

/// An INTEGER (or ENUMERATED with `t`), two's complement, minimal.
pub fn int(t: u8, v: i64) -> Vec<u8> {
    let bytes = v.to_be_bytes();
    let mut i = 0;
    while i < 7
        && ((bytes[i] == 0 && bytes[i + 1] & 0x80 == 0)
            || (bytes[i] == 0xff && bytes[i + 1] & 0x80 != 0))
    {
        i += 1;
    }
    tlv(t, &bytes[i..])
}

/// A BOOLEAN.
pub fn boolean(v: bool) -> Vec<u8> {
    tlv(tag::BOOLEAN, &[if v { 0xff } else { 0 }])
}

/// An OCTET STRING.
pub fn octets(v: &[u8]) -> Vec<u8> {
    tlv(tag::OCTET_STRING, v)
}

/// A reader over BER bytes.
#[derive(Debug)]
pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> Reader<'a> {
    /// Reads `buf`.
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf }
    }

    /// Nothing left.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// The next tag, not consumed.
    pub fn peek(&self) -> Option<u8> {
        self.buf.first().copied()
    }

    /// The next TLV: its tag and body.
    pub fn next_tlv(&mut self) -> Result<(u8, &'a [u8]), Error> {
        let short = || Error::Decode("BER value cut short".into());
        let (&t, rest) = self.buf.split_first().ok_or_else(short)?;
        let (&l0, mut rest) = rest.split_first().ok_or_else(short)?;
        let len = if l0 < 0x80 {
            usize::from(l0)
        } else {
            let n = usize::from(l0 & 0x7f);
            if n == 0 || n > 4 || rest.len() < n {
                return Err(Error::Decode(format!("BER length form {l0:#04x}")));
            }
            let v = rest[..n].iter().fold(0usize, |a, b| (a << 8) | usize::from(*b));
            rest = &rest[n..];
            v
        };
        if rest.len() < len {
            return Err(short());
        }
        let (body, tail) = rest.split_at(len);
        self.buf = tail;
        Ok((t, body))
    }

    /// The next TLV, which must have tag `t`.
    pub fn expect(&mut self, t: u8) -> Result<&'a [u8], Error> {
        let (got, body) = self.next_tlv()?;
        if got == t {
            Ok(body)
        } else {
            Err(Error::Decode(format!("tag {got:#04x}, wanted {t:#04x}")))
        }
    }

    /// An INTEGER or ENUMERATED body with tag `t`.
    pub fn int(&mut self, t: u8) -> Result<i64, Error> {
        let body = self.expect(t)?;
        if body.is_empty() || body.len() > 8 {
            return Err(Error::Decode(format!("integer of {} octets", body.len())));
        }
        let first = if body[0] & 0x80 != 0 { -1i64 } else { 0 };
        Ok(body.iter().fold(first, |a, b| (a << 8) | i64::from(*b)))
    }

    /// A BOOLEAN.
    pub fn boolean(&mut self) -> Result<bool, Error> {
        let body = self.expect(tag::BOOLEAN)?;
        match body {
            [v] => Ok(*v != 0),
            _ => Err(Error::Decode("boolean not one octet".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_are_minimal() {
        assert_eq!(int(tag::INTEGER, 0), [2, 1, 0]);
        assert_eq!(int(tag::INTEGER, 127), [2, 1, 0x7f]);
        assert_eq!(int(tag::INTEGER, 128), [2, 2, 0, 0x80]);
        assert_eq!(int(tag::INTEGER, -1), [2, 1, 0xff]);
        assert_eq!(int(tag::INTEGER, -129), [2, 2, 0xff, 0x7f]);
        for v in [0, 1, -1, 255, 256, -256, 1 << 40, i64::MIN, i64::MAX] {
            let b = int(tag::INTEGER, v);
            assert_eq!(Reader::new(&b).int(tag::INTEGER).unwrap(), v);
        }
    }

    #[test]
    fn long_lengths_round_trip() {
        let body = vec![7u8; 300];
        let b = octets(&body);
        assert_eq!(&b[..4], &[4, 0x82, 0x01, 0x2c]);
        let mut r = Reader::new(&b);
        assert_eq!(r.expect(tag::OCTET_STRING).unwrap(), &body[..]);
        assert!(r.is_empty());
    }

    #[test]
    fn short_input_is_an_error() {
        assert!(Reader::new(&[4, 5, 1]).next_tlv().is_err());
        assert!(Reader::new(&[4]).next_tlv().is_err());
        assert!(Reader::new(&[4, 0x85, 0, 0, 0, 0, 1]).next_tlv().is_err());
    }

    #[test]
    fn sequence_of_parts() {
        let s = seq(tag::SEQUENCE, &[int(tag::INTEGER, 15), boolean(true)]);
        assert_eq!(s, [0x30, 6, 2, 1, 15, 1, 1, 0xff]);
        let mut r = Reader::new(r_body(&s));
        assert_eq!(r.int(tag::INTEGER).unwrap(), 15);
        assert!(r.boolean().unwrap());
    }

    fn r_body(b: &[u8]) -> &[u8] {
        Reader::new(b).expect(tag::SEQUENCE).unwrap()
    }
}
