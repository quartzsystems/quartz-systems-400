// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The local code page: `ldap_set_iconv_local_codepage`,
//! `ldap_set_iconv_local_charset`, `ldap_get_iconv_local_codepage`, the
//! `ldap_xlate_*` conversions between it and UTF-8 or UCS-2 (big
//! endian), and `ldap_char_size`.

use mi::Job;

use super::{ret, with};
use crate::consts::*;
use crate::mem;

/// The CCSID a character set name gives: `IBM-nnn`, `IBMnnn`, `CPnnn`,
/// `UTF-8`, `ISO8859-1`, or digits.
#[must_use]
pub fn charset_ccsid(name: &str) -> Option<u16> {
    let n = name.trim().to_ascii_uppercase().replace('_', "-");
    let cp = match n.as_str() {
        "UTF-8" | "UTF8" => 1208,
        "ISO8859-1" | "ISO-8859-1" | "LATIN1" => 819,
        _ => {
            let digits = n
                .strip_prefix("IBM-")
                .or_else(|| n.strip_prefix("IBM"))
                .or_else(|| n.strip_prefix("CP"))
                .unwrap_or(&n);
            digits.parse::<u16>().ok()?
        }
    };
    let c = ccsid::conv::ccsid_of_codepage(cp);
    ccsid::conv::supported().contains(&c).then_some(c)
}

/// `int ldap_set_iconv_local_codepage(int codepage)`.
pub fn set_codepage(j: &mut Job, cp: i64) -> i64 {
    let Ok(cp) = u16::try_from(cp) else {
        return ret(LDAP_PARAM_ERROR);
    };
    let c = ccsid::conv::ccsid_of_codepage(cp);
    if !ccsid::conv::supported().contains(&c) {
        return ret(LDAP_PARAM_ERROR);
    }
    with(j, |s, _| {
        s.ccsid = c;
        ret(LDAP_SUCCESS)
    })
}

/// `int ldap_set_iconv_local_charset(const char *charset)`.
pub fn set_charset(j: &mut Job, name: u128) -> i64 {
    let Some(b) = mem::cbytes(j, name) else {
        return ret(LDAP_PARAM_ERROR);
    };
    // The name is in the current local code page.
    let n = with(j, |s, _| s.codec().text(&b));
    match charset_ccsid(&n) {
        Some(c) => with(j, |s, _| {
            s.ccsid = c;
            ret(LDAP_SUCCESS)
        }),
        None => ret(LDAP_PARAM_ERROR),
    }
}

/// `int ldap_get_iconv_local_codepage(void)`.
pub fn get_codepage(j: &mut Job) -> i64 {
    with(j, |s, _| i64::from(ccsid::conv::codepage_of(s.ccsid)))
}

/// Converts the program's `inlen` bytes into `out` (room `*outlen`);
/// `*outlen` gets the bytes written, or the bytes needed when they do
/// not fit (`LDAP_NO_MEMORY`).
fn xlate(
    j: &mut Job,
    inp: u128,
    inlen: u128,
    out: u128,
    outlen: u128,
    f: impl FnOnce(&[u8], u16) -> Option<Vec<u8>>,
) -> i64 {
    let (Some(n), Some(room)) = (mem::get_u32(j, inlen), mem::get_u32(j, outlen)) else {
        return ret(LDAP_PARAM_ERROR);
    };
    let src = if n == 0 {
        Vec::new()
    } else {
        match mem::bytes(j, inp, n as usize) {
            Some(b) => b.to_vec(),
            None => return ret(LDAP_PARAM_ERROR),
        }
    };
    let ccsid = with(j, |s, _| s.ccsid);
    let Some(conv) = f(&src, ccsid) else {
        return ret(LDAP_LOCAL_ERROR);
    };
    let len = u32::try_from(conv.len()).unwrap_or(u32::MAX);
    if len > room {
        mem::put_u32(j, outlen, len);
        return ret(LDAP_NO_MEMORY);
    }
    if !conv.is_empty() {
        let Some(o) = mem::bytes(j, out, conv.len()) else {
            return ret(LDAP_PARAM_ERROR);
        };
        o.copy_from_slice(&conv);
    }
    mem::put_u32(j, outlen, len);
    ret(LDAP_SUCCESS)
}

fn ucs2(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

fn from_ucs2(b: &[u8]) -> Option<String> {
    if !b.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = b
        .chunks(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).ok()
}

fn local(b: &[u8], c: u16) -> Option<String> {
    ccsid::conv::to_string(c, b).ok()
}

fn to_local(s: &str, c: u16) -> Option<Vec<u8>> {
    ccsid::conv::from_str(c, s).ok()
}

/// `int ldap_xlate_local_to_utf8(char *inbuf, unsigned long *inlen,
/// char *outbuf, unsigned long *outlen)`.
pub fn local_to_utf8(j: &mut Job, i: u128, il: u128, o: u128, ol: u128) -> i64 {
    xlate(j, i, il, o, ol, |b, c| local(b, c).map(String::into_bytes))
}

/// `int ldap_xlate_utf8_to_local(char *inbuf, unsigned long *inlen,
/// char *outbuf, unsigned long *outlen)`.
pub fn utf8_to_local(j: &mut Job, i: u128, il: u128, o: u128, ol: u128) -> i64 {
    xlate(j, i, il, o, ol, |b, c| {
        to_local(std::str::from_utf8(b).ok()?, c)
    })
}

/// `int ldap_xlate_local_to_unicode(char *inbuf, unsigned long *inlen,
/// char *outbuf, unsigned long *outlen)`: UCS-2, big endian.
pub fn local_to_unicode(j: &mut Job, i: u128, il: u128, o: u128, ol: u128) -> i64 {
    xlate(j, i, il, o, ol, |b, c| local(b, c).map(|s| ucs2(&s)))
}

/// `int ldap_xlate_unicode_to_local(char *inbuf, unsigned long *inlen,
/// char *outbuf, unsigned long *outlen)`.
pub fn unicode_to_local(j: &mut Job, i: u128, il: u128, o: u128, ol: u128) -> i64 {
    xlate(j, i, il, o, ol, |b, c| to_local(&from_ucs2(b)?, c))
}

/// `int ldap_char_size(char *p)`: the bytes of the character at `p` in
/// the local code page: its UTF-8 sequence's length for 1208, else 1.
pub fn char_size(j: &mut Job, p: u128) -> i64 {
    let Some(b) = mem::bytes(j, p, 1) else {
        return 0;
    };
    let first = b[0];
    let c = with(j, |s, _| s.ccsid);
    if c != 1208 {
        return 1;
    }
    match first {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charsets() {
        assert_eq!(charset_ccsid("IBM-037"), Some(37));
        assert_eq!(charset_ccsid("UTF-8"), Some(1208));
        assert_eq!(charset_ccsid("ISO8859-1"), Some(819));
        assert_eq!(charset_ccsid("nonsense"), None);
        assert_eq!(from_ucs2(&ucs2("Aé")).unwrap(), "Aé");
    }
}
