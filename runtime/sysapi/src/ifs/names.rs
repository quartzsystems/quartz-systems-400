// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The calls that name a path: `stat`, `lstat` (and `64`), `access`,
//! `chmod`, `chown`, `lchown`, `link`, `unlink`, `symlink`, `readlink`,
//! `mkdir`, `mkfifo`, `rmdir`, `chdir`, `getcwd`, `utime`, `umask`,
//! `pathconf`, `statvfs` (and `64`), `Qp0lRenameKeep`,
//! `Qp0lRenameUnlink` and C's `rename` (`Qp0lRenameKeep`'s rule, as
//! the header maps it outside POSIX), `strerror` and `perror`'s IFS
//! numbers coming from `net` and here. Each answers 0 or -1 with
//! `errno`, as the header says.

use ifs::Errno;
use ifs::backend::Nanos;
use ifs::ops::RenameMode;
use mi::Job;

use super::file::{bound_r9, put_stat};
use super::layout::{self as lay, UTIMBUF};
use super::path::{cpath, encode};
use super::{R, client, job_ccsid};
use crate::net::abi::{bound_c, fail, int, is_null, mem, opt_mem, raw, ret, uint};

/// The `0`/`-1` answer of a call that returns nothing else.
fn unit(j: &mut Job, r: R<()>) -> i64 {
    ret(j, r.map(|()| 0))
}

/// Stat a path into a buffer.
pub(crate) fn stat_into(j: &mut Job, path: &str, buf: u128, follow: bool, large: bool) -> R<()> {
    let s = client(j).stat(path, follow)?;
    put_stat(j, buf, &s, large)
}

fn stat(j: &mut Job, p: u128, buf: u128) -> i64 {
    let r = cpath(j, p).and_then(|path| stat_into(j, &path, buf, true, false));
    unit(j, r)
}

fn stat64(j: &mut Job, p: u128, buf: u128) -> i64 {
    let r = cpath(j, p).and_then(|path| stat_into(j, &path, buf, true, true));
    unit(j, r)
}

fn lstat(j: &mut Job, p: u128, buf: u128) -> i64 {
    let r = cpath(j, p).and_then(|path| stat_into(j, &path, buf, false, false));
    unit(j, r)
}

fn lstat64(j: &mut Job, p: u128, buf: u128) -> i64 {
    let r = cpath(j, p).and_then(|path| stat_into(j, &path, buf, false, true));
    unit(j, r)
}

fn access(j: &mut Job, p: u128, amode: i64) -> i64 {
    let r = cpath(j, p).and_then(|path| client(j).access(&path, int(amode)));
    unit(j, r)
}

fn chmod(j: &mut Job, p: u128, mode: i64) -> i64 {
    let r = cpath(j, p).and_then(|path| client(j).chmod(&path, uint(mode)));
    unit(j, r)
}

fn chown(j: &mut Job, p: u128, uid: i64, gid: i64) -> i64 {
    let r = cpath(j, p).and_then(|path| client(j).chown(&path, uint(uid), uint(gid)));
    unit(j, r)
}

/// `lchown()`: `chown()` of the path itself.
// MIGRATE(phase11): a link's own owner once IfsClient has an lchown.
fn lchown(j: &mut Job, p: u128, uid: i64, gid: i64) -> i64 {
    chown(j, p, uid, gid)
}

fn link(j: &mut Job, a: u128, b: u128) -> i64 {
    let r = (|| {
        let (x, y) = (cpath(j, a)?, cpath(j, b)?);
        client(j).link(&x, &y)
    })();
    unit(j, r)
}

fn unlink(j: &mut Job, p: u128) -> i64 {
    let r = cpath(j, p).and_then(|path| client(j).unlink(&path));
    unit(j, r)
}

/// Rename with a mode.
pub(crate) fn rename_paths(j: &mut Job, old: &str, new: &str, mode: RenameMode) -> R<()> {
    client(j).rename(old, new, mode)
}

fn rename_with(j: &mut Job, a: u128, b: u128, mode: RenameMode) -> i64 {
    let r = (|| {
        let (x, y) = (cpath(j, a)?, cpath(j, b)?);
        rename_paths(j, &x, &y, mode)
    })();
    unit(j, r)
}

fn rename_keep(j: &mut Job, a: u128, b: u128) -> i64 {
    rename_with(j, a, b, RenameMode::Keep)
}

fn rename_unlink(j: &mut Job, a: u128, b: u128) -> i64 {
    rename_with(j, a, b, RenameMode::Unlink)
}

fn symlink(j: &mut Job, target: u128, p: u128) -> i64 {
    let r = (|| {
        let (t, l) = (cpath(j, target)?, cpath(j, p)?);
        client(j).symlink(&t, &l)
    })();
    unit(j, r)
}

/// The target of a link in the job's CCSID, cut to `room` bytes, no NUL.
pub(crate) fn readlink_into(j: &mut Job, path: &str, buf: u128, room: i64) -> R<i64> {
    let t = client(j).readlink(path)?;
    let room = usize::try_from(uint(room)).map_err(|_| Errno::EINVAL)?;
    let b = encode(job_ccsid(j), &t);
    let n = b.len().min(room);
    mem(j, buf, n)?.copy_from_slice(&b[..n]);
    Ok(i64::try_from(n).unwrap_or(i64::MAX))
}

fn readlink(j: &mut Job, p: u128, buf: u128, room: i64) -> i64 {
    let r = cpath(j, p).and_then(|path| readlink_into(j, &path, buf, room));
    ret(j, r)
}

fn mkdir(j: &mut Job, p: u128, mode: i64) -> i64 {
    let r = cpath(j, p).and_then(|path| client(j).mkdir(&path, uint(mode)));
    unit(j, r)
}

fn mkfifo(j: &mut Job, p: u128, mode: i64) -> i64 {
    let r = cpath(j, p).and_then(|path| client(j).mkfifo(&path, uint(mode)));
    unit(j, r)
}

fn rmdir(j: &mut Job, p: u128) -> i64 {
    let r = cpath(j, p).and_then(|path| client(j).rmdir(&path));
    unit(j, r)
}

fn chdir(j: &mut Job, p: u128) -> i64 {
    let r = cpath(j, p).and_then(|path| client(j).chdir(&path));
    unit(j, r)
}

/// `getcwd()`: the directory in the job's CCSID with a NUL, `ERANGE`
/// when it does not fit; the buffer's pointer, or null.
fn getcwd(j: &mut Job, buf: u128, size: i64) -> u128 {
    let r = (|| {
        let cwd = client(j).getcwd()?;
        let mut b = encode(job_ccsid(j), &cwd);
        b.push(0);
        let room = usize::try_from(uint(size)).map_err(|_| Errno::EINVAL)?;
        if room == 0 {
            return Err(Errno::EINVAL);
        }
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
            raw(mi::FatPtr::null())
        }
    }
}

/// `utime()` times from a `struct utimbuf` (none: now).
pub(crate) fn utime_path(j: &mut Job, path: &str, times: u128) -> R<()> {
    let t = match opt_mem(j, times, UTIMBUF.size)? {
        None => None,
        Some(b) => {
            let a = lay::get_i(b, &UTIMBUF, "actime");
            let m = lay::get_i(b, &UTIMBUF, "modtime");
            let ns = |s: i64| -> Nanos { s.saturating_mul(1_000_000_000) };
            Some((ns(a), ns(m)))
        }
    };
    client(j).utime(path, t)
}

fn utime(j: &mut Job, p: u128, times: u128) -> i64 {
    let r = cpath(j, p).and_then(|path| utime_path(j, &path, times));
    unit(j, r)
}

fn umask(j: &mut Job, mode: i64) -> i64 {
    match client(j).umask(uint(mode) & 0o777) {
        Ok(old) => i64::from(old),
        Err(e) => fail(j, e),
    }
}

/// `pathconf()`: -1 without `errno` for no limit.
pub(crate) fn pathconf_path(j: &mut Job, path: &str, name: i64) -> i64 {
    match client(j).pathconf(path, int(name)) {
        Ok(v) => v.unwrap_or(-1),
        Err(e) => fail(j, e),
    }
}

fn pathconf(j: &mut Job, p: u128, name: i64) -> i64 {
    match cpath(j, p) {
        Ok(path) => pathconf_path(j, &path, name),
        Err(e) => fail(j, e),
    }
}

/// `statvfs()` into a buffer.
pub(crate) fn statvfs_into(j: &mut Job, path: &str, buf: u128) -> R<()> {
    let v = client(j).statvfs(path)?;
    let b = lay::statvfs(&v);
    mem(j, buf, b.len())?.copy_from_slice(&b);
    Ok(())
}

fn statvfs(j: &mut Job, p: u128, buf: u128) -> i64 {
    let r = cpath(j, p).and_then(|path| statvfs_into(j, &path, buf));
    unit(j, r)
}

/// `perror()`: the program's text, a colon and the `errno` text on the
/// job's console (its standard error).
fn perror(j: &mut Job, s: u128) -> i64 {
    let code = j.errno.get();
    let text = sockets::strerror(code);
    let line = if is_null(s) {
        text
    } else {
        match cpath(j, s) {
            Ok(pre) if !pre.is_empty() => format!("{pre}: {text}"),
            _ => text,
        }
    };
    j.console.push(line);
    0
}

bound_c!(b_stat(p: u128, b: u128) -> i64 = stat);
bound_c!(b_stat64(p: u128, b: u128) -> i64 = stat64);
bound_c!(b_lstat(p: u128, b: u128) -> i64 = lstat);
bound_c!(b_lstat64(p: u128, b: u128) -> i64 = lstat64);
bound_c!(b_access(p: u128, m: i64) -> i64 = access);
bound_c!(b_chmod(p: u128, m: i64) -> i64 = chmod);
bound_c!(b_chown(p: u128, u: i64, g: i64) -> i64 = chown);
bound_c!(b_lchown(p: u128, u: i64, g: i64) -> i64 = lchown);
bound_c!(b_link(a: u128, b: u128) -> i64 = link);
bound_c!(b_unlink(p: u128) -> i64 = unlink);
bound_c!(b_rename_keep(a: u128, b: u128) -> i64 = rename_keep);
bound_c!(b_rename_unlink(a: u128, b: u128) -> i64 = rename_unlink);
bound_c!(b_symlink(t: u128, p: u128) -> i64 = symlink);
bound_r9!(b_readlink(p: u128; b: u128, n: i64) -> i64 = readlink);
bound_c!(b_mkdir(p: u128, m: i64) -> i64 = mkdir);
bound_c!(b_mkfifo(p: u128, m: i64) -> i64 = mkfifo);
bound_c!(b_rmdir(p: u128) -> i64 = rmdir);
bound_c!(b_chdir(p: u128) -> i64 = chdir);
bound_c!(b_getcwd(b: u128, n: i64) -> u128 = getcwd);
bound_c!(b_utime(p: u128, t: u128) -> i64 = utime);
bound_c!(b_umask(m: i64) -> i64 = umask);
bound_c!(b_pathconf(p: u128, n: i64) -> i64 = pathconf);
bound_c!(b_statvfs(p: u128, b: u128) -> i64 = statvfs);
bound_c!(b_perror(s: u128) -> i64 = perror);

/// The procedures with their code addresses.
pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("stat", at!(b_stat)),
        ("stat64", at!(b_stat64)),
        ("lstat", at!(b_lstat)),
        ("lstat64", at!(b_lstat64)),
        ("access", at!(b_access)),
        ("chmod", at!(b_chmod)),
        ("chown", at!(b_chown)),
        ("lchown", at!(b_lchown)),
        ("link", at!(b_link)),
        ("unlink", at!(b_unlink)),
        ("Qp0lRenameKeep", at!(b_rename_keep)),
        ("Qp0lRenameUnlink", at!(b_rename_unlink)),
        ("rename", at!(b_rename_keep)),
        ("symlink", at!(b_symlink)),
        ("readlink", at!(b_readlink)),
        ("mkdir", at!(b_mkdir)),
        ("mkfifo", at!(b_mkfifo)),
        ("rmdir", at!(b_rmdir)),
        ("chdir", at!(b_chdir)),
        ("getcwd", at!(b_getcwd)),
        ("utime", at!(b_utime)),
        ("umask", at!(b_umask)),
        ("pathconf", at!(b_pathconf)),
        ("statvfs", at!(b_statvfs)),
        ("statvfs64", at!(b_statvfs)),
        ("perror", at!(b_perror)),
    ]
}
