// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Controls: making them (`ldap_create_*_control`), reading the ones a
//! server returned (`ldap_parse_*_control`), and the list functions
//! (copy, insert, add, remove, merge, free). No client control is
//! built: a critical one fails an operation with `LDAP_NOT_SUPPORTED`.

use ldap::ber::{Reader, Writer, tag};
use ldap::control::{
    Assertion, Control, ControlValue, EntryChange, PagedResults, PasswordPolicyResponse,
    PersistentSearch, PolicyWarning, ProxiedAuthz, SortKey, SortRequest, SortResponse,
};
use mi::{FatPtr, Job};

use super::{free_block, give_berval, give_ptr_array, give_str, key, ret, with};
use crate::consts::*;
use crate::mem::{self, Codec};
use crate::state::{Ber, State};

/// The client controls of a call (the handle's when null): an error if
/// one is critical, since none is understood.
pub fn client_controls(j: &Job, codec: Codec, cctrls: u128, dft: &[Control]) -> Result<(), i32> {
    let list = if mem::is_null(cctrls) {
        dft.to_vec()
    } else {
        mem::read_controls(j, cctrls, codec)
    };
    if list.iter().any(|c| c.critical) {
        return Err(LDAP_NOT_SUPPORTED);
    }
    Ok(())
}

/// The server controls of a call (none: the handle's), after checking
/// its client controls.
pub fn op_controls(
    s: &mut State,
    j: &Job,
    k: u64,
    sctrls: u128,
    cctrls: u128,
) -> Result<Option<Vec<Control>>, i32> {
    let codec = s.codec_of(Some(k));
    let dft = super::h(s, k).opts.cctrls.clone();
    if let Err(e) = client_controls(j, codec, cctrls, &dft) {
        return Err(super::h(s, k).set_error(
            e,
            None,
            Some("a critical client control is not supported".into()),
        ));
    }
    Ok((!mem::is_null(sctrls)).then(|| mem::read_controls(j, sctrls, codec)))
}

/// A control placed for the program; its pointer.
pub fn give_control(s: &mut State, j: &mut Job, codec: Codec, c: &Control) -> FatPtr {
    s.heap.place(j, &mem::control_image(c, codec))
}

fn out(s: &mut State, j: &mut Job, codec: Codec, c: &Control, outp: u128) -> i64 {
    let p = give_control(s, j, codec, c);
    if mem::put_ptr(j, outp, p) {
        ret(LDAP_SUCCESS)
    } else {
        free_block(s, j, mem::raw(p));
        ret(LDAP_PARAM_ERROR)
    }
}

fn codec(s: &State, j: &Job, ld: u128) -> Codec {
    s.codec_of(key(s, j, ld))
}

fn crit(v: i64) -> bool {
    v & 0xFF != 0
}

/// `int ldap_create_control(const char *oid, BerElement *ber, int
/// iscritical, LDAPControl **ctrlp)`: the value is what `ber` holds.
pub fn create_control(j: &mut Job, oid: u128, ber: u128, iscritical: i64, outp: u128) -> i64 {
    with(j, |s, j| {
        let c = s.codec();
        let Some(oid) = c.arg(j, oid) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let value = match mem::native(j, ber) {
            None => None,
            Some(n) => match s.bers.get(&n) {
                Some(Ber::Write(w)) => Some(w.flatten()),
                Some(Ber::Read(r)) => Some(r.data.clone()),
                _ => return ret(LDAP_PARAM_ERROR),
            },
        };
        out(
            s,
            j,
            c,
            &Control {
                oid,
                critical: crit(iscritical),
                value,
            },
            outp,
        )
    })
}

/// `int ldap_create_page_control(LDAP *ld, unsigned long pagesize,
/// struct berval *cookie, const char iscritical, LDAPControl **output)`.
pub fn create_page_control(
    j: &mut Job,
    ld: u128,
    size: i64,
    cookie: u128,
    iscritical: i64,
    outp: u128,
) -> i64 {
    with(j, |s, j| {
        let c = codec(s, j, ld);
        let p = PagedResults {
            size: i32::try_from(mem::uint(size)).unwrap_or(i32::MAX),
            cookie: mem::berval(j, cookie).unwrap_or_default(),
        };
        out(s, j, c, &p.to_control(crit(iscritical)), outp)
    })
}

/// `int ldap_parse_page_control(LDAP *ld, LDAPControl **ctrls,
/// unsigned long *totalcount, struct berval **cookie)`.
pub fn parse_page_control(j: &mut Job, ld: u128, ctrls: u128, total: u128, cookie: u128) -> i64 {
    with(j, |s, j| {
        let list = mem::read_controls(j, ctrls, codec(s, j, ld));
        match PagedResults::find(&list) {
            Ok(Some(p)) => {
                mem::put_u32(j, total, u32::try_from(p.size.max(0)).unwrap_or(0));
                if !mem::is_null(cookie) {
                    let b = give_berval(s, j, &p.cookie);
                    mem::put_ptr(j, cookie, b);
                }
                ret(LDAP_SUCCESS)
            }
            Ok(None) => ret(LDAP_CONTROL_NOT_FOUND),
            Err(_) => ret(LDAP_DECODING_ERROR),
        }
    })
}

/// The keys of a sort key string: blank-separated `[-]attr[:rule]`.
#[must_use]
pub fn sort_keys(text: &str) -> Vec<SortKey> {
    text.split_whitespace()
        .map(|w| {
            let (reverse, w) = match w.strip_prefix('-') {
                Some(r) => (true, r),
                None => (false, w),
            };
            let (attr, rule) = match w.split_once(':') {
                Some((a, r)) => (a, Some(r.to_string())),
                None => (w, None),
            };
            SortKey {
                attr: attr.to_string(),
                rule,
                reverse,
            }
        })
        .filter(|k| !k.attr.is_empty())
        .collect()
}

/// `int ldap_create_sort_keylist(LDAPSortKey ***sortKeyList, const
/// char *keyString)`.
pub fn create_sort_keylist(j: &mut Job, outp: u128, keys: u128) -> i64 {
    with(j, |s, j| {
        let c = s.codec();
        let Some(text) = c.arg(j, keys) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let keys = sort_keys(&text);
        if keys.is_empty() {
            return ret(LDAP_PARAM_ERROR);
        }
        let ptrs: Vec<FatPtr> = keys
            .iter()
            .map(|k| {
                let mut im = mem::Image::new(mem::SORTKEY);
                let a = im.cstr(&c.bytes(&k.attr));
                im.ptr(0, a);
                if let Some(r) = &k.rule {
                    let o = im.cstr(&c.bytes(r));
                    im.ptr(16, o);
                }
                im.i32(32, i32::from(k.reverse));
                s.heap.place(j, &im)
            })
            .collect();
        let arr = give_ptr_array(s, j, &ptrs);
        if mem::put_ptr(j, outp, arr) {
            ret(LDAP_SUCCESS)
        } else {
            ret(LDAP_PARAM_ERROR)
        }
    })
}

/// `void ldap_free_sort_keylist(LDAPSortKey **sortKeyList)`.
pub fn free_sort_keylist(j: &mut Job, list: u128) -> i64 {
    with(j, |s, j| {
        super::free_bervals(s, j, list);
        0
    })
}

/// A program's `LDAPSortKey **`.
fn read_keys(j: &Job, c: Codec, list: u128) -> Vec<SortKey> {
    mem::ptr_array(j, list)
        .into_iter()
        .filter_map(|p| {
            let b = mem::bytes(j, p, mem::SORTKEY)?;
            let a = u128::from_le_bytes(b[0..16].try_into().ok()?);
            let r = u128::from_le_bytes(b[16..32].try_into().ok()?);
            let rev = i32::from_be_bytes([b[32], b[33], b[34], b[35]]) != 0;
            Some(SortKey {
                attr: c.arg(j, a)?,
                rule: c.arg(j, r),
                reverse: rev,
            })
        })
        .collect()
}

/// `int ldap_create_sort_control(LDAP *ld, LDAPSortKey **keyList,
/// const char iscritical, LDAPControl **output)`.
pub fn create_sort_control(j: &mut Job, ld: u128, list: u128, iscritical: i64, outp: u128) -> i64 {
    with(j, |s, j| {
        let c = codec(s, j, ld);
        let keys = read_keys(j, c, list);
        if keys.is_empty() {
            return ret(LDAP_PARAM_ERROR);
        }
        out(
            s,
            j,
            c,
            &SortRequest(keys).to_control(crit(iscritical)),
            outp,
        )
    })
}

/// `int ldap_parse_sort_control(LDAP *ld, LDAPControl **ctrls,
/// unsigned long *returnCode, char **attribute)`.
pub fn parse_sort_control(j: &mut Job, ld: u128, ctrls: u128, rc: u128, attr: u128) -> i64 {
    with(j, |s, j| {
        let c = codec(s, j, ld);
        let list = mem::read_controls(j, ctrls, c);
        match SortResponse::find(&list) {
            Ok(Some(r)) => {
                mem::put_u32(j, rc, r.result.number());
                if !mem::is_null(attr) {
                    let p = r.attr.map_or(FatPtr::null(), |a| give_str(s, j, c, &a));
                    mem::put_ptr(j, attr, p);
                }
                ret(LDAP_SUCCESS)
            }
            Ok(None) => ret(LDAP_CONTROL_NOT_FOUND),
            Err(_) => ret(LDAP_DECODING_ERROR),
        }
    })
}

/// `int ldap_create_proxyauth_control(LDAP *ld, char *proxyDN, char
/// iscritical, LDAPControl **controlp)`: proxied authorization v2 for
/// `dn:` and the DN (an authorization ID with `dn:` or `u:` is kept).
pub fn create_proxyauth_control(
    j: &mut Job,
    ld: u128,
    dn: u128,
    iscritical: i64,
    outp: u128,
) -> i64 {
    with(j, |s, j| {
        let c = codec(s, j, ld);
        let dn = c.arg(j, dn).unwrap_or_default();
        let id = if dn.is_empty() || dn.starts_with("dn:") || dn.starts_with("u:") {
            dn
        } else {
            format!("dn:{dn}")
        };
        out(
            s,
            j,
            c,
            &ProxiedAuthz(id).to_control(crit(iscritical)),
            outp,
        )
    })
}

/// `int ldap_create_persistentsearch_control(LDAP *ld, int
/// changeTypes, int changesOnly, int returnECs, char iscritical,
/// LDAPControl **ctrlp)`.
pub fn create_persistentsearch_control(
    j: &mut Job,
    ld: u128,
    types: i64,
    only: i64,
    ecs: i64,
    iscritical: i64,
    outp: u128,
) -> i64 {
    with(j, |s, j| {
        let c = codec(s, j, ld);
        let t = mem::int(types);
        if t & !LDAP_CHANGETYPE_ANY != 0 || t == 0 {
            return ret(LDAP_PARAM_ERROR);
        }
        let p = PersistentSearch {
            change_types: t,
            changes_only: mem::int(only) != 0,
            return_ecs: mem::int(ecs) != 0,
        };
        out(s, j, c, &p.to_control(crit(iscritical)), outp)
    })
}

/// `int ldap_parse_entrychange_control(LDAP *ld, LDAPControl **ctrls,
/// int *changeType, char **prevDN, int *hasChangeNumber, long
/// *changeNumber)`.
pub fn parse_entrychange_control(
    j: &mut Job,
    ld: u128,
    ctrls: u128,
    ty: u128,
    prev: u128,
    has: u128,
    num: u128,
) -> i64 {
    with(j, |s, j| {
        let c = codec(s, j, ld);
        let list = mem::read_controls(j, ctrls, c);
        match EntryChange::find(&list) {
            Ok(Some(e)) => {
                mem::put_i32(j, ty, e.change_type);
                if !mem::is_null(prev) {
                    let p = e
                        .previous_dn
                        .map_or(FatPtr::null(), |d| give_str(s, j, c, &d));
                    mem::put_ptr(j, prev, p);
                }
                mem::put_i32(j, has, i32::from(e.change_number.is_some()));
                mem::put_i32(
                    j,
                    num,
                    e.change_number
                        .map_or(0, |n| i32::try_from(n).unwrap_or(i32::MAX)),
                );
                ret(LDAP_SUCCESS)
            }
            Ok(None) => ret(LDAP_CONTROL_NOT_FOUND),
            Err(_) => ret(LDAP_DECODING_ERROR),
        }
    })
}

/// `int ldap_create_assertion_control(LDAP *ld, char *filter, char
/// iscritical, LDAPControl **ctrlp)` (RFC 4528).
pub fn create_assertion_control(
    j: &mut Job,
    ld: u128,
    filter: u128,
    iscritical: i64,
    outp: u128,
) -> i64 {
    with(j, |s, j| {
        let c = codec(s, j, ld);
        let Some(f) = c.arg(j, filter) else {
            return ret(LDAP_PARAM_ERROR);
        };
        match ldap::Filter::parse(&f) {
            Ok(f) => out(s, j, c, &Assertion(f).to_control(crit(iscritical)), outp),
            Err(_) => ret(LDAP_FILTER_ERROR),
        }
    })
}

/// `int ldap_create_vlv_control(LDAP *ld, LDAPVLVInfo *vlvinfop,
/// LDAPControl **ctrlp)`: the virtual list view request
/// (draft-ietf-ldapext-ldapv3-vlv), always critical as the draft asks.
pub fn create_vlv_control(j: &mut Job, ld: u128, info: u128, outp: u128) -> i64 {
    with(j, |s, j| {
        let c = codec(s, j, ld);
        let Some(b) = mem::bytes(j, info, 80) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let u = |o: usize| i64::from(u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]));
        let (before, after, offset, count) = (u(4), u(8), u(12), u(16));
        let attrv = u128::from_le_bytes(b[32..48].try_into().expect("16"));
        let ctx = u128::from_le_bytes(b[48..64].try_into().expect("16"));
        let assertion = mem::berval(j, attrv);
        let context = mem::berval(j, ctx);
        let mut w = Writer::new();
        w.constructed(tag::SEQUENCE, |w| {
            w.integer(tag::INTEGER, before);
            w.integer(tag::INTEGER, after);
            match &assertion {
                Some(v) => w.octets(tag::context(1), v),
                None => w.constructed(tag::context_constructed(0), |w| {
                    w.integer(tag::INTEGER, offset);
                    w.integer(tag::INTEGER, count);
                }),
            }
            if let Some(cx) = &context {
                w.octets(tag::OCTET_STRING, cx);
            }
        });
        let ctl = Control {
            oid: VLV_REQUEST.to_string(),
            critical: true,
            value: Some(w.into_bytes()),
        };
        out(s, j, c, &ctl, outp)
    })
}

/// `int ldap_parse_vlv_control(LDAP *ld, LDAPControl **ctrls, unsigned
/// long *target_posp, unsigned long *list_countp, struct berval
/// **contextp, int *errcodep)`.
#[allow(clippy::too_many_arguments)]
pub fn parse_vlv_control(
    j: &mut Job,
    ld: u128,
    ctrls: u128,
    pos: u128,
    count: u128,
    ctx: u128,
    err: u128,
) -> i64 {
    with(j, |s, j| {
        let list = mem::read_controls(j, ctrls, codec(s, j, ld));
        let Some(cv) = list.iter().find(|c| c.oid == VLV_RESPONSE) else {
            return ret(LDAP_CONTROL_NOT_FOUND);
        };
        let Some(v) = &cv.value else {
            return ret(LDAP_DECODING_ERROR);
        };
        let parsed = (|| {
            let mut r = Reader::new(v);
            let mut q = r.sequence(tag::SEQUENCE).ok()?;
            let p = q.integer(tag::INTEGER).ok()?;
            let n = q.integer(tag::INTEGER).ok()?;
            let e = q.enumerated(tag::ENUMERATED).ok()?;
            let cx = q.opt_octets(tag::OCTET_STRING).ok()?.map(<[u8]>::to_vec);
            Some((p, n, e, cx))
        })();
        let Some((p, n, e, cx)) = parsed else {
            return ret(LDAP_DECODING_ERROR);
        };
        mem::put_u32(j, pos, u32::try_from(p).unwrap_or(0));
        mem::put_u32(j, count, u32::try_from(n).unwrap_or(0));
        mem::put_i32(j, err, i32::try_from(e).unwrap_or(LDAP_OTHER));
        if !mem::is_null(ctx) {
            let b = cx.map_or(FatPtr::null(), |c| give_berval(s, j, &c));
            mem::put_ptr(j, ctx, b);
        }
        ret(LDAP_SUCCESS)
    })
}

/// `int ldap_parse_pwdpolicy_response(LDAPControl **ctrls, int
/// *warning, int *value, int *error)`: the password policy response
/// control's warning (`LDAP_PP_*`, its value) and error
/// (`LDAP_PP_NO_ERROR` for none).
pub fn parse_pwdpolicy_response(
    j: &mut Job,
    ctrls: u128,
    warn: u128,
    value: u128,
    error: u128,
) -> i64 {
    with(j, |s, j| {
        let list = mem::read_controls(j, ctrls, s.codec());
        match PasswordPolicyResponse::find(&list) {
            Ok(Some(p)) => {
                let (w, v) = match p.warning {
                    None => (LDAP_PP_WARNING_NONE, 0),
                    Some(PolicyWarning::TimeBeforeExpiration(n)) => {
                        (LDAP_PP_TIME_BEFORE_EXPIRATION, n)
                    }
                    Some(PolicyWarning::GraceAuthNsRemaining(n)) => {
                        (LDAP_PP_GRACE_LOGINS_REMAINING, n)
                    }
                };
                mem::put_i32(j, warn, w);
                mem::put_i32(j, value, v);
                let e = p.error.map_or(LDAP_PP_NO_ERROR, |e| {
                    i32::try_from(e.number()).unwrap_or(LDAP_PP_NO_ERROR)
                });
                mem::put_i32(j, error, e);
                ret(LDAP_SUCCESS)
            }
            Ok(None) => ret(LDAP_CONTROL_NOT_FOUND),
            Err(_) => ret(LDAP_DECODING_ERROR),
        }
    })
}

/// `void ldap_control_free(LDAPControl *ctrl)`.
pub fn control_free(j: &mut Job, c: u128) -> i64 {
    with(j, |s, j| {
        free_block(s, j, c);
        0
    })
}

/// `void ldap_controls_free(LDAPControl **ctrls)`.
pub fn controls_free(j: &mut Job, c: u128) -> i64 {
    with(j, |s, j| {
        super::free_controls(s, j, c);
        0
    })
}

fn put_list(s: &mut State, j: &mut Job, listp: u128, old: u128, ptrs: &[FatPtr]) -> i64 {
    let arr = give_ptr_array(s, j, ptrs);
    if !mem::put_ptr(j, listp, arr) {
        free_block(s, j, mem::raw(arr));
        return ret(LDAP_PARAM_ERROR);
    }
    free_block(s, j, old);
    ret(LDAP_SUCCESS)
}

/// The pointers of an `LDAPControl **`, as fat pointers.
fn list_of(j: &Job, p: u128) -> Vec<FatPtr> {
    mem::ptr_array(j, p).into_iter().map(mem::fat).collect()
}

/// `int ldap_copy_controls(LDAPControl ***to, LDAPControl **from)`.
pub fn copy_controls(j: &mut Job, to: u128, from: u128) -> i64 {
    with(j, |s, j| {
        let c = s.codec();
        let list = mem::read_controls(j, from, c);
        let p = if mem::is_null(from) {
            FatPtr::null()
        } else {
            super::give_controls(s, j, c, &list)
        };
        if mem::put_ptr(j, to, p) {
            ret(LDAP_SUCCESS)
        } else {
            ret(LDAP_PARAM_ERROR)
        }
    })
}

/// `int ldap_insert_control(LDAPControl *newControl, LDAPControl
/// ***ctrlList)`: the control is added at the end of the list (a new
/// list when `*ctrlList` is null).
pub fn insert_control(j: &mut Job, newc: u128, listp: u128) -> i64 {
    with(j, |s, j| {
        if mem::is_null(newc) {
            return ret(LDAP_PARAM_ERROR);
        }
        let Some(old) = mem::get_ptr(j, listp) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let mut ptrs = list_of(j, old);
        ptrs.push(mem::fat(newc));
        put_list(s, j, listp, old, &ptrs)
    })
}

/// `int ldap_add_control(const char *oid, unsigned long len, char
/// *value, int iscritical, LDAPControl ***ctrlList)`: a new control
/// added to the list.
pub fn add_control(
    j: &mut Job,
    oid: u128,
    len: i64,
    value: u128,
    iscritical: i64,
    listp: u128,
) -> i64 {
    with(j, |s, j| {
        let c = s.codec();
        let Some(oid) = c.arg(j, oid) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let n = mem::uint(len) as usize;
        let value = if mem::is_null(value) {
            None
        } else {
            match mem::bytes(j, value, n) {
                Some(b) => Some(b.to_vec()),
                None => return ret(LDAP_PARAM_ERROR),
            }
        };
        let Some(old) = mem::get_ptr(j, listp) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let mut ptrs = list_of(j, old);
        ptrs.push(give_control(
            s,
            j,
            c,
            &Control {
                oid,
                critical: iscritical != 0,
                value,
            },
        ));
        put_list(s, j, listp, old, &ptrs)
    })
}

/// `int ldap_remove_control(LDAPControl *delControl, LDAPControl
/// ***ctrlList, int freeit)`: `LDAP_CONTROL_NOT_FOUND` when it is not in
/// the list.
pub fn remove_control(j: &mut Job, del: u128, listp: u128, freeit: i64) -> i64 {
    with(j, |s, j| {
        let Some(old) = mem::get_ptr(j, listp) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let Some(target) = mem::native(j, del) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let ptrs = list_of(j, old);
        let keep: Vec<FatPtr> = ptrs
            .iter()
            .copied()
            .filter(|p| mem::native(j, mem::raw(*p)) != Some(target))
            .collect();
        if keep.len() == ptrs.len() {
            return ret(LDAP_CONTROL_NOT_FOUND);
        }
        if freeit != 0 {
            free_block(s, j, del);
        }
        put_list(s, j, listp, old, &keep)
    })
}

/// `int ldap_merge_control_lists(LDAPControl **list1, LDAPControl
/// **list2, LDAPControl ***merged)`: a new list with copies of both.
pub fn merge_control_lists(j: &mut Job, a: u128, b: u128, outp: u128) -> i64 {
    with(j, |s, j| {
        let c = s.codec();
        let mut list = mem::read_controls(j, a, c);
        list.extend(mem::read_controls(j, b, c));
        let p = super::give_controls(s, j, c, &list);
        if mem::put_ptr(j, outp, p) {
            ret(LDAP_SUCCESS)
        } else {
            ret(LDAP_PARAM_ERROR)
        }
    })
}

/// `int ldap_get_bind_controls(LDAP *ld, LDAPControl ***ctrls)`: the
/// controls of the last bind response.
pub fn get_bind_controls(j: &mut Job, ld: u128, outp: u128) -> i64 {
    super::on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let list = super::h(s, k).bind_controls.clone();
        let c = s.codec_of(Some(k));
        let p = if list.is_empty() {
            FatPtr::null()
        } else {
            super::give_controls(s, j, c, &list)
        };
        if mem::put_ptr(j, outp, p) {
            ret(LDAP_SUCCESS)
        } else {
            ret(LDAP_PARAM_ERROR)
        }
    })
}
