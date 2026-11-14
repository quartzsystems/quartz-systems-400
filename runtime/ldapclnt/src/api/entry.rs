// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Walking a chain: messages, entries and references; an entry's DN,
//! attributes (with a `BerElement` cursor), values and controls; and the
//! frees (`ldap_value_free*`, `ldap_memfree`, `ldap_ber_free`,
//! `ldap_mods_free`).

use ldap::msg::{Attribute, LdapMessage, ProtocolOp};
use mi::Job;

use super::{
    free_bervals, free_block, give_bervals, give_controls, give_str, give_strings, key, null, ptr,
    ret, with,
};
use crate::consts::*;
use crate::mem;
use crate::state::{Ber, State};

/// Which messages a walk visits.
#[derive(Clone, Copy)]
enum Kind {
    Any,
    Entry,
    Reference,
}

fn is(kind: Kind, m: &LdapMessage) -> bool {
    match kind {
        Kind::Any => true,
        Kind::Entry => matches!(m.op, ProtocolOp::SearchResultEntry { .. }),
        Kind::Reference => matches!(m.op, ProtocolOp::SearchResultReference(_)),
    }
}

fn first(j: &mut Job, p: u128, kind: Kind, after: bool) -> u128 {
    with(j, |s, j| {
        let Some((base, i)) = s.message(j, p) else {
            return null();
        };
        let start = if after { i + 1 } else { i };
        let found = s.chains[&base].msgs[start.min(s.chains[&base].msgs.len())..]
            .iter()
            .position(|m| is(kind, m));
        match found {
            Some(n) => ptr(s.message_ptr(j, base, start + n)),
            None => null(),
        }
    })
}

fn count(j: &mut Job, p: u128, kind: Kind) -> i64 {
    with(j, |s, j| match s.message(j, p) {
        Some((base, i)) => i64::try_from(
            s.chains[&base].msgs[i..]
                .iter()
                .filter(|m| is(kind, m))
                .count(),
        )
        .unwrap_or(i64::MAX),
        None => -1,
    })
}

/// `LDAPMessage *ldap_first_message(LDAP *ld, LDAPMessage *res)`.
pub fn first_message(j: &mut Job, _ld: u128, res: u128) -> u128 {
    first(j, res, Kind::Any, false)
}

/// `LDAPMessage *ldap_next_message(LDAP *ld, LDAPMessage *msg)`.
pub fn next_message(j: &mut Job, _ld: u128, m: u128) -> u128 {
    first(j, m, Kind::Any, true)
}

/// `int ldap_count_messages(LDAP *ld, LDAPMessage *res)`.
pub fn count_messages(j: &mut Job, _ld: u128, res: u128) -> i64 {
    count(j, res, Kind::Any)
}

/// `LDAPMessage *ldap_first_entry(LDAP *ld, LDAPMessage *res)`.
pub fn first_entry(j: &mut Job, _ld: u128, res: u128) -> u128 {
    first(j, res, Kind::Entry, false)
}

/// `LDAPMessage *ldap_next_entry(LDAP *ld, LDAPMessage *entry)`.
pub fn next_entry(j: &mut Job, _ld: u128, e: u128) -> u128 {
    first(j, e, Kind::Entry, true)
}

/// `int ldap_count_entries(LDAP *ld, LDAPMessage *res)`.
pub fn count_entries(j: &mut Job, _ld: u128, res: u128) -> i64 {
    count(j, res, Kind::Entry)
}

/// `LDAPMessage *ldap_first_reference(LDAP *ld, LDAPMessage *res)`.
pub fn first_reference(j: &mut Job, _ld: u128, res: u128) -> u128 {
    first(j, res, Kind::Reference, false)
}

/// `LDAPMessage *ldap_next_reference(LDAP *ld, LDAPMessage *ref)`.
pub fn next_reference(j: &mut Job, _ld: u128, r: u128) -> u128 {
    first(j, r, Kind::Reference, true)
}

/// `int ldap_count_references(LDAP *ld, LDAPMessage *res)`.
pub fn count_references(j: &mut Job, _ld: u128, res: u128) -> i64 {
    count(j, res, Kind::Reference)
}

/// The entry a pointer reaches: DN, attributes and controls.
fn entry(
    s: &State,
    j: &Job,
    e: u128,
) -> Option<(String, Vec<Attribute>, Vec<ldap::control::Control>)> {
    let (_, _, m) = super::result::message(s, j, e)?;
    match m.op {
        ProtocolOp::SearchResultEntry { dn, attrs } => Some((dn, attrs, m.controls)),
        _ => None,
    }
}

fn set_err(s: &mut State, j: &Job, ld: u128, code: i32) {
    match key(s, j, ld) {
        Some(k) => {
            super::h(s, k).set_error(code, None, None);
        }
        None => s.errno = code,
    }
}

/// `char *ldap_get_dn(LDAP *ld, LDAPMessage *entry)`.
pub fn get_dn(j: &mut Job, ld: u128, e: u128) -> u128 {
    with(j, |s, j| match entry(s, j, e) {
        Some((dn, _, _)) => {
            let c = s.codec_of(key(s, j, ld));
            ptr(give_str(s, j, c, &dn))
        }
        None => {
            set_err(s, j, ld, LDAP_PARAM_ERROR);
            null()
        }
    })
}

/// `char *ldap_first_attribute(LDAP *ld, LDAPMessage *entry, BerElement
/// **ber)`: the first attribute's name; `*ber` the cursor
/// `ldap_next_attribute` goes on with, freed with `ldap_ber_free`.
pub fn first_attribute(j: &mut Job, ld: u128, e: u128, berp: u128) -> u128 {
    with(j, |s, j| {
        let Some((_, attrs, _)) = entry(s, j, e) else {
            set_err(s, j, ld, LDAP_PARAM_ERROR);
            return null();
        };
        let b = s.new_ber(j, Ber::Attrs(1));
        mem::put_ptr(j, berp, b);
        match attrs.first() {
            Some(a) => {
                let c = s.codec_of(key(s, j, ld));
                ptr(give_str(s, j, c, &a.ty))
            }
            None => null(),
        }
    })
}

/// `char *ldap_next_attribute(LDAP *ld, LDAPMessage *entry, BerElement
/// *ber)`: null after the last.
pub fn next_attribute(j: &mut Job, ld: u128, e: u128, ber: u128) -> u128 {
    with(j, |s, j| {
        let Some((_, attrs, _)) = entry(s, j, e) else {
            set_err(s, j, ld, LDAP_PARAM_ERROR);
            return null();
        };
        let Some(n) = mem::native(j, ber) else {
            set_err(s, j, ld, LDAP_PARAM_ERROR);
            return null();
        };
        let Some(Ber::Attrs(i)) = s.bers.get_mut(&n) else {
            set_err(s, j, ld, LDAP_PARAM_ERROR);
            return null();
        };
        let at = *i;
        *i += 1;
        match attrs.get(at) {
            Some(a) => {
                let c = s.codec_of(key(s, j, ld));
                ptr(give_str(s, j, c, &a.ty))
            }
            None => null(),
        }
    })
}

/// `int ldap_count_attributes(LDAP *ld, LDAPMessage *entry)`.
pub fn count_attributes(j: &mut Job, _ld: u128, e: u128) -> i64 {
    with(j, |s, j| {
        entry(s, j, e).map_or(-1, |(_, a, _)| i64::try_from(a.len()).unwrap_or(i64::MAX))
    })
}

fn values(s: &State, j: &Job, e: u128, attr: &str) -> Option<Vec<Vec<u8>>> {
    let (_, attrs, _) = entry(s, j, e)?;
    attrs
        .into_iter()
        .find(|a| a.ty.eq_ignore_ascii_case(attr))
        .map(|a| a.vals)
}

/// `char **ldap_get_values(LDAP *ld, LDAPMessage *entry, const char
/// *attr)`: the values as strings in the handle's codec; null when the
/// entry has no such attribute.
pub fn get_values(j: &mut Job, ld: u128, e: u128, attr: u128) -> u128 {
    with(j, |s, j| {
        let k = key(s, j, ld);
        let c = s.codec_of(k);
        let Some(a) = c.arg(j, attr) else {
            set_err(s, j, ld, LDAP_PARAM_ERROR);
            return null();
        };
        match values(s, j, e, &a) {
            Some(v) => {
                let t: Vec<String> = v
                    .iter()
                    .map(|b| String::from_utf8_lossy(b).into_owned())
                    .collect();
                ptr(give_strings(s, j, c, &t))
            }
            None => {
                set_err(s, j, ld, LDAP_NO_SUCH_ATTRIBUTE);
                null()
            }
        }
    })
}

/// `struct berval **ldap_get_values_len(LDAP *ld, LDAPMessage *entry,
/// const char *attr)`: the values as they came.
pub fn get_values_len(j: &mut Job, ld: u128, e: u128, attr: u128) -> u128 {
    with(j, |s, j| {
        let c = s.codec_of(key(s, j, ld));
        let Some(a) = c.arg(j, attr) else {
            set_err(s, j, ld, LDAP_PARAM_ERROR);
            return null();
        };
        match values(s, j, e, &a) {
            Some(v) => ptr(give_bervals(s, j, &v)),
            None => {
                set_err(s, j, ld, LDAP_NO_SUCH_ATTRIBUTE);
                null()
            }
        }
    })
}

/// `int ldap_count_values(char **vals)` and `ldap_count_values_len`.
pub fn count_values(j: &mut Job, vals: u128) -> i64 {
    i64::try_from(mem::ptr_array(j, vals).len()).unwrap_or(i64::MAX)
}

/// `void ldap_value_free(char **vals)`.
pub fn value_free(j: &mut Job, vals: u128) -> i64 {
    with(j, |s, j| {
        free_block(s, j, vals);
        0
    })
}

/// `void ldap_value_free_len(struct berval **vals)`.
pub fn value_free_len(j: &mut Job, vals: u128) -> i64 {
    with(j, |s, j| {
        free_bervals(s, j, vals);
        0
    })
}

/// `int ldap_get_entry_controls(LDAP *ld, LDAPMessage *entry,
/// LDAPControl ***serverctrlsp)` and its `_np` form.
pub fn get_entry_controls(j: &mut Job, ld: u128, e: u128, out: u128) -> i64 {
    with(j, |s, j| {
        let Some((_, _, m)) = super::result::message(s, j, e) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let c = s.codec_of(key(s, j, ld));
        let p = if m.controls.is_empty() {
            mi::FatPtr::null()
        } else {
            give_controls(s, j, c, &m.controls)
        };
        if mem::put_ptr(j, out, p) {
            ret(LDAP_SUCCESS)
        } else {
            ret(LDAP_PARAM_ERROR)
        }
    })
}

/// `void ldap_memfree(char *mem)`.
pub fn memfree(j: &mut Job, p: u128) -> i64 {
    with(j, |s, j| {
        free_block(s, j, p);
        0
    })
}

/// `void ldap_ber_free(BerElement *ber, int freebuf)` (and `ber_free`).
pub fn ber_free(j: &mut Job, ber: u128, _freebuf: i64) -> i64 {
    with(j, |s, j| {
        if let Some(n) = mem::native(j, ber) {
            s.free_ber(j, n);
        }
        0
    })
}

/// `void ldap_mods_free(LDAPMod **mods, int freemods)`: frees what the
/// library gave out among them; the program's own storage is its own.
pub fn mods_free(j: &mut Job, mods: u128, freemods: i64) -> i64 {
    with(j, |s, j| {
        for m in mem::ptr_array(j, mods) {
            if let Some(b) = mem::bytes(j, m, mem::MOD) {
                let ty = u128::from_le_bytes(b[16..32].try_into().expect("16"));
                let vals = u128::from_le_bytes(b[32..48].try_into().expect("16"));
                free_block(s, j, ty);
                free_bervals(s, j, vals);
            }
            if freemods != 0 {
                free_block(s, j, m);
            }
        }
        if freemods != 0 {
            free_block(s, j, mods);
        }
        0
    })
}
