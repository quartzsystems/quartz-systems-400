// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The little DER the keys need: a PKCS#1 `RSAPublicKey` (n, e) read and
//! written, and the last BIT STRING of a `SubjectPublicKeyInfo`.

/// Read one TLV: tag, contents, rest.
fn tlv(b: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, b) = b.split_first()?;
    let (&first, mut b) = b.split_first()?;
    let len = if first < 0x80 {
        usize::from(first)
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 4 || b.len() < n {
            return None;
        }
        let mut len = 0usize;
        for &x in &b[..n] {
            len = (len << 8) | usize::from(x);
        }
        b = &b[n..];
        len
    };
    if b.len() < len {
        return None;
    }
    Some((tag, &b[..len], &b[len..]))
}

fn header(tag: u8, len: usize, out: &mut Vec<u8>) {
    out.push(tag);
    if len < 0x80 {
        out.push(u8::try_from(len).unwrap_or(0));
    } else {
        let bytes = len.to_be_bytes();
        let skip = bytes.iter().take_while(|b| **b == 0).count();
        out.push(0x80 | u8::try_from(bytes.len() - skip).unwrap_or(0));
        out.extend_from_slice(&bytes[skip..]);
    }
}

fn integer(v: &[u8], out: &mut Vec<u8>) {
    let n = v.iter().take_while(|b| **b == 0).count();
    let v = &v[n..];
    let pad = v.first().is_none_or(|b| b & 0x80 != 0);
    header(0x02, v.len() + usize::from(pad), out);
    if pad {
        out.push(0);
    }
    out.extend_from_slice(v);
}

/// `RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }`.
#[must_use]
pub fn rsa_public(n: &[u8], e: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    integer(n, &mut body);
    integer(e, &mut body);
    let mut out = Vec::new();
    header(0x30, body.len(), &mut out);
    out.extend_from_slice(&body);
    out
}

/// (n, e) of an `RSAPublicKey`, without leading zeros.
#[must_use]
pub fn rsa_public_parts(der: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let (tag, seq, _) = tlv(der)?;
    if tag != 0x30 {
        return None;
    }
    let (t1, n, rest) = tlv(seq)?;
    let (t2, e, _) = tlv(rest)?;
    if t1 != 2 || t2 != 2 {
        return None;
    }
    let trim = |v: &[u8]| v[v.iter().take_while(|b| **b == 0).count()..].to_vec();
    Some((trim(n), trim(e)))
}

/// The key bits of a `SubjectPublicKeyInfo` (the BIT STRING, unused-bits
/// byte dropped).
#[must_use]
pub fn spki_key(der: &[u8]) -> Option<Vec<u8>> {
    let (tag, seq, _) = tlv(der)?;
    if tag != 0x30 {
        return None;
    }
    let (_, _, rest) = tlv(seq)?;
    let (tag, bits, _) = tlv(rest)?;
    if tag != 0x03 || bits.is_empty() {
        return None;
    }
    Some(bits[1..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsa_round_trip() {
        let n: Vec<u8> = (0..384u32).map(|i| u8::try_from(i % 251).unwrap() | 0x80).collect();
        let der = rsa_public(&n, &[1, 0, 1]);
        let (n2, e2) = rsa_public_parts(&der).unwrap();
        assert_eq!(n2, n);
        assert_eq!(e2, [1, 0, 1]);
    }
}
