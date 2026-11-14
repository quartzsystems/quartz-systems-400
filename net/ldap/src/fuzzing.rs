// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Entry points for the fuzz targets (docs/DIRSRV.md §13). Each takes
//! any bytes and must return without a panic or a hang. Where a parser
//! has a printer, what parses is printed, parsed again and compared; a
//! difference is a bug and panics.

use crate::ber::{Reader, tag};
use crate::dn::Dn;
use crate::filter::Filter;
use crate::ldif;
use crate::msg::LdapMessage;
use crate::schema::SchemaValue;
use crate::url::LdapUrl;

/// Walks every element of `r`, into constructed ones, until the first
/// error.
fn walk(r: &mut Reader<'_>) {
    while !r.is_empty() {
        let Some(t) = r.peek_tag() else { return };
        let ok = if t & tag::CONSTRUCTED != 0 {
            r.sequence(t).map(|mut inner| walk(&mut inner)).is_ok()
        } else {
            match t {
                tag::INTEGER | tag::ENUMERATED => r.integer(t).is_ok(),
                tag::BOOLEAN => r.boolean(t).is_ok(),
                _ => r.element().is_ok(),
            }
        };
        if !ok {
            return;
        }
    }
}

/// BER: the reader over arbitrary nesting, with a small length limit.
pub fn ber(data: &[u8]) {
    let mut r = Reader::with_limits(data, 1 << 16, crate::ber::MAX_DEPTH);
    walk(&mut r);
    let _ = crate::ber::frame_len(data, 1 << 16);
}

/// `LDAPMessage`: decode; what decodes encodes and decodes the same.
pub fn msg(data: &[u8]) {
    if let Ok(m) = LdapMessage::decode(data) {
        let again = LdapMessage::decode(&m.encode()).expect("an encoded message decodes");
        assert_eq!(again, m, "message round trip");
        for c in &m.controls {
            known_control(c);
        }
    }
}

/// Decodes a control's value as the type its OID names.
fn known_control(c: &crate::control::Control) {
    use crate::control::{self as k, ControlValue};
    let _ = match c.oid.as_str() {
        k::PAGED_RESULTS => k::PagedResults::from_control(c).map(drop),
        k::SORT_REQUEST => k::SortRequest::from_control(c).map(drop),
        k::SORT_RESPONSE => k::SortResponse::from_control(c).map(drop),
        k::PERSISTENT_SEARCH => k::PersistentSearch::from_control(c).map(drop),
        k::ENTRY_CHANGE => k::EntryChange::from_control(c).map(drop),
        k::PROXIED_AUTHZ => k::ProxiedAuthz::from_control(c).map(drop),
        k::ASSERTION => k::Assertion::from_control(c).map(drop),
        k::PRE_READ => k::PreReadRequest::from_control(c)
            .map(drop)
            .or_else(|_| k::PreReadResponse::from_control(c).map(drop)),
        k::POST_READ => k::PostReadRequest::from_control(c)
            .map(drop)
            .or_else(|_| k::PostReadResponse::from_control(c).map(drop)),
        k::PASSWORD_POLICY => k::PasswordPolicyResponse::from_control(c).map(drop),
        _ => Ok(()),
    };
}

/// Filters: the first byte picks the string form (even) or BER (odd).
pub fn filter(data: &[u8]) {
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    if sel & 1 == 0 {
        let s = String::from_utf8_lossy(rest);
        if let Ok(f) = Filter::parse(&s) {
            let printed = f.to_string();
            let again = Filter::parse(&printed).expect("a printed filter parses");
            assert_eq!(again, f, "filter string round trip: {printed}");
            assert_eq!(Filter::from_ber(&f.to_ber()).expect("BER"), f);
        }
    } else if let Ok(f) = Filter::from_ber(rest) {
        assert_eq!(
            Filter::from_ber(&f.to_ber()).expect("BER"),
            f,
            "filter BER round trip"
        );
        let _ = Filter::parse(&f.to_string());
    }
}

/// DNs: parse, print, parse again.
pub fn dn(data: &[u8]) {
    let s = String::from_utf8_lossy(data);
    if let Ok(d) = Dn::parse(&s) {
        let printed = d.to_string();
        let again = Dn::parse(&printed).expect("a printed DN parses");
        assert_eq!(again, d, "DN round trip: {printed}");
        let _ = d.parent();
    }
}

fn no_lines(mut l: ldif::Ldif) -> ldif::Ldif {
    for r in &mut l.records {
        r.line = 0;
    }
    l
}

/// LDIF: parse, write, parse again.
pub fn ldif(data: &[u8]) {
    if let Ok(l) = ldif::parse(data) {
        let written = ldif::write(&l);
        let again = ldif::parse(written.as_bytes()).expect("written LDIF parses");
        assert_eq!(no_lines(again), no_lines(l), "LDIF round trip");
    }
}

/// Schema descriptions: the first byte picks the kind.
pub fn schema(data: &[u8]) {
    const KINDS: [&str; 9] = [
        "attributeTypes",
        "objectClasses",
        "ldapSyntaxes",
        "matchingRules",
        "matchingRuleUse",
        "dITContentRules",
        "dITStructureRules",
        "nameForms",
        "IBMAttributeTypes",
    ];
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    let kind = KINDS[usize::from(sel) % KINDS.len()];
    let s = String::from_utf8_lossy(rest);
    if let Ok(v) = SchemaValue::parse(kind, &s) {
        let printed = v.to_string();
        let again = SchemaValue::parse(kind, &printed).expect("a printed description parses");
        assert_eq!(again, v, "schema round trip: {printed}");
    }
}

/// LDAP URLs: parse, print, parse again.
pub fn url(data: &[u8]) {
    let s = String::from_utf8_lossy(data);
    if let Ok(u) = LdapUrl::parse(&s) {
        let printed = u.to_string();
        let again = LdapUrl::parse(&printed).expect("a printed URL parses");
        assert_eq!(again, u, "URL round trip: {printed}");
        let _ = u.filter();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn seeds_run() {
        super::ber(&[0x30, 0x03, 0x02, 0x01, 0x01]);
        super::msg(&[0x30, 0x05, 0x02, 0x01, 0x03, 0x42, 0x00]);
        super::filter(b"\x00(&(cn=a*b)(!(sn:dn:2.5.13.2:=x)))");
        super::filter(&[1, 0xA3, 0x07, 0x04, 0x02, b'c', b'n', 0x04, 0x01, b'a']);
        super::dn(b"cn=a\\2C b+sn=#04024869, dc=x");
        super::ldif(
            b"version: 1\ndn: cn=a\ncn:: YQ==\n\ndn: cn=b\nchangetype: modify\nadd: cn\ncn: x\n-\n",
        );
        super::schema(b"\x00( 2.5.4.3 NAME ( 'cn' 'commonName' ) SUP name )");
        super::schema(
            b"\x08( 2.5.4.3 DBNAME( 'cn' 'cn' ) ACCESS-CLASS normal LENGTH 256 EQUALITY )",
        );
        super::url(b"ldap://h:389/dc=x?cn,sn?sub?(cn=a)?!e-bindname=cn=a%2Cdc=x");
        // Cases found by reading the printers: each must round-trip.
        super::url(b"ldap://h/???%21x=1");
        super::ldif(b"dn: a\nchangetype: modify\nadd:: YQpi\n-\n");
        super::ldif(b"charset:: YQpi\ndn: a\n");
    }

    /// A few thousand byte mutations of each seed, from a fixed seed: a
    /// quick check of the round trips, not a fuzzer.
    #[test]
    fn mutations_round_trip() {
        type Entry = (fn(&[u8]), &'static [u8]);
        const TOKENS: &[&[u8]] = &[
            b"\\",
            b"*",
            b"(",
            b")",
            b"#",
            b",",
            b"+",
            b"=",
            b"\n",
            b" ",
            b"%",
            b"'",
            b"$",
            b":",
            b"!",
            b"\\00",
            b"\xc3\xa9",
            b"\xff",
        ];
        let seeds: [Entry; 7] = [
            (super::msg, &[0x30, 0x0C, 0x02, 0x01, 0x01, 0x60, 0x07, 0x02, 0x01, 0x03, 0x04, 0x00, 0x80, 0x00]),
            (super::filter, b"\x00(&(cn=a*b*c)(|(sn>=x)(!(o~=y)))(cn:dn:1.2:=v)(x=*))"),
            (super::dn, b"cn=a\\2C b+sn=#04024869, OS400-SYS=SYSTEM;dc=\"q,r\""),
            (super::ldif, b"version: 1\ndn: cn=a\ncontrol: 1.2 true:: AA==\nchangetype: modrdn\nnewrdn: cn=b\ndeleteoldrdn: 1\nnewsuperior: dc=y\n\ndn: cn=c\ncn: x\n y\njpegPhoto:< file:///a\n"),
            (super::schema, b"\x00( 2.5.4.3 NAME ( 'cn' 'commonName' ) DESC 'a\\27b' SUP name USAGE dSAOperation X-O ( 'a' 'b' ) )"),
            (super::url, b"ldap://[::1]:389/dc=x?cn,sn?one?(cn=a%20b)?!e-x=cn=a%2Cdc=x,y"),
            (super::ber, &[0x30, 0x08, 0x31, 0x03, 0x01, 0x01, 0xFF, 0x04, 0x01, 0x41]),
        ];
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut rnd = move |n: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            usize::try_from(state % (n.max(1) as u64)).unwrap_or(0)
        };
        for (run, seed) in seeds {
            for _ in 0..3000 {
                let mut d = seed.to_vec();
                for _ in 0..=rnd(4) {
                    match rnd(4) {
                        0 if !d.is_empty() => {
                            let i = rnd(d.len());
                            d.remove(i);
                        }
                        1 => {
                            let i = rnd(d.len() + 1);
                            let t = TOKENS[rnd(TOKENS.len())];
                            d.splice(i..i, t.iter().copied());
                        }
                        2 if !d.is_empty() => {
                            let i = rnd(d.len());
                            d[i] = u8::try_from(rnd(256)).unwrap_or(0);
                        }
                        _ if d.len() > 1 => {
                            let i = rnd(d.len());
                            let j = rnd(d.len());
                            d.swap(i, j);
                        }
                        _ => {}
                    }
                }
                run(&d);
            }
        }
    }
}
