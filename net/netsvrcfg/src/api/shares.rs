// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QZLSADFS`, `QZLSCHFS`, `QZLSADPS`, `QZLSCHPS` and `QZLSRMS`. A
//! share change takes effect at once, as on IBM i.
//!
//! File share parameters: 0 share name (12), 1 path, 2 path length,
//! 3 path CCSID, 4 text (50), 5 permission, 6 maximum users, then the
//! optional 7 text CCSID, 8 text conversion (1), 9 extensions (50
//! each), 10 extension count, 11 encryption required (1), 12
//! authorization list (10). Print share parameters: 0 share name, 1
//! output queue (20), 2 text, 3 spooled file type, 4 printer driver
//! (50), then the optional 5 printer file (20), 6 publish (1), 7
//! encryption required, 8 authorization list.

use super::error::{ApiError, authority, msg, share as reason};
use super::host::Host;
use super::parms::Parms;
use crate::fmt::text::{bin4, text_at};
use crate::model::config::Config;
use crate::model::share::{FileShare, Kind, Permission, PrintShare, Qualified, Share, SpoolType};
use crate::model::valid::{self, PathError};

fn share_err(api: &str, share: &str, r: i32) -> ApiError {
    ApiError::of(msg::SHARE, api).text(share, 12).bin4(r)
}

/// The share name parameter, checked.
fn share_name(api: &str, p: &Parms<'_>) -> Result<String, ApiError> {
    let raw = p.text(0);
    valid::share_name(&raw).map_err(|_| ApiError::of(msg::CHARACTER, api).bin4(1).text(&raw, 15))
}

/// A `0`/`1` flag parameter; `None` when not passed.
fn flag(p: &Parms<'_>, i: usize) -> Result<Option<bool>, ()> {
    match p.ch(i) {
        None => Ok(None),
        Some('0') => Ok(Some(false)),
        Some('1') => Ok(Some(true)),
        Some(_) => Err(()),
    }
}

/// `*IOSYSCFG`, or ownership of what is shared (`owner` `None`: it
/// does not exist).
fn may_share(api: &str, h: &mut dyn Host, owner: Option<String>) -> Result<(), ApiError> {
    let c = h.caller();
    if c.iosyscfg {
        return Ok(());
    }
    match owner {
        None => Err(ApiError::of(msg::AUTHORITY, api).bin4(authority::MISSING)),
        Some(o) if o != c.user => Err(ApiError::of(msg::AUTHORITY, api).bin4(authority::NOT_OWNER)),
        Some(_) => Ok(()),
    }
}

/// The path parameter in its CCSID, as UTF-8.
fn path(api: &str, p: &Parms<'_>) -> Result<String, ApiError> {
    let len = p.bin4(2);
    let ccsid = p.bin4(3);
    let max = if matches!(ccsid, 1200 | 13488) {
        2048
    } else {
        1024
    };
    let n = usize::try_from(len).unwrap_or(0);
    if len <= 0 || n > max {
        return Err(ApiError::of(msg::LENGTH, api)
            .bin4(3)
            .bin4(len)
            .bin4(0)
            .bin4(i32::try_from(max + 1).unwrap_or(i32::MAX)));
    }
    let raw = p.raw(1, n);
    let conv_err = |from: i32| {
        ApiError::of(msg::CONVERSION, api)
            .bin4(1208)
            .bin4(from)
            .bin4(crate::api::error::NOT_SUPPORTED_RC)
            .bin4(2)
            .bin4(0)
    };
    let s = match ccsid {
        0 | 37 => ccsid::from_ebcdic(&raw),
        c => {
            let c16 = u16::try_from(c).map_err(|_| conv_err(c))?;
            ccsid::conv::to_string(c16, &raw).map_err(|_| conv_err(c))?
        }
    };
    valid::path(&s).map_err(|e| match e {
        PathError::NotAbsolute => share_err(api, &p.text(0), reason::PATH),
        PathError::Length(l) => ApiError::of(msg::LENGTH, api)
            .bin4(3)
            .bin4(i32::try_from(l).unwrap_or(i32::MAX))
            .bin4(0)
            .bin4(1025),
    })
}

fn autl(api: &str, h: &mut dyn Host, p: &Parms<'_>, i: usize) -> Result<Option<String>, ApiError> {
    if !p.has(i) {
        return Ok(None);
    }
    let raw = p.text(i);
    if raw.is_empty() || raw == "*NONE" {
        return Ok(Some(String::new()));
    }
    let name = valid::object_name(&raw).map_err(|_| {
        ApiError::of(msg::CHARACTER, api)
            .bin4(i32::try_from(i + 1).unwrap_or(0))
            .text(&raw, 15)
    })?;
    if !h.autl_exists(&name) {
        return Err(ApiError::new(msg::NO_AUTL).text(&name, 10));
    }
    Ok(Some(name))
}

fn text_ccsid_ok(c: i32) -> bool {
    c == 0 || u16::try_from(c).is_ok_and(|c| ccsid::conv::supported().contains(&c))
}

/// The extension table: `count` entries of a length and 46 bytes.
fn extensions(api: &str, p: &Parms<'_>) -> Result<Option<Vec<String>>, ApiError> {
    if !p.has(10) {
        return Ok(None);
    }
    let n = p.bin4(10);
    let count = usize::try_from(n)
        .ok()
        .filter(|c| *c <= valid::EXTENSIONS_MAX);
    let Some(count) = count else {
        return Err(ApiError::of(msg::LENGTH, api)
            .bin4(11)
            .bin4(n)
            .bin4(-1)
            .bin4(i32::try_from(valid::EXTENSIONS_MAX + 1).unwrap_or(0)));
    };
    let table = p.get(9).unwrap_or(&[]);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let at = 50 * i;
        let len = bin4(table, at);
        let l = usize::try_from(len)
            .ok()
            .filter(|l| (1..=valid::EXTENSION_MAX).contains(l));
        let Some(l) = l else {
            return Err(ApiError::of(msg::LENGTH, api)
                .bin4(10)
                .bin4(len)
                .bin4(0)
                .bin4(i32::try_from(valid::EXTENSION_MAX + 1).unwrap_or(0)));
        };
        let e = text_at(table, at + 4, l);
        let e = valid::extension(&e)
            .map_err(|_| ApiError::of(msg::CHARACTER, api).bin4(10).text(&e, 15))?;
        if !out.contains(&e) {
            out.push(e);
        }
    }
    Ok(Some(out))
}

/// The file share a call describes, from `base` (the existing share or
/// a new one).
fn file_share(
    api: &str,
    h: &mut dyn Host,
    p: &Parms<'_>,
    name: &str,
    base: &Share,
) -> Result<Share, ApiError> {
    let path = path(api, p)?;
    let perm =
        Permission::from_code(p.bin4(5)).ok_or_else(|| share_err(api, name, reason::PERMISSION))?;
    let max = p.bin4(6);
    if max != -1 && max < 1 {
        return Err(share_err(api, name, reason::MAX_USERS));
    }
    let mut f = base.as_file().cloned().unwrap_or(FileShare {
        path: String::new(),
        permission: perm,
        max_users: -1,
        text_ccsid: 0,
        text_conversion: false,
        extensions: Vec::new(),
    });
    f.path = path;
    f.permission = perm;
    f.max_users = max;
    if p.has(7) {
        let c = p.bin4(7);
        if !text_ccsid_ok(c) {
            return Err(share_err(api, name, reason::TEXT_CONVERSION));
        }
        f.text_ccsid = c;
    }
    match flag(p, 8) {
        Ok(Some(v)) => f.text_conversion = v,
        Ok(None) => {}
        Err(()) => return Err(share_err(api, name, reason::TEXT_CONVERSION)),
    }
    if let Some(e) = extensions(api, p)? {
        f.extensions = e;
    }
    let mut s = base.clone();
    s.name = name.into();
    s.text = p.text(4);
    s.kind = Kind::File(f);
    match flag(p, 11) {
        Ok(Some(v)) => s.encryption_required = v,
        Ok(None) => {}
        Err(()) => {
            return Err(ApiError::of(msg::CHARACTER, api)
                .bin4(12)
                .raw(&p.raw(11, 15)));
        }
    }
    if let Some(a) = autl(api, h, p, 12)? {
        s.autl = a;
    }
    Ok(s)
}

fn qualified(p: &Parms<'_>, i: usize) -> Qualified {
    let b = p.raw(i, 20);
    Qualified::new(&text_at(&b, 0, 10), &text_at(&b, 10, 10))
}

/// The print share a call describes, from `base`; whether publishing
/// was asked for (QS/400 has no directory to publish to).
fn print_share(
    api: &str,
    h: &mut dyn Host,
    p: &Parms<'_>,
    name: &str,
    base: &Share,
) -> Result<(Share, bool), ApiError> {
    let outq = qualified(p, 1);
    let bad_name = |i: i32, v: &str| ApiError::of(msg::CHARACTER, api).bin4(i).text(v, 15);
    let outq = Qualified::new(
        &valid::object_name(&outq.name).map_err(|_| bad_name(2, &outq.name))?,
        &valid::object_name(&outq.library).map_err(|_| bad_name(2, &outq.library))?,
    );
    let st =
        SpoolType::from_code(p.bin4(3)).ok_or_else(|| share_err(api, name, reason::SPOOL_TYPE))?;
    let mut ps = base.as_print().cloned().unwrap_or(PrintShare {
        outq: outq.clone(),
        driver: String::new(),
        spool_type: st,
        printer_file: Qualified::default(),
        publish: false,
    });
    ps.outq = outq;
    ps.spool_type = st;
    ps.driver = p.text(4);
    if p.has(5) {
        let q = qualified(p, 5);
        ps.printer_file = if q.name.is_empty() || q.name == "*NONE" {
            Qualified::default()
        } else {
            Qualified::new(
                &valid::object_name(&q.name).map_err(|_| bad_name(6, &q.name))?,
                &valid::object_name(&q.library).map_err(|_| bad_name(6, &q.library))?,
            )
        };
    }
    let publish = match flag(p, 6) {
        Ok(v) => v.unwrap_or(false),
        Err(()) => return Err(share_err(api, name, reason::PUBLISH)),
    };
    ps.publish = false;
    let mut s = base.clone();
    s.name = name.into();
    s.text = p.text(2);
    s.kind = Kind::Print(ps);
    match flag(p, 7) {
        Ok(Some(v)) => s.encryption_required = v,
        Ok(None) => {}
        Err(()) => return Err(ApiError::of(msg::CHARACTER, api).bin4(8).raw(&p.raw(7, 15))),
    }
    if let Some(a) = autl(api, h, p, 8)? {
        s.autl = a;
    }
    Ok((s, publish))
}

fn save(h: &mut dyn Host, c: &Config) -> Result<(), ApiError> {
    h.save(c)
}

/// Publishing was asked for: the share is in place, unpublished, and
/// the call reports `CPFB682` reason 6 as IBM does when it cannot
/// publish.
fn not_published(api: &str, name: &str) -> ApiError {
    ApiError::of(msg::FAILED, api)
        .bin4(6)
        .bin4(0)
        .text(name, 12)
}

/// `QZLSADFS`.
pub fn add_file(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSADFS";
    let name = share_name(API, p)?;
    let mut c = h.load()?;
    if c.shares.contains_key(&name) {
        return Err(share_err(API, &name, reason::DUPLICATE));
    }
    let s = file_share(API, h, p, &name, &Share::file(&name, "/", ""))?;
    let owner = s.as_file().and_then(|f| h.path_owner(&f.path));
    may_share(API, h, owner)?;
    c.shares.insert(name, s);
    save(h, &c)?;
    Ok(Vec::new())
}

/// `QZLSCHFS`.
pub fn change_file(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSCHFS";
    let name = share_name(API, p)?;
    let mut c = h.load()?;
    let old = c
        .shares
        .get(&name)
        .cloned()
        .ok_or_else(|| share_err(API, &name, reason::MISSING))?;
    if old.as_file().is_none() {
        return Err(share_err(API, &name, reason::WRONG_TYPE));
    }
    let s = file_share(API, h, p, &name, &old)?;
    for path in [old.as_file(), s.as_file()]
        .into_iter()
        .flatten()
        .map(|f| f.path.clone())
    {
        let owner = h.path_owner(&path);
        may_share(API, h, owner)?;
    }
    c.shares.insert(name, s);
    save(h, &c)?;
    Ok(Vec::new())
}

/// `QZLSADPS`.
pub fn add_print(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSADPS";
    let name = share_name(API, p)?;
    let mut c = h.load()?;
    if c.shares.contains_key(&name) {
        return Err(share_err(API, &name, reason::DUPLICATE));
    }
    let base = Share::print(&name, Qualified::default(), "");
    let (s, publish) = print_share(API, h, p, &name, &base)?;
    let owner = s.as_print().and_then(|ps| h.outq_owner(&ps.outq));
    may_share(API, h, owner)?;
    c.shares.insert(name.clone(), s);
    save(h, &c)?;
    if publish {
        return Err(not_published(API, &name));
    }
    Ok(Vec::new())
}

/// `QZLSCHPS`.
pub fn change_print(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSCHPS";
    let name = share_name(API, p)?;
    let mut c = h.load()?;
    let old = c
        .shares
        .get(&name)
        .cloned()
        .ok_or_else(|| share_err(API, &name, reason::MISSING))?;
    if old.as_print().is_none() {
        return Err(share_err(API, &name, reason::WRONG_TYPE));
    }
    let (s, publish) = print_share(API, h, p, &name, &old)?;
    for q in [old.as_print(), s.as_print()]
        .into_iter()
        .flatten()
        .map(|ps| ps.outq.clone())
    {
        let owner = h.outq_owner(&q);
        may_share(API, h, owner)?;
    }
    c.shares.insert(name.clone(), s);
    save(h, &c)?;
    if publish {
        return Err(not_published(API, &name));
    }
    Ok(Vec::new())
}

/// `QZLSRMS(share name)`.
pub fn remove(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSRMS";
    let name = share_name(API, p)?;
    let mut c = h.load()?;
    let old = c
        .shares
        .get(&name)
        .cloned()
        .ok_or_else(|| share_err(API, &name, reason::MISSING))?;
    let owner = match &old.kind {
        Kind::File(f) => h.path_owner(&f.path),
        Kind::Print(ps) => h.outq_owner(&ps.outq),
    };
    may_share(API, h, owner)?;
    c.shares.remove(&name);
    save(h, &c)?;
    Ok(Vec::new())
}
