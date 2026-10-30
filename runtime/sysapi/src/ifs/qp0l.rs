// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QP0LLIB2`'s path services: `Qp0lCvtPathToQSYSObjName` (and its
//! `Qlg` name), `Qp0lProcessSubtree` (calling the user's exit procedure
//! or program for each object), `Qp0lGetPathFromFileID` and
//! `QlgGetPathFromFileID`.

use ifs::Errno;
use ifs::ops::WalkOpts;
use ifs::ops::audit::FileId;
use ifs::vfs::qsys::{QsysNode, node_of};
use mi::{FatPtr, Job, PtrKind};

use super::consts::{
    QP0L_BYPASS_NO_ERRORID, QP0L_END_PROCESS_SUBTREE, QP0L_SELECT_DONE, QP0L_SELECT_NOT_OK, QP0L_SELECT_OK,
    QP0L_SUBTREE_NO, QP0L_USER_FUNCTION_PGM, QP0L_USER_FUNCTION_PTR,
};
use super::file::bound_r9;
use super::layout::{self as lay, QSYS_INFO, USER_FUNCTION};
use super::path::{encode, qlg_bytes, qlg_path};
use super::{R, client, job_ccsid};
use crate::net::Stable;
use crate::net::abi::{bound_c, fail, fat, is_null, mem, opt_mem, ptr_at, raw, ret, uint};
use crate::param::{Args, ErrCode};
use crate::provider::ApiFail;

/// The native address behind a pointer argument, 0 for null.
fn native(j: &Job, p: u128) -> u64 {
    if is_null(p) {
        return 0;
    }
    j.spaces.extent(fat(p)).map_or(0, |(at, _)| at)
}

/// The `QSYS0100` answer for an absolute path, names in `ccsid`.
fn qsys_info(path: &str, ccsid: u16) -> Result<Vec<u8>, ApiFail> {
    let not_qsys = || ApiFail::id("CPFA0DB").text(path, path.len().min(256));
    let comps = ifs::path::components(path);
    let Some((first, rest)) = comps.split_first() else {
        return Err(not_qsys());
    };
    if !first.eq_ignore_ascii_case("QSYS.LIB") {
        return Err(not_qsys());
    }
    let node = node_of(rest).map_err(|_| not_qsys())?;
    let l = &QSYS_INFO;
    let mut b = vec![0u8; l.size];
    let field = |b: &mut [u8], name: &str, s: &str| {
        let mut v = encode(ccsid, s);
        let (_, n) = lay::at(l, name);
        v.resize(n, encode(ccsid, " ").first().copied().unwrap_or(0x40));
        lay::put(b, l, name, &v);
    };
    for (n, v) in [
        ("Lib_Name", ""),
        ("Lib_Type", ""),
        ("Obj_Name", ""),
        ("Obj_Type", ""),
        ("Mbr_Name", ""),
        ("Mbr_Type", ""),
        ("Asp_Name", "*SYSBAS"),
    ] {
        field(&mut b, n, v);
    }
    let (lib, obj, mbr) = match &node {
        QsysNode::Root => (Some("QSYS".to_string()), None, None),
        QsysNode::Lib(l) => (Some(l.clone()), None, None),
        QsysNode::Obj(l, o, t) => (Some(l.clone()), Some((o.clone(), t.clone())), None),
        QsysNode::Mbr(l, f, m) => (Some(l.clone()), Some((f.clone(), "*FILE".to_string())), Some(m.clone())),
    };
    if let Some(lname) = lib {
        field(&mut b, "Lib_Name", &lname);
        field(&mut b, "Lib_Type", "*LIB");
    }
    if let Some((o, t)) = obj {
        field(&mut b, "Obj_Name", &o);
        field(&mut b, "Obj_Type", &t);
    }
    if let Some(m) = mbr {
        field(&mut b, "Mbr_Name", &m);
        field(&mut b, "Mbr_Type", "*MBR");
    }
    lay::put_n(&mut b, l, "CCSID_Out", u64::from(ccsid));
    Ok(b)
}

#[allow(clippy::too_many_arguments)]
fn cvt_path(j: &mut Job, p: u128, out: u128, fmt: u128, room: i64, ccsid: i64, err: u128) -> i64 {
    let ec_addr = native(j, err);
    let errc = ErrCode::at(&Args(&[ec_addr]), 0);
    let r: Result<(), ApiFail> = (|| {
        let f = mem(j, fmt, 8).map(|b| ccsid::from_ebcdic(b)).map_err(|_| ApiFail::id("CPF3CF1"))?;
        if f != "QSYS0100" {
            return Err(ApiFail::id("CPF3C21").text(&f, 8));
        }
        let room = usize::try_from(uint(room)).map_err(|_| ApiFail::id("CPF3C24"))?;
        if room < 8 {
            return Err(ApiFail::id("CPF3C24"));
        }
        let path = qlg_path(j, p).map_err(|e| ApiFail::id(&e.message_id()))?;
        let abs = if path.starts_with('/') {
            path
        } else {
            let cwd = client(j).getcwd().map_err(|e| ApiFail::id(&e.message_id()))?;
            ifs::path::join_lexical(&cwd, &path)
        };
        let cc = match u16::try_from(uint(ccsid)) {
            Ok(0) | Err(_) => job_ccsid(j),
            Ok(c) => c,
        };
        let mut b = qsys_info(&abs, cc)?;
        let n = room.min(b.len());
        lay::put_n(&mut b, &QSYS_INFO, "Bytes_Returned", n as u64);
        lay::put_n(&mut b, &QSYS_INFO, "Bytes_Available", QSYS_INFO.size as u64);
        mem(j, out, n).map_err(|_| ApiFail::id("CPF3CF1"))?.copy_from_slice(&b[..n]);
        Ok(())
    })();
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
    0
}

/// Whether an object's type is one of a `Qp0l_Objtypes_List_t`'s.
fn type_matches(types: &[String], objtype: &str) -> bool {
    types.is_empty()
        || types.iter().any(|t| match t.as_str() {
            "*ALLDIR" => matches!(objtype, "*DIR" | "*LIB" | "*FILE" | "*DDIR"),
            "*ALLSTMF" => matches!(objtype, "*STMF" | "*DSTMF"),
            "*ALLQSYS" => objtype == "*LIB" || objtype == "*MBR",
            "*NOQSYS" => objtype != "*LIB" && objtype != "*MBR",
            t => t == objtype,
        })
}

/// The exit a `Qp0l_User_Function_t` names.
enum Exit {
    Proc(usize),
    Pgm(String, String),
}

fn exit_of(j: &Job, uf: u128) -> R<Exit> {
    let b = mem(j, uf, USER_FUNCTION.size)?;
    match u32::try_from(lay::get_n(b, &USER_FUNCTION, "Function_Type")).unwrap_or(u32::MAX) {
        QP0L_USER_FUNCTION_PTR => {
            let (o, _) = lay::at(&USER_FUNCTION, "Procedure");
            let p = fat(ptr_at(b, o));
            if p.kind() != Some(PtrKind::Procedure) {
                return Err(Errno::EINVAL);
            }
            usize::try_from(p.offset).map(Exit::Proc).map_err(|_| Errno::EINVAL)
        }
        QP0L_USER_FUNCTION_PGM => {
            let lib = ccsid::decode_trimmed(&b[4..14]);
            let pgm = ccsid::decode_trimmed(&b[14..24]);
            Ok(Exit::Pgm(pgm, if lib.is_empty() { "*LIBL".into() } else { lib }))
        }
        _ => Err(Errno::EINVAL),
    }
}

/// Storage the exit's parameters live in for one call.
struct Call {
    status: Stable,
    error: Stable,
    ret: Stable,
    name: Stable,
}

impl Call {
    fn new(j: &mut Job, path: &str, status: u32, error: i32) -> Self {
        let mut c = Self {
            status: Stable::new(j, 4),
            error: Stable::new(j, 4),
            ret: Stable::new(j, 4),
            name: {
                let q = qlg_bytes(job_ccsid(j), path);
                let mut s = Stable::new(j, q.len() + 1);
                s.bytes()[..q.len()].copy_from_slice(&q);
                s
            },
        };
        c.status.bytes().copy_from_slice(&status.to_be_bytes());
        c.error.bytes().copy_from_slice(&error.to_be_bytes());
        c
    }

    fn ret(&mut self) -> u32 {
        u32::from_be_bytes(self.ret.bytes()[..4].try_into().expect("4 bytes"))
    }

    fn free(self, j: &mut Job) {
        for s in [&self.status, &self.error, &self.ret, &self.name] {
            j.spaces.kill_at(s.native());
        }
    }
}

/// Call the exit once; whether the walk goes on.
fn call_exit(j: &mut Job, exit: &Exit, path: &str, status: u32, error: i32, info: u64) -> R<bool> {
    let mut c = Call::new(j, path, status, error);
    let params = [c.status.native(), c.error.native(), c.ret.native(), c.name.native(), info];
    match exit {
        Exit::Proc(addr) => mi::act::call_by_ref(j, *addr, &params),
        Exit::Pgm(pgm, lib) => {
            let r = j.call_program(pgm, lib, &params);
            if r.is_err() {
                c.free(j);
                return Err(Errno::EUNKNOWN);
            }
        }
    }
    let go_on = c.ret() == 0 && !j.pending();
    c.free(j);
    Ok(go_on)
}

#[allow(clippy::too_many_arguments)]
fn process_subtree(
    j: &mut Job,
    p: u128,
    subtree: i64,
    types: u128,
    _local: i64,
    inex: u128,
    err_opt: i64,
    uf: u128,
    info: u128,
) -> i64 {
    let r = (|| {
        let path = qlg_path(j, p)?;
        let exit = exit_of(j, uf)?;
        let wanted: Vec<String> = match opt_mem(j, types, 4)? {
            None => Vec::new(),
            Some(b) => {
                let n = usize::try_from(u32::from_be_bytes(b[..4].try_into().expect("4 bytes"))).unwrap_or(0);
                let all = mem(j, types, 4 + 11 * n)?;
                (0..n)
                    .map(|i| ccsid::decode_trimmed(&all[4 + 11 * i..14 + 11 * i]))
                    .collect()
            }
        };
        // Inclusion (0) or exclusion (1) lists of paths.
        let mut include: Vec<String> = Vec::new();
        let mut exclude: Vec<String> = Vec::new();
        if let Some(b) = opt_mem(j, inex, 16)? {
            let kind = u32::from_be_bytes(b[..4].try_into().expect("4 bytes"));
            let n = usize::try_from(u32::from_be_bytes(b[4..8].try_into().expect("4 bytes"))).unwrap_or(0);
            let all = mem(j, inex, 16 + 16 * n)?;
            for i in 0..n {
                let q = ptr_at(all, 16 + 16 * i);
                let s = qlg_path(j, q)?;
                if kind == 0 { include.push(s) } else { exclude.push(s) }
            }
        }
        let under = |p: &str, roots: &[String]| {
            roots.iter().any(|r| {
                let r = r.trim_end_matches('/');
                p.eq_ignore_ascii_case(r) || p.to_ascii_lowercase().starts_with(&format!("{}/", r.to_ascii_lowercase()))
            })
        };
        let info_addr = native(j, info);
        let entries = client(j).walk(&path, WalkOpts::default(), 1 << 20)?;
        let root_depth = ifs::path::components(&path).len();
        let err_opt = uint(err_opt);
        for e in entries {
            let depth = ifs::path::components(&e.path).len().saturating_sub(root_depth);
            if uint(subtree) == QP0L_SUBTREE_NO && depth > 1 {
                continue;
            }
            if (!include.is_empty() && !under(&e.path, &include)) || under(&e.path, &exclude) {
                continue;
            }
            let go_on = match &e.stat {
                Ok(st) => {
                    if !type_matches(&wanted, st.objtype.trim()) {
                        continue;
                    }
                    call_exit(j, &exit, &e.path, QP0L_SELECT_OK, 0, info_addr)?
                }
                Err(er) => match err_opt {
                    QP0L_BYPASS_NO_ERRORID => continue,
                    QP0L_END_PROCESS_SUBTREE => return Err(*er),
                    _ => call_exit(j, &exit, &e.path, QP0L_SELECT_NOT_OK, er.code(), info_addr)?,
                },
            };
            if !go_on {
                return Ok(0);
            }
        }
        call_exit(j, &exit, &path, QP0L_SELECT_DONE, 0, info_addr)?;
        Ok(0)
    })();
    ret(j, r)
}

fn file_id(j: &Job, fid: u128) -> R<FileId> {
    let b: [u8; 16] = mem(j, fid, 16)?.try_into().map_err(|_| Errno::EINVAL)?;
    Ok(FileId::from_bytes(&b))
}

/// `Qp0lGetPathFromFileID`: the path NUL-terminated in the job's CCSID;
/// the buffer, or null (`ERANGE` when it does not fit).
fn path_from_id(j: &mut Job, buf: u128, size: i64, fid: u128) -> u128 {
    let r = (|| {
        let id = file_id(j, fid)?;
        let path = client(j).path_from_file_id(id)?;
        let mut b = encode(job_ccsid(j), &path);
        b.push(0);
        let room = usize::try_from(uint(size)).map_err(|_| Errno::EINVAL)?;
        if b.len() > room {
            return Err(Errno::ERANGE);
        }
        mem(j, buf, b.len())?.copy_from_slice(&b);
        Ok(buf)
    })();
    match r {
        Ok(p) => p,
        Err(e) => {
            fail(j, e);
            raw(FatPtr::null())
        }
    }
}

/// `QlgGetPathFromFileID`: as above, a `Qlg_Path_Name_T` in the buffer.
fn qlg_path_from_id(j: &mut Job, buf: u128, size: i64, fid: u128) -> u128 {
    let r = (|| {
        let id = file_id(j, fid)?;
        let path = client(j).path_from_file_id(id)?;
        let q = qlg_bytes(job_ccsid(j), &path);
        let room = usize::try_from(uint(size)).map_err(|_| Errno::EINVAL)?;
        if q.len() > room {
            return Err(Errno::ERANGE);
        }
        mem(j, buf, q.len())?.copy_from_slice(&q);
        Ok(buf)
    })();
    match r {
        Ok(p) => p,
        Err(e) => {
            fail(j, e);
            raw(FatPtr::null())
        }
    }
}

bound_r9!(b_cvt_path(p: u128; o: u128, f: u128, n: i64, c: i64, e: u128) -> i64 = cvt_path);
bound_c!(b_process_subtree(p: u128, s: i64, t: u128, l: i64, x: u128, e: i64, u: u128, i: u128) -> i64 = process_subtree);
bound_c!(b_path_from_id(b: u128, n: i64, f: u128) -> u128 = path_from_id);
bound_c!(b_qlg_path_from_id(b: u128, n: i64, f: u128) -> u128 = qlg_path_from_id);

/// The procedures with their code addresses.
pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("Qp0lCvtPathToQSYSObjName", at!(b_cvt_path)),
        ("QlgCvtPathToQSYSObjName", at!(b_cvt_path)),
        ("Qp0lProcessSubtree", at!(b_process_subtree)),
        ("QlgProcessSubtree", at!(b_process_subtree)),
        ("Qp0lGetPathFromFileID", at!(b_path_from_id)),
        ("QlgGetPathFromFileID", at!(b_qlg_path_from_id)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(b: &[u8], name: &str) -> String {
        let (o, n) = lay::at(&QSYS_INFO, name);
        ccsid::from_ebcdic(&b[o..o + n]).trim_end().to_string()
    }

    #[test]
    fn qsys_paths_split_into_names() {
        let b = qsys_info("/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR", 37).unwrap();
        assert_eq!(text(&b, "Lib_Name"), "QGPL");
        assert_eq!(text(&b, "Obj_Name"), "QCLSRC");
        assert_eq!(text(&b, "Obj_Type"), "*FILE");
        assert_eq!(text(&b, "Mbr_Name"), "HELLO");
        assert_eq!(text(&b, "Mbr_Type"), "*MBR");
        let b = qsys_info("/qsys.lib/X.PGM", 37).unwrap();
        assert_eq!((text(&b, "Lib_Name"), text(&b, "Obj_Name")), ("QSYS".into(), "X".into()));
        assert_eq!(qsys_info("/home/x", 37).unwrap_err().msgid, "CPFA0DB");
    }
}
