// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Qp0lGetAttr` and `Qp0lSetAttr` (`QlgGetAttr`, `QlgSetAttr`): the
//! attribute buffer is a chain of entries, each a `Qp0l_Attr_Header_t`
//! (the next entry's offset from the buffer's start, 0 on the last; the
//! ID; the data's size) and the data, the next header on a four-byte
//! boundary. [`DATA`] is each attribute's data size, as `docs/IFS.md`
//! "APIs" lists it.

use ifs::Errno;
use ifs::meta::{CrtObjAud, ObjAudit, Scan, StgOpt};
use ifs::ops::{Attr, AttrId, AuthList, Role};
use mi::Job;
use sec::auth::bits::Auth;

use super::consts::QP0L_FOLLOW_SYMLNK;
use super::layout::{self as lay, ATTR_HEADER, AUTH_GENERAL, AUTH_USERS, CHECKOUT, JOURNAL_INFO};
use super::path::qlg_path;
use super::{R, client};
use super::file::bound_r9;
use crate::net::abi::{get_int, is_null, mem, put_int, ret, uint};
use crate::param::fixed;

/// Data bytes of each attribute (0: variable, `QP0L_ATTR_AUTH`).
pub const DATA: &[(&str, u32, usize)] = &[
    ("QP0L_ATTR_OBJTYPE", 0, 11),
    ("QP0L_ATTR_DATA_SIZE", 1, 4),
    ("QP0L_ATTR_ALLOC_SIZE", 2, 4),
    ("QP0L_ATTR_EXTENDED_ATTR_SIZE", 3, 4),
    ("QP0L_ATTR_CREATE_TIME", 4, 4),
    ("QP0L_ATTR_ACCESS_TIME", 5, 4),
    ("QP0L_ATTR_CHANGE_TIME", 6, 4),
    ("QP0L_ATTR_MODIFY_TIME", 7, 4),
    ("QP0L_ATTR_STG_FREE", 8, 1),
    ("QP0L_ATTR_CHECKED_OUT", 9, 16),
    ("QP0L_ATTR_LOCAL_REMOTE", 10, 1),
    ("QP0L_ATTR_AUTH", 11, 0),
    ("QP0L_ATTR_FILE_ID", 12, 16),
    ("QP0L_ATTR_ASP", 13, 2),
    ("QP0L_ATTR_DATA_SIZE_64", 14, 8),
    ("QP0L_ATTR_ALLOC_SIZE_64", 15, 8),
    ("QP0L_ATTR_PC_READ_ONLY", 17, 1),
    ("QP0L_ATTR_PC_HIDDEN", 18, 1),
    ("QP0L_ATTR_PC_SYSTEM", 19, 1),
    ("QP0L_ATTR_PC_ARCHIVE", 20, 1),
    ("QP0L_ATTR_SYSTEM_ARCHIVE", 21, 1),
    ("QP0L_ATTR_CODEPAGE", 22, 2),
    ("QP0L_ATTR_FILE_FORMAT", 23, 1),
    ("QP0L_ATTR_JOURNAL_INFORMATION", 25, 36),
    ("QP0L_ATTR_ALWCKPWRT", 26, 1),
    ("QP0L_ATTR_CCSID", 27, 2),
    ("QP0L_ATTR_SIGNED", 28, 1),
    ("QP0L_ATTR_SYS_SIGNED", 29, 1),
    ("QP0L_ATTR_MULT_SIGS", 30, 1),
    ("QP0L_ATTR_DISK_STG_OPT", 31, 1),
    ("QP0L_ATTR_MAIN_STG_OPT", 32, 1),
    ("QP0L_ATTR_DIR_FORMAT", 33, 1),
    ("QP0L_ATTR_AUDIT", 34, 10),
    ("QP0L_ATTR_CRTOBJSCAN", 35, 1),
    ("QP0L_ATTR_SCAN", 36, 1),
    ("QP0L_ATTR_ALWSAV", 38, 1),
    ("QP0L_ATTR_RSTDRNMUNL", 39, 1),
    ("QP0L_ATTR_CRTOBJAUD", 41, 10),
    ("QP0L_ATTR_SYSTEM_USE", 42, 1),
    ("QP0L_ATTR_TEMPORARY", 43, 1),
    ("QP0L_ATTR_INHERIT_ALWCKPWRT", 46, 1),
    ("QP0L_ATTR_SYS_RESTRICTS_SAVE", 47, 1),
    ("QP0L_ATTR_TEXT", 48, 54),
    ("QP0L_ATTR_SUID", 300, 1),
    ("QP0L_ATTR_SGID", 301, 1),
];

const DATA_SIZE_64: u32 = 14;
const ALLOC_SIZE_64: u32 = 15;

fn flag(b: bool) -> Vec<u8> {
    vec![u8::from(b)]
}

fn u4(v: u64) -> R<Vec<u8>> {
    u32::try_from(v).map(|x| x.to_be_bytes().to_vec()).map_err(|_| Errno::EOVERFLOW)
}

fn time(t: i64) -> Vec<u8> {
    let s = u32::try_from(lay::secs(t)).unwrap_or(u32::MAX);
    s.to_be_bytes().to_vec()
}

fn audit_name(a: ObjAudit) -> Vec<u8> {
    fixed(a.name(), 10)
}

fn crtobjaud_name(c: CrtObjAud) -> Vec<u8> {
    match c {
        CrtObjAud::SysVal => fixed("*SYSVAL", 10),
        CrtObjAud::Value(a) => audit_name(a),
    }
}

/// The bit of an authority by its name.
fn bit(name: &str) -> u32 {
    sec::auth::bits::NAMES.iter().find(|(n, _)| *n == name).map_or(0, |(_, b)| *b)
}

/// `QP0L_ATTR_AUTH`'s data: the general part, then one user entry per
/// row of `DSPAUT` (`*PUBLIC` first).
fn auth_data(list: &AuthList) -> Vec<u8> {
    let g = &AUTH_GENERAL;
    let u = &AUTH_USERS;
    let mut b = vec![0u8; g.size];
    lay::put(&mut b, g, "Object_owner", &fixed(&list.owner, 10));
    lay::put(&mut b, g, "Primary_group", &fixed(list.group.as_deref().unwrap_or("*NONE"), 10));
    lay::put(&mut b, g, "Auth_list_name", &fixed(list.autl.as_deref().unwrap_or("*NONE"), 10));
    lay::put_n(&mut b, g, "UserArray_Offset", g.size as u64);
    lay::put_n(&mut b, g, "Number_of_users", list.entries.len() as u64);
    lay::put_n(&mut b, g, "UserEntry_size", u.size as u64);
    for e in &list.entries {
        let mut r = vec![0u8; u.size];
        let who = if e.role == Role::Public { "*PUBLIC" } else { e.who.as_str() };
        lay::put(&mut r, u, "User_name", &fixed(who, 10));
        lay::put(&mut r, u, "User_data_auth", &fixed(&e.data, 10));
        let has = |a: Auth, n: &str| u64::from(a.0 & bit(n) != 0);
        for (field, n) in [
            ("OBJMGT_rights", "*OBJMGT"),
            ("OBJEXIST_rights", "*OBJEXIST"),
            ("OBJALTER_rights", "*OBJALTER"),
            ("OBJREF_rights", "*OBJREF"),
            ("OBJOPR_DataRights", "*OBJOPR"),
            ("READ_DataRights", "*READ"),
            ("ADD_DataRights", "*ADD"),
            ("UPDATE_DataRights", "*UPD"),
            ("DELETE_DataRights", "*DLT"),
            ("EXECUTE_DataRights", "*EXECUTE"),
        ] {
            lay::put_n(&mut r, u, field, has(e.auth, n));
        }
        lay::put_n(&mut r, u, "EXCLUDE_DataRights", u64::from(e.auth.0 == 0));
        b.extend_from_slice(&r);
    }
    b
}

/// One attribute's ID and data.
fn data_of(a: &Attr, auth: &dyn Fn() -> R<AuthList>) -> R<(u32, Vec<u8>)> {
    let id = a.id() as u32;
    let d = match a {
        Attr::ObjType(t) => {
            let mut v = fixed(t, 10);
            v.push(0);
            v
        }
        Attr::DataSize(n) | Attr::AllocSize(n) | Attr::ExtendedAttrSize(n) => u4(*n)?,
        Attr::CreateTime(t) | Attr::AccessTime(t) | Attr::ChangeTime(t) | Attr::ModifyTime(t) => time(*t),
        Attr::StgFree(b)
        | Attr::LocalRemote(b)
        | Attr::PcReadOnly(b)
        | Attr::PcHidden(b)
        | Attr::PcSystem(b)
        | Attr::PcArchive(b)
        | Attr::SystemArchive(b)
        | Attr::Alwckpwrt(b)
        | Attr::Signed(b)
        | Attr::SysSigned(b)
        | Attr::MultSigs(b)
        | Attr::Alwsav(b)
        | Attr::Rstdrnmunl(b)
        | Attr::Temporary(b)
        | Attr::InheritAlwckpwrt(b)
        | Attr::SysRestrictsSave(b)
        | Attr::Suid(b)
        | Attr::Sgid(b) => flag(*b),
        Attr::CheckedOut(c) => {
            let l = &CHECKOUT;
            let mut v = vec![0u8; l.size];
            if let Some(c) = c {
                lay::put_n(&mut v, l, "Checked_Out_Flag", 1);
                lay::put(&mut v, l, "User_Name", &fixed(&c.user, 10));
                lay::put_n(&mut v, l, "Checked_Out_Time", lay::secs(c.time));
            } else {
                lay::put(&mut v, l, "User_Name", &fixed("", 10));
            }
            v
        }
        Attr::Auth { .. } => auth_data(&auth()?),
        Attr::FileId(f) => f.to_bytes().to_vec(),
        Attr::Asp(n) => n.to_be_bytes().to_vec(),
        Attr::Codepage(n) | Attr::Ccsid(n) => n.to_be_bytes().to_vec(),
        Attr::FileFormat(n) | Attr::DirFormat(n) | Attr::SystemUse(n) => vec![*n],
        Attr::JournalInformation(j) => {
            let l = &JOURNAL_INFO;
            let mut v = vec![0u8; l.size];
            if let Some(j) = j {
                lay::put_n(&mut v, l, "Status", 1);
                let mut opts = 0x20u64;
                if j.images == 1 {
                    opts |= 0x40;
                }
                if j.inherit {
                    opts |= 0x80;
                }
                if !j.omit_open_close {
                    opts |= 0x08;
                }
                lay::put_n(&mut v, l, "Options", opts);
                lay::put(&mut v, l, "Journal_Identifier", &j.jid);
                let (lib, name) = j.journal.split_once('/').unwrap_or(("", j.journal.as_str()));
                lay::put(&mut v, l, "Journal_Name", &fixed(name, 10));
                lay::put(&mut v, l, "Journal_Library", &fixed(lib, 10));
                lay::put_n(&mut v, l, "Time_Last_Started", lay::secs(j.start));
            }
            v
        }
        Attr::DiskStgOpt(s) | Attr::MainStgOpt(s) => vec![*s as u8],
        Attr::Audit(a) => audit_name(*a),
        Attr::Crtobjscan(s) | Attr::Scan(s) => vec![*s as u8],
        Attr::Crtobjaud(c) => crtobjaud_name(*c),
        Attr::Text(t) => {
            let mut v = 37u32.to_be_bytes().to_vec();
            v.extend(fixed(t, 50));
            v
        }
    };
    Ok((id, d))
}

/// The attribute buffer of a list of entries.
fn chain(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut b = Vec::new();
    for (i, (id, d)) in entries.iter().enumerate() {
        let start = b.len();
        let mut h = vec![0u8; ATTR_HEADER.size];
        lay::put_n(&mut h, &ATTR_HEADER, "Attr_ID", u64::from(*id));
        lay::put_n(&mut h, &ATTR_HEADER, "Attr_Size", d.len() as u64);
        b.extend_from_slice(&h);
        b.extend_from_slice(d);
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
        if i + 1 < entries.len() {
            let next = b.len() as u64;
            lay::put_n(&mut b[start..start + 16], &ATTR_HEADER, "Next_Attr_Offset", next);
        }
    }
    b
}

/// The attributes asked for (every one for none), as the buffer they
/// make: in the order asked, else by ID.
pub(crate) fn get_buffer(j: &mut Job, path: &str, ids: &[u32], follow: bool) -> R<Vec<u8>> {
    let c = client(j);
    let mut ask = Vec::new();
    for id in ids {
        let base = match *id {
            DATA_SIZE_64 => 1,
            ALLOC_SIZE_64 => 2,
            n => n,
        };
        let a = AttrId::from_u32(base).ok_or(Errno::EINVAL)?;
        if !ask.contains(&a) {
            ask.push(a);
        }
    }
    let attrs = c.get_attrs(path, &ask, follow)?;
    let find = |id: u32| attrs.iter().find(|a| a.id() as u32 == id);
    let order: Vec<u32> = if ids.is_empty() {
        let mut v: Vec<u32> = attrs.iter().map(|a| a.id() as u32).collect();
        if v.contains(&1) {
            v.push(DATA_SIZE_64);
        }
        if v.contains(&2) {
            v.push(ALLOC_SIZE_64);
        }
        v.sort_unstable();
        v
    } else {
        ids.to_vec()
    };
    let list = || c.dspaut(path);
    let mut entries: Vec<(u32, Vec<u8>)> = Vec::new();
    for id in order {
        match (id, find(id), find(1), find(2)) {
            (DATA_SIZE_64, _, Some(Attr::DataSize(n)), _) | (ALLOC_SIZE_64, _, _, Some(Attr::AllocSize(n))) => {
                entries.push((id, n.to_be_bytes().to_vec()));
            }
            (_, Some(a), _, _) => match data_of(a, &list) {
                Ok(e) => entries.push(e),
                // Every attribute at once leaves out what a 32-bit field
                // cannot hold; the 64-bit forms carry it.
                Err(Errno::EOVERFLOW) if ids.is_empty() => {}
                Err(e) => return Err(e),
            },
            _ => {}
        }
    }
    Ok(chain(&entries))
}

#[allow(clippy::too_many_arguments)]
fn get_attr(j: &mut Job, p: u128, list: u128, buf: u128, size: i64, needed: u128, returned: u128, follow: i64) -> i64 {
    let r = (|| {
        let path = qlg_path(j, p)?;
        let ids: Vec<u32> = if is_null(list) {
            Vec::new()
        } else {
            let n = usize::try_from(get_int(j, list)?).map_err(|_| Errno::EINVAL)?;
            let b = mem(j, list, 4 + 4 * n)?;
            (0..n)
                .map(|i| u32::from_be_bytes(b[4 + 4 * i..8 + 4 * i].try_into().expect("4 bytes")))
                .collect()
        };
        let out = get_buffer(j, &path, &ids, uint(follow) == QP0L_FOLLOW_SYMLNK)?;
        let room = usize::try_from(uint(size)).map_err(|_| Errno::EINVAL)?;
        let total = i32::try_from(out.len()).map_err(|_| Errno::EOVERFLOW)?;
        put_int(j, needed, total)?;
        if out.len() > room {
            put_int(j, returned, 0)?;
            return Ok(0);
        }
        mem(j, buf, out.len())?.copy_from_slice(&out);
        put_int(j, returned, total)?;
        Ok(0)
    })();
    ret(j, r)
}

/// The attribute an entry of a `Qp0lSetAttr` buffer sets.
fn attr_of(id: u32, d: &[u8]) -> R<Attr> {
    let b1 = || d.first().copied().ok_or(Errno::EINVAL);
    let yes = || b1().map(|v| v != 0);
    let u2 = || {
        d.get(..2)
            .map(|x| u16::from_be_bytes([x[0], x[1]]))
            .ok_or(Errno::EINVAL)
    };
    let text10 = || ccsid::from_ebcdic(d.get(..10).unwrap_or(d)).trim_end().to_string();
    let stg = |v: u8| match v {
        0 => Ok(StgOpt::Normal),
        1 => Ok(StgOpt::Minimize),
        2 => Ok(StgOpt::Dynamic),
        _ => Err(Errno::EINVAL),
    };
    let scan = |v: u8| match v {
        0 => Ok(Scan::No),
        1 => Ok(Scan::Yes),
        2 => Ok(Scan::ChgOnly),
        _ => Err(Errno::EINVAL),
    };
    let aud = |s: &str| ObjAudit::parse(s).ok_or(Errno::EINVAL);
    Ok(match AttrId::from_u32(id).ok_or(Errno::EINVAL)? {
        AttrId::PcReadOnly => Attr::PcReadOnly(yes()?),
        AttrId::PcHidden => Attr::PcHidden(yes()?),
        AttrId::PcSystem => Attr::PcSystem(yes()?),
        AttrId::PcArchive => Attr::PcArchive(yes()?),
        AttrId::SystemArchive => Attr::SystemArchive(yes()?),
        AttrId::Alwckpwrt => Attr::Alwckpwrt(yes()?),
        AttrId::InheritAlwckpwrt => Attr::InheritAlwckpwrt(yes()?),
        AttrId::Alwsav => Attr::Alwsav(yes()?),
        AttrId::Rstdrnmunl => Attr::Rstdrnmunl(yes()?),
        AttrId::Suid => Attr::Suid(yes()?),
        AttrId::Sgid => Attr::Sgid(yes()?),
        AttrId::Ccsid => Attr::Ccsid(u2()?),
        AttrId::Codepage => Attr::Codepage(u2()?),
        AttrId::DiskStgOpt => Attr::DiskStgOpt(stg(b1()?)?),
        AttrId::MainStgOpt => Attr::MainStgOpt(stg(b1()?)?),
        AttrId::Crtobjscan => Attr::Crtobjscan(scan(b1()?)?),
        AttrId::Scan => Attr::Scan(scan(b1()?)?),
        AttrId::Audit => Attr::Audit(aud(&text10())?),
        AttrId::Crtobjaud => {
            let t = text10();
            Attr::Crtobjaud(if t == "*SYSVAL" { CrtObjAud::SysVal } else { CrtObjAud::Value(aud(&t)?) })
        }
        AttrId::Text => {
            let t = d.get(4..).ok_or(Errno::EINVAL)?;
            Attr::Text(ccsid::from_ebcdic(t).trim_end().to_string())
        }
        _ => return Err(Errno::EINVAL),
    })
}

/// The entries of a `Qp0lSetAttr` buffer.
pub(crate) fn parse_buffer(b: &[u8]) -> R<Vec<Attr>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    loop {
        let h = b.get(at..at + ATTR_HEADER.size).ok_or(Errno::EINVAL)?;
        let id = u32::try_from(lay::get_n(h, &ATTR_HEADER, "Attr_ID")).map_err(|_| Errno::EINVAL)?;
        let n = usize::try_from(lay::get_n(h, &ATTR_HEADER, "Attr_Size")).map_err(|_| Errno::EINVAL)?;
        let next = usize::try_from(lay::get_n(h, &ATTR_HEADER, "Next_Attr_Offset")).map_err(|_| Errno::EINVAL)?;
        let d = b.get(at + 16..at + 16 + n).ok_or(Errno::EINVAL)?;
        out.push(attr_of(id, d)?);
        if next == 0 {
            return Ok(out);
        }
        if next <= at {
            return Err(Errno::EINVAL);
        }
        at = next;
    }
}

fn set_attr(j: &mut Job, p: u128, buf: u128, size: i64, follow: i64) -> i64 {
    let r = (|| {
        let path = qlg_path(j, p)?;
        let n = usize::try_from(uint(size)).map_err(|_| Errno::EINVAL)?;
        let b = mem(j, buf, n)?.to_vec();
        let attrs = parse_buffer(&b)?;
        let c = client(j);
        for a in &attrs {
            c.set_attr(&path, a, uint(follow) == QP0L_FOLLOW_SYMLNK)?;
        }
        Ok(0)
    })();
    ret(j, r)
}

bound_r9!(b_get_attr(p: u128; l: u128, b: u128, s: i64, n: u128, r: u128, f: i64) -> i64 = get_attr);
bound_r9!(b_set_attr(p: u128; b: u128, s: i64, f: i64) -> i64 = set_attr);

/// The procedures with their code addresses.
pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("Qp0lGetAttr", at!(b_get_attr)),
        ("QlgGetAttr", at!(b_get_attr)),
        ("Qp0lSetAttr", at!(b_set_attr)),
        ("QlgSetAttr", at!(b_set_attr)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_parses_back() {
        let b = chain(&[(17, vec![1]), (27, 1208u16.to_be_bytes().to_vec())]);
        assert_eq!(lay::get_n(&b, &ATTR_HEADER, "Next_Attr_Offset"), 20);
        assert_eq!(b.len(), 40);
        let attrs = parse_buffer(&b).unwrap();
        assert_eq!(attrs, vec![Attr::PcReadOnly(true), Attr::Ccsid(1208)]);
    }

    #[test]
    fn every_data_size_is_the_codes() {
        for (name, id, _) in DATA {
            assert!(AttrId::from_u32(*id).is_some() || [14, 15].contains(id), "{name}");
        }
    }
}
