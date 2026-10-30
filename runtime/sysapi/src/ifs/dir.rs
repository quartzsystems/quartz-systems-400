// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Directory streams: `opendir`, `readdir`, `readdir_r`, `rewinddir`,
//! `closedir`, and the NLS forms `QlgOpendir`, `QlgReaddir`,
//! `QlgReaddir_r` whose entries are `dirent_lg`. A `DIR *` points to a
//! small block the job keeps per stream; `readdir` answers a pointer
//! to the stream's own entry, overwritten by the next call. Entries
//! come from the service in batches.

use std::collections::VecDeque;

use ifs::Errno;
use ifs::ops::DirEnt;
use mi::{FatPtr, Job};

use super::layout::{self as lay, DIRENT, DIRENT_LG};
use super::path::{cpath, encode, qlg_bytes, qlg_path};
use super::{Dir, R, client, job_ccsid, state};
use crate::net::Stable;
use crate::net::abi::{bound_c, fail, fat, mem, put_ptr, raw};

/// Entries asked of the service at once.
const BATCH: usize = 64;

/// What a `DIR` block holds (the program never reads it).
const DIR_SIZE: usize = 32;

fn null() -> u128 {
    raw(FatPtr::null())
}

/// Open a stream on `path`; the `DIR` pointer.
pub(crate) fn open_dir(j: &mut Job, path: &str) -> R<u128> {
    let handle = client(j).opendir(path)?;
    let mut dir = Stable::new(j, DIR_SIZE);
    dir.bytes()[..4].copy_from_slice(&ccsid::to_ebcdic("QDIR").unwrap_or_default());
    dir.bytes()[8..16].copy_from_slice(&handle.to_be_bytes());
    let p = raw(dir.ptr());
    let native = dir.native();
    state(j).borrow_mut().dirs.insert(
        native,
        Dir {
            handle,
            block: dir,
            entry: None,
            entry_lg: None,
            ahead: VecDeque::new(),
        },
    );
    Ok(p)
}

/// The native address a `DIR` pointer names, when it is an open stream.
fn native_of(j: &mut Job, p: u128) -> R<u64> {
    let (at, _) = j.spaces.extent(fat(p)).map_err(|_| Errno::EBADF)?;
    if state(j).borrow().dirs.contains_key(&at) {
        Ok(at)
    } else {
        Err(Errno::EBADF)
    }
}

/// The next entry of a stream, `None` at the end.
fn next(j: &mut Job, p: u128) -> R<Option<DirEnt>> {
    let at = native_of(j, p)?;
    let st = state(j);
    if let Some(e) = st.borrow_mut().dirs.get_mut(&at).and_then(|d| d.ahead.pop_front()) {
        return Ok(Some(e));
    }
    let handle = st.borrow().dirs.get(&at).map(|d| d.handle).ok_or(Errno::EBADF)?;
    let batch = client(j).readdir(handle, BATCH)?;
    let mut s = st.borrow_mut();
    let d = s.dirs.get_mut(&at).ok_or(Errno::EBADF)?;
    d.ahead.extend(batch);
    Ok(d.ahead.pop_front())
}

/// The record length an entry reports: through the name's NUL, on a
/// four-byte boundary.
fn reclen(base: usize, name: usize) -> u64 {
    ((base + name + 1).div_ceil(4) * 4) as u64
}

/// A `struct dirent` of an entry, names in the job's CCSID.
pub(crate) fn dirent_bytes(ccsid: u16, e: &DirEnt) -> R<Vec<u8>> {
    let l = &DIRENT;
    let name = encode(ccsid, &e.name);
    let (_, room) = lay::at(l, "d_name");
    if name.len() >= room {
        return Err(Errno::ENAMETOOLONG);
    }
    let mut b = vec![0u8; l.size];
    lay::put_n(&mut b, l, "d_fileno", e.ino & 0xFFFF_FFFF);
    lay::put_n(&mut b, l, "d_reclen", reclen(lay::at(l, "d_name").0, name.len()));
    let (nls, _) = lay::at(l, "d_nlsinfo");
    b[nls..nls + 4].copy_from_slice(&u32::from(ccsid).to_be_bytes());
    lay::put_n(&mut b, l, "d_namelen", name.len() as u64);
    lay::put(&mut b, l, "d_name", &name);
    Ok(b)
}

/// A `struct dirent_lg` of an entry: the name after a
/// `Qlg_Path_Name_T` in the job's CCSID, not NUL-terminated.
pub(crate) fn dirent_lg_bytes(ccsid: u16, e: &DirEnt) -> R<Vec<u8>> {
    let l = &DIRENT_LG;
    let q = qlg_bytes(ccsid, &e.name);
    let (_, room) = lay::at(l, "d_lg_name");
    if q.len() - 32 > room {
        return Err(Errno::ENAMETOOLONG);
    }
    let mut b = vec![0u8; l.size];
    lay::put_n(&mut b, l, "d_lg_fileno", e.ino & 0xFFFF_FFFF);
    lay::put_n(&mut b, l, "d_lg_reclen", reclen(lay::at(l, "d_lg_name").0, q.len() - 32));
    lay::put(&mut b, l, "d_lg_qlg", &q[..32]);
    lay::put(&mut b, l, "d_lg_name", &q[32..]);
    Ok(b)
}

/// The stream's own entry storage, made on first use; its pointer.
fn entry_ptr(j: &mut Job, at: u64, lg: bool, bytes: &[u8]) -> R<u128> {
    let st = state(j);
    let missing = {
        let s = st.borrow();
        let d = s.dirs.get(&at).ok_or(Errno::EBADF)?;
        if lg { d.entry_lg.is_none() } else { d.entry.is_none() }
    };
    if missing {
        let e = Stable::new(j, bytes.len());
        let mut s = st.borrow_mut();
        let d = s.dirs.get_mut(&at).ok_or(Errno::EBADF)?;
        if lg {
            d.entry_lg = Some(e);
        } else {
            d.entry = Some(e);
        }
    }
    let mut s = st.borrow_mut();
    let d = s.dirs.get_mut(&at).ok_or(Errno::EBADF)?;
    let e = if lg { d.entry_lg.as_mut() } else { d.entry.as_mut() }.ok_or(Errno::EBADF)?;
    e.bytes()[..bytes.len()].copy_from_slice(bytes);
    Ok(raw(e.ptr()))
}

fn read_one(j: &mut Job, p: u128, lg: bool) -> R<Option<u128>> {
    let Some(e) = next(j, p)? else {
        return Ok(None);
    };
    let ccsid = job_ccsid(j);
    let bytes = if lg { dirent_lg_bytes(ccsid, &e)? } else { dirent_bytes(ccsid, &e)? };
    let at = native_of(j, p)?;
    entry_ptr(j, at, lg, &bytes).map(Some)
}

fn readdir_any(j: &mut Job, p: u128, lg: bool) -> u128 {
    match read_one(j, p, lg) {
        Ok(Some(ptr)) => ptr,
        // The end of the stream leaves errno alone.
        Ok(None) => null(),
        Err(e) => {
            fail(j, e);
            null()
        }
    }
}

/// `readdir_r`: the entry into the caller's storage and `*result`
/// set to it, or to null at the end; an error number, not `errno`.
fn readdir_r_any(j: &mut Job, p: u128, entry: u128, result: u128, lg: bool) -> i64 {
    let r = (|| -> R<()> {
        let res = mem(j, result, 16)?;
        match next(j, p)? {
            None => {
                put_ptr(res, 0, FatPtr::null());
                Ok(())
            }
            Some(e) => {
                let ccsid = job_ccsid(j);
                let bytes = if lg { dirent_lg_bytes(ccsid, &e)? } else { dirent_bytes(ccsid, &e)? };
                mem(j, entry, bytes.len())?.copy_from_slice(&bytes);
                put_ptr(res, 0, fat(entry));
                Ok(())
            }
        }
    })();
    match r {
        Ok(()) => 0,
        Err(e) => i64::from(e.code()),
    }
}

fn opendir(j: &mut Job, p: u128) -> u128 {
    match cpath(j, p).and_then(|path| open_dir(j, &path)) {
        Ok(d) => d,
        Err(e) => {
            fail(j, e);
            null()
        }
    }
}

fn qlg_opendir(j: &mut Job, p: u128) -> u128 {
    match qlg_path(j, p).and_then(|path| open_dir(j, &path)) {
        Ok(d) => d,
        Err(e) => {
            fail(j, e);
            null()
        }
    }
}

fn readdir(j: &mut Job, p: u128) -> u128 {
    readdir_any(j, p, false)
}

fn qlg_readdir(j: &mut Job, p: u128) -> u128 {
    readdir_any(j, p, true)
}

fn readdir_r(j: &mut Job, p: u128, entry: u128, result: u128) -> i64 {
    readdir_r_any(j, p, entry, result, false)
}

fn qlg_readdir_r(j: &mut Job, p: u128, entry: u128, result: u128) -> i64 {
    readdir_r_any(j, p, entry, result, true)
}

fn rewinddir(j: &mut Job, p: u128) -> i64 {
    let r = (|| {
        let at = native_of(j, p)?;
        let st = state(j);
        let handle = {
            let mut s = st.borrow_mut();
            let d = s.dirs.get_mut(&at).ok_or(Errno::EBADF)?;
            d.ahead.clear();
            d.handle
        };
        client(j).rewinddir(handle)
    })();
    // rewinddir() returns nothing; an error only sets errno.
    if let Err(e) = r {
        fail(j, e);
    }
    0
}

fn closedir(j: &mut Job, p: u128) -> i64 {
    let r = (|| {
        let at = native_of(j, p)?;
        let d = state(j).borrow_mut().dirs.remove(&at).ok_or(Errno::EBADF)?;
        for s in [Some(&d.block), d.entry.as_ref(), d.entry_lg.as_ref()].into_iter().flatten() {
            j.spaces.kill_at(s.native());
        }
        client(j).closedir(d.handle)
    })();
    crate::net::abi::ret(j, r.map(|()| 0))
}

bound_c!(b_opendir(p: u128) -> u128 = opendir);
bound_c!(b_qlg_opendir(p: u128) -> u128 = qlg_opendir);
bound_c!(b_readdir(d: u128) -> u128 = readdir);
bound_c!(b_qlg_readdir(d: u128) -> u128 = qlg_readdir);
bound_c!(b_readdir_r(d: u128, e: u128, r: u128) -> i64 = readdir_r);
bound_c!(b_qlg_readdir_r(d: u128, e: u128, r: u128) -> i64 = qlg_readdir_r);
bound_c!(b_rewinddir(d: u128) -> i64 = rewinddir);
bound_c!(b_closedir(d: u128) -> i64 = closedir);

/// The procedures with their code addresses.
pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("opendir", at!(b_opendir)),
        ("readdir", at!(b_readdir)),
        ("readdir_r", at!(b_readdir_r)),
        ("rewinddir", at!(b_rewinddir)),
        ("closedir", at!(b_closedir)),
        ("QlgOpendir", at!(b_qlg_opendir)),
        ("QlgReaddir", at!(b_qlg_readdir)),
        ("QlgReaddir_r", at!(b_qlg_readdir_r)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ifs::resolve::NodeType;

    #[test]
    fn an_entry_in_storage() {
        let e = DirEnt {
            name: "a.txt".into(),
            ino: 0x1_0000_0007,
            ntype: NodeType::File,
        };
        let b = dirent_bytes(37, &e).unwrap();
        assert_eq!(lay::get_n(&b, &DIRENT, "d_fileno"), 7);
        assert_eq!(lay::get_n(&b, &DIRENT, "d_namelen"), 5);
        assert_eq!(lay::get_n(&b, &DIRENT, "d_reclen"), 64);
        assert_eq!(&b[56..62], &[0x81, 0x4B, 0xA3, 0xA7, 0xA3, 0]);
        let g = dirent_lg_bytes(37, &e).unwrap();
        assert_eq!(lay::get_n(&g, &DIRENT_LG, "d_lg_fileno"), 7);
        assert_eq!(&g[72..77], &[0x81, 0x4B, 0xA3, 0xA7, 0xA3]);
        assert_eq!(&g[40..44], &37u32.to_be_bytes());
    }
}
