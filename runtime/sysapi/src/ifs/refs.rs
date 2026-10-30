// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The program APIs `QP0LROR` (retrieve an object's references, format
//! `RORO0100`) and `QP0LRRO` (retrieve the objects a job references,
//! format `RROO0100`; this job only, `*` in `JIDF0100`). References are
//! what the IFS lock table holds: opens with their access and share
//! mode, byte-range locks and the check-out.

use ifs::lock::share::Share;
use ifs::lock::{Held, LockInfo};
use ifs::ops::{Attr, AttrId};
use mi::Job;

use super::layout::{self as lay, OBJ_LIST, RORO0100, RROO0100, SIM_REF_TYPES};
use super::path::{qlg_bytes, qlg_path_at};
use super::{client, job_ccsid};
use crate::param::{Args, ErrCode, fixed};
use crate::{ApiFail, Provider};

/// The program APIs.
pub const APIS: &[(&str, crate::ApiFn)] = &[("QP0LROR", qp0lror), ("QP0LRRO", qp0lrro)];

/// The simple reference types of an object's references.
fn sim_types(refs: &[LockInfo], checked_out: Option<&str>) -> Vec<u8> {
    let l = &SIM_REF_TYPES;
    let mut b = vec![0u8; l.size];
    let mut add = |name: &str| {
        let v = lay::get_n(&b, l, name) + 1;
        lay::put_n(&mut b, l, name, v);
    };
    for r in refs {
        match r.what {
            Held::Open(a, s) => {
                add(match (a.read, a.write) {
                    (true, true) => "RdWr",
                    (false, true) => "Write",
                    _ => "Read",
                });
                add(match s {
                    Share::ReadOnly => "ShareRead",
                    Share::WriteOnly => "ShareWrite",
                    Share::ReadWrite => "ShareRdWr",
                    Share::None => "ShareNone",
                });
            }
            Held::Range(_) => add("Attributes"),
        }
    }
    match checked_out {
        Some(u) => {
            lay::put_n(&mut b, l, "CheckedOut", 1);
            lay::put(&mut b, l, "CheckOutName", &fixed(u, 10));
        }
        None => lay::put(&mut b, l, "CheckOutName", &fixed("", 10)),
    }
    b
}

/// Deliver `out` into a receiver of `room` bytes at `recv`, with bytes
/// returned and available first.
fn deliver(recv: u64, room: usize, mut out: Vec<u8>) {
    let avail = out.len();
    let n = room.min(avail);
    lay::put_n(&mut out[..8], &RORO0100, "BytesReturned", n as u64);
    lay::put_n(&mut out[..8], &RORO0100, "BytesAvailable", avail as u64);
    if n > 0 && recv != 0 {
        mi::storage::view(recv, n).copy_from_slice(&out[..n]);
    }
}

/// `QP0LROR`: receiver, its length, format, path, error code.
fn qp0lror(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let r: Result<(), ApiFail> = (|| {
        let room = usize::try_from(a.bin4(1)).map_err(|_| ApiFail::id("CPF3C24"))?;
        if room < 8 {
            return Err(ApiFail::id("CPF3C24"));
        }
        let fmt = a.char(2, 8);
        if fmt != "RORO0100" {
            return Err(ApiFail::id("CPF3C21").text(&fmt, 8));
        }
        let path = qlg_path_at(j, a.addr(3)).map_err(|e| ApiFail::id(&e.message_id()))?;
        let c = client(j);
        let refs = c.object_locks(&path).map_err(|e| ApiFail::id(&e.message_id()))?;
        let who = match c.get_attrs(&path, &[AttrId::CheckedOut], true) {
            Ok(v) => v.into_iter().find_map(|x| match x {
                Attr::CheckedOut(Some(co)) => Some(co.user),
                _ => None,
            }),
            Err(_) => None,
        };
        let sim = sim_types(&refs, who.as_deref());
        let l = &RORO0100;
        let mut out = vec![0u8; l.size];
        lay::put_n(&mut out, l, "SimTypesOffset", l.size as u64);
        lay::put_n(&mut out, l, "SimTypesLength", sim.len() as u64);
        let opens = refs.iter().filter(|r| matches!(r.what, Held::Open(..))).count();
        lay::put_n(&mut out, l, "Count", opens as u64);
        lay::put_n(&mut out, l, "InUse", u64::from(!refs.is_empty() || who.is_some()));
        out.extend_from_slice(&sim);
        deliver(a.addr(0), room, out);
        Ok(())
    })();
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QP0LRRO`: receiver, its length, format, job identification, its
/// format, error code.
fn qp0lrro(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let r: Result<(), ApiFail> = (|| {
        let room = usize::try_from(a.bin4(1)).map_err(|_| ApiFail::id("CPF3C24"))?;
        if room < 8 {
            return Err(ApiFail::id("CPF3C24"));
        }
        let fmt = a.char(2, 8);
        if fmt != "RROO0100" {
            return Err(ApiFail::id("CPF3C21").text(&fmt, 8));
        }
        let jfmt = a.char(4, 8);
        if jfmt != "JIDF0100" {
            return Err(ApiFail::id("CPF3C21").text(&jfmt, 8));
        }
        let name = a.char(3, 10);
        if name != "*" {
            // Another job's references are the storage service's to tell.
            // MIGRATE(phase11): other jobs through the service's lock table.
            return Err(ApiFail::id("CPF3C58"));
        }
        let held = client(j).owner_locks().map_err(|e| ApiFail::id(&e.message_id()))?;
        // One entry per object, with every reference this job has on it.
        let mut objs: Vec<(ifs::lock::ObjKey, Vec<LockInfo>)> = Vec::new();
        for (k, li) in held {
            match objs.iter_mut().find(|(x, _)| *x == k) {
                Some((_, v)) => v.push(li),
                None => objs.push((k, vec![li])),
            }
        }
        let ccsid = job_ccsid(j);
        let h = &RROO0100;
        let mut out = vec![0u8; h.size];
        lay::put_n(&mut out, h, "ObjectsReturned", objs.len() as u64);
        lay::put_n(&mut out, h, "ObjectsAvailable", objs.len() as u64);
        lay::put_n(&mut out, h, "ObjListOffset", if objs.is_empty() { 0 } else { h.size as u64 });
        let n = objs.len();
        for (i, (k, refs)) in objs.into_iter().enumerate() {
            let path = refs.first().map(|r| r.path.clone()).unwrap_or_default();
            let sim = sim_types(&refs, None);
            let q = qlg_bytes(ccsid, &path);
            let l = &OBJ_LIST;
            let mut e = vec![0u8; l.size];
            lay::put_n(&mut e, l, "RefDisp", l.size as u64);
            lay::put_n(&mut e, l, "RefLength", sim.len() as u64);
            lay::put_n(&mut e, l, "RefCount", refs.iter().filter(|r| matches!(r.what, Held::Open(..))).count() as u64);
            lay::put_n(&mut e, l, "PathDisp", (l.size + sim.len()) as u64);
            lay::put_n(&mut e, l, "PathLength", q.len() as u64);
            let fid = ifs::ops::audit::FileId {
                fs: k.fs,
                ino: k.ino,
                generation: 0,
            };
            lay::put(&mut e, l, "FileID", &fid.to_bytes());
            lay::put_n(&mut e, l, "FileSysId", u64::from(k.fs));
            lay::put_n(&mut e, l, "FileIdNumber", k.ino & 0xFFFF_FFFF);
            e.extend_from_slice(&sim);
            e.extend_from_slice(&q);
            while !e.len().is_multiple_of(4) {
                e.push(0);
            }
            if i + 1 < n {
                let len = e.len() as u64;
                lay::put_n(&mut e, l, "NextListEntryDisp", len);
            }
            out.extend_from_slice(&e);
        }
        deliver(a.addr(0), room, out);
        Ok(())
    })();
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ifs::lock::Owner;
    use ifs::lock::share::Access;

    #[test]
    fn opens_count_by_access_and_share() {
        let r = |read, write, s| LockInfo {
            path: "/a".into(),
            owner: Owner(1),
            what: Held::Open(Access { read, write }, s),
        };
        let b = sim_types(&[r(true, false, Share::ReadWrite), r(true, true, Share::None)], Some("BOB"));
        let l = &SIM_REF_TYPES;
        assert_eq!(lay::get_n(&b, l, "Read"), 1);
        assert_eq!(lay::get_n(&b, l, "RdWr"), 1);
        assert_eq!(lay::get_n(&b, l, "ShareRdWr"), 1);
        assert_eq!(lay::get_n(&b, l, "ShareNone"), 1);
        assert_eq!(lay::get_n(&b, l, "CheckedOut"), 1);
    }
}
