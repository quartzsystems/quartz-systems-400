// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LDAP URLs (RFC 4516): `ldap_is_ldap_url`, `ldap_url_parse` into an
//! `LDAPURLDesc`, `ldap_free_urldesc`, and their `_utf8` forms.
//!
//! `LDAPURLDesc` as the QS/400 header lays it out: `lud_host` at 0,
//! `lud_port` at 16, `lud_dn` at 32, `lud_attrs` at 48, `lud_scope` at
//! 64, `lud_filter` at 80, `lud_string` (the URL) at 96; 112 bytes.

use ldap::msg::Scope;
use ldap::url::LdapUrl;
use mi::Job;

use super::{free_block, ret, with};
use crate::consts::*;
use crate::mem::{self, Codec, Image};

/// The size of `LDAPURLDesc`.
pub const URLDESC: usize = 112;

const UTF8: Codec = Codec {
    ccsid: 1208,
    utf8: true,
};

fn is_url(j: &mut Job, p: u128, codec: Option<Codec>) -> i64 {
    with(j, |s, j| {
        let c = codec.unwrap_or_else(|| s.codec());
        let ok = c.arg(j, p).is_some_and(|t| {
            let t = t.trim();
            let lower = t.to_ascii_lowercase();
            (lower.starts_with("ldap://")
                || lower.starts_with("ldaps://")
                || lower.starts_with("ldapi://"))
                && LdapUrl::parse(t).is_ok()
        });
        i64::from(ok)
    })
}

/// `int ldap_is_ldap_url(const char *url)`: 1 for an LDAP URL.
pub fn is_ldap_url(j: &mut Job, url: u128) -> i64 {
    is_url(j, url, None)
}

/// `int ldap_is_ldap_url_utf8(const char *url)`.
pub fn is_ldap_url_utf8(j: &mut Job, url: u128) -> i64 {
    is_url(j, url, Some(UTF8))
}

/// The image of a parsed URL.
#[must_use]
pub fn desc_image(u: &LdapUrl, text: &str, c: Codec) -> Image {
    let mut im = Image::new(URLDESC);
    if let Some(h) = &u.host {
        let o = im.cstr(&c.bytes(h));
        im.ptr(0, o);
    }
    im.i32(16, i32::from(u.port_or_default()));
    let o = im.cstr(&c.bytes(&u.dn));
    im.ptr(32, o);
    if !u.attrs.is_empty() {
        let enc: Vec<Vec<u8>> = u.attrs.iter().map(|a| c.bytes(a)).collect();
        let a = im.strings(&enc);
        im.ptr(48, a);
    }
    let scope = match u.scope.unwrap_or(Scope::Base) {
        Scope::Base => LDAP_SCOPE_BASE,
        Scope::One => LDAP_SCOPE_ONELEVEL,
        Scope::Sub => LDAP_SCOPE_SUBTREE,
    };
    im.i32(64, scope);
    let f = u.filter.clone().unwrap_or_else(|| "(objectClass=*)".into());
    let o = im.cstr(&c.bytes(&f));
    im.ptr(80, o);
    let o = im.cstr(&c.bytes(text));
    im.ptr(96, o);
    im
}

fn parse(j: &mut Job, p: u128, outp: u128, codec: Option<Codec>) -> i64 {
    with(j, |s, j| {
        let c = codec.unwrap_or_else(|| s.codec());
        let Some(t) = c.arg(j, p) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let t = t.trim().to_string();
        match LdapUrl::parse(&t) {
            Ok(u) => {
                let d = s.heap.place(j, &desc_image(&u, &t, c));
                if mem::put_ptr(j, outp, d) {
                    ret(LDAP_SUCCESS)
                } else {
                    free_block(s, j, mem::raw(d));
                    ret(LDAP_PARAM_ERROR)
                }
            }
            Err(_) => ret(LDAP_PARAM_ERROR),
        }
    })
}

/// `int ldap_url_parse(const char *url, LDAPURLDesc **ludpp)`.
pub fn url_parse(j: &mut Job, url: u128, outp: u128) -> i64 {
    parse(j, url, outp, None)
}

/// `int ldap_url_parse_utf8(const char *url, LDAPURLDesc **ludpp)`.
pub fn url_parse_utf8(j: &mut Job, url: u128, outp: u128) -> i64 {
    parse(j, url, outp, Some(UTF8))
}

/// `void ldap_free_urldesc(LDAPURLDesc *ludp)`.
pub fn free_urldesc(j: &mut Job, d: u128) -> i64 {
    with(j, |s, j| {
        free_block(s, j, d);
        0
    })
}
