// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NLS-enabled forms: each `Qlg*` call (and `Qp0lOpen`,
//! `Qp0lUnlink`) takes `Qlg_Path_Name_T` structures where its plain
//! form takes strings, and otherwise behaves as that form does. Names
//! the header maps to another (`QlgOpen` to `Qp0lOpen`, `QlgUnlink` to
//! `Qp0lUnlink`) are registered under both.

use ifs::Errno;
use ifs::ops::{OpenArgs, RenameMode};
use mi::{FatPtr, Job};

use super::file::{bound_argc, bound_r9, creat_path, open_path};
use super::names::{pathconf_path, rename_paths, stat_into, statvfs_into, utime_path};
use super::path::{qlg_bytes, qlg_path};
use super::{R, client, job_ccsid};
use crate::net::abi::{bound_c, fail, int, mem, raw, ret, uint};

fn unit(j: &mut Job, r: R<()>) -> i64 {
    ret(j, r.map(|()| 0))
}

#[allow(clippy::too_many_arguments)]
fn open(j: &mut Job, argc: i64, p: u128, oflag: i64, mode: i64, conv: i64, tconv: i64) -> i64 {
    let r = (|| {
        let path = qlg_path(j, p)?;
        let has = |i: i64| argc > i;
        let mut a = OpenArgs::new(uint(oflag));
        if has(2) {
            a = a.mode(uint(mode));
        }
        if has(3) {
            a = a.conv(uint(conv));
        }
        if has(4) {
            a = a.text_conv(uint(tconv));
        }
        open_path(j, None, &path, a)
    })();
    ret(j, r)
}

#[allow(clippy::too_many_arguments)]
fn open64(j: &mut Job, argc: i64, p: u128, oflag: i64, mode: i64, conv: i64, tconv: i64) -> i64 {
    let flags = uint(oflag) | ifs::ops::flags::O_LARGEFILE;
    open(j, argc, p, i64::from(flags), mode, conv, tconv)
}

fn creat(j: &mut Job, p: u128, mode: i64) -> i64 {
    let r = qlg_path(j, p).and_then(|path| creat_path(j, &path, uint(mode), false));
    ret(j, r)
}

fn creat64(j: &mut Job, p: u128, mode: i64) -> i64 {
    let r = qlg_path(j, p).and_then(|path| creat_path(j, &path, uint(mode), true));
    ret(j, r)
}

fn stat(j: &mut Job, p: u128, buf: u128) -> i64 {
    let r = qlg_path(j, p).and_then(|path| stat_into(j, &path, buf, true, false));
    unit(j, r)
}

fn stat64(j: &mut Job, p: u128, buf: u128) -> i64 {
    let r = qlg_path(j, p).and_then(|path| stat_into(j, &path, buf, true, true));
    unit(j, r)
}

fn lstat(j: &mut Job, p: u128, buf: u128) -> i64 {
    let r = qlg_path(j, p).and_then(|path| stat_into(j, &path, buf, false, false));
    unit(j, r)
}

fn lstat64(j: &mut Job, p: u128, buf: u128) -> i64 {
    let r = qlg_path(j, p).and_then(|path| stat_into(j, &path, buf, false, true));
    unit(j, r)
}

fn access(j: &mut Job, p: u128, amode: i64) -> i64 {
    let r = qlg_path(j, p).and_then(|path| client(j).access(&path, int(amode)));
    unit(j, r)
}

fn chdir(j: &mut Job, p: u128) -> i64 {
    let r = qlg_path(j, p).and_then(|path| client(j).chdir(&path));
    unit(j, r)
}

fn chmod(j: &mut Job, p: u128, mode: i64) -> i64 {
    let r = qlg_path(j, p).and_then(|path| client(j).chmod(&path, uint(mode)));
    unit(j, r)
}

fn chown(j: &mut Job, p: u128, uid: i64, gid: i64) -> i64 {
    let r = qlg_path(j, p).and_then(|path| client(j).chown(&path, uint(uid), uint(gid)));
    unit(j, r)
}

fn link(j: &mut Job, a: u128, b: u128) -> i64 {
    let r = (|| {
        let (x, y) = (qlg_path(j, a)?, qlg_path(j, b)?);
        client(j).link(&x, &y)
    })();
    unit(j, r)
}

fn symlink(j: &mut Job, t: u128, p: u128) -> i64 {
    let r = (|| {
        let (x, y) = (qlg_path(j, t)?, qlg_path(j, p)?);
        client(j).symlink(&x, &y)
    })();
    unit(j, r)
}

fn mkdir(j: &mut Job, p: u128, mode: i64) -> i64 {
    let r = qlg_path(j, p).and_then(|path| client(j).mkdir(&path, uint(mode)));
    unit(j, r)
}

fn mkfifo(j: &mut Job, p: u128, mode: i64) -> i64 {
    let r = qlg_path(j, p).and_then(|path| client(j).mkfifo(&path, uint(mode)));
    unit(j, r)
}

fn rmdir(j: &mut Job, p: u128) -> i64 {
    let r = qlg_path(j, p).and_then(|path| client(j).rmdir(&path));
    unit(j, r)
}

fn unlink(j: &mut Job, p: u128) -> i64 {
    let r = qlg_path(j, p).and_then(|path| client(j).unlink(&path));
    unit(j, r)
}

fn rename_with(j: &mut Job, a: u128, b: u128, mode: RenameMode) -> i64 {
    let r = (|| {
        let (x, y) = (qlg_path(j, a)?, qlg_path(j, b)?);
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

/// `QlgReadlink`: the target as a `Qlg_Path_Name_T` in the job's
/// CCSID into the caller's buffer of `size` bytes (structure
/// included); the bytes of the name placed.
fn readlink(j: &mut Job, p: u128, out: u128, size: i64) -> i64 {
    let r = (|| {
        let path = qlg_path(j, p)?;
        let room = usize::try_from(uint(size)).map_err(|_| Errno::EINVAL)?;
        if room < 32 {
            return Err(Errno::EINVAL);
        }
        let t = client(j).readlink(&path)?;
        let q = qlg_bytes(job_ccsid(j), &t);
        let n = q.len().min(room);
        mem(j, out, n)?.copy_from_slice(&q[..n]);
        Ok(i64::try_from(n - 32).unwrap_or(0))
    })();
    ret(j, r)
}

fn pathconf(j: &mut Job, p: u128, name: i64) -> i64 {
    match qlg_path(j, p) {
        Ok(path) => pathconf_path(j, &path, name),
        Err(e) => fail(j, e),
    }
}

fn statvfs(j: &mut Job, p: u128, buf: u128) -> i64 {
    let r = qlg_path(j, p).and_then(|path| statvfs_into(j, &path, buf));
    unit(j, r)
}

fn utime(j: &mut Job, p: u128, t: u128) -> i64 {
    let r = qlg_path(j, p).and_then(|path| utime_path(j, &path, t));
    unit(j, r)
}

/// `QlgGetcwd`: the current directory as a `Qlg_Path_Name_T` (the name
/// NUL-terminated when there is room) in `size` bytes; the buffer's
/// pointer, or null with `ERANGE`.
fn getcwd(j: &mut Job, out: u128, size: i64) -> u128 {
    let r = (|| {
        let cwd = client(j).getcwd()?;
        let mut q = qlg_bytes(job_ccsid(j), &cwd);
        let room = usize::try_from(uint(size)).map_err(|_| Errno::EINVAL)?;
        if q.len() > room {
            return Err(Errno::ERANGE);
        }
        if q.len() < room {
            q.push(0);
        }
        mem(j, out, q.len())?.copy_from_slice(&q);
        Ok(out)
    })();
    match r {
        Ok(p) => p,
        Err(e) => {
            fail(j, e);
            raw(FatPtr::null())
        }
    }
}

bound_argc!(b_open(p: u128, o: i64, m: i64, c: i64, t: i64) -> i64 = open);
bound_argc!(b_open64(p: u128, o: i64, m: i64, c: i64, t: i64) -> i64 = open64);
bound_c!(b_creat(p: u128, m: i64) -> i64 = creat);
bound_c!(b_creat64(p: u128, m: i64) -> i64 = creat64);
bound_c!(b_stat(p: u128, b: u128) -> i64 = stat);
bound_c!(b_stat64(p: u128, b: u128) -> i64 = stat64);
bound_c!(b_lstat(p: u128, b: u128) -> i64 = lstat);
bound_c!(b_lstat64(p: u128, b: u128) -> i64 = lstat64);
bound_c!(b_access(p: u128, m: i64) -> i64 = access);
bound_c!(b_chdir(p: u128) -> i64 = chdir);
bound_c!(b_chmod(p: u128, m: i64) -> i64 = chmod);
bound_c!(b_chown(p: u128, u: i64, g: i64) -> i64 = chown);
bound_c!(b_link(a: u128, b: u128) -> i64 = link);
bound_c!(b_symlink(t: u128, p: u128) -> i64 = symlink);
bound_c!(b_mkdir(p: u128, m: i64) -> i64 = mkdir);
bound_c!(b_mkfifo(p: u128, m: i64) -> i64 = mkfifo);
bound_c!(b_rmdir(p: u128) -> i64 = rmdir);
bound_c!(b_unlink_qlg(p: u128) -> i64 = unlink);
bound_c!(b_rename_keep(a: u128, b: u128) -> i64 = rename_keep);
bound_c!(b_rename_unlink(a: u128, b: u128) -> i64 = rename_unlink);
bound_r9!(b_readlink(p: u128; o: u128, n: i64) -> i64 = readlink);
bound_c!(b_pathconf(p: u128, n: i64) -> i64 = pathconf);
bound_c!(b_statvfs(p: u128, b: u128) -> i64 = statvfs);
bound_c!(b_utime(p: u128, t: u128) -> i64 = utime);
bound_c!(b_getcwd(o: u128, n: i64) -> u128 = getcwd);

/// The procedures with their code addresses.
pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("Qp0lOpen", at!(b_open)),
        ("QlgOpen", at!(b_open)),
        ("QlgOpen64", at!(b_open64)),
        ("QlgCreat", at!(b_creat)),
        ("QlgCreat64", at!(b_creat64)),
        ("QlgStat", at!(b_stat)),
        ("QlgStat64", at!(b_stat64)),
        ("QlgLstat", at!(b_lstat)),
        ("QlgLstat64", at!(b_lstat64)),
        ("QlgAccess", at!(b_access)),
        ("QlgChdir", at!(b_chdir)),
        ("QlgChmod", at!(b_chmod)),
        ("QlgChown", at!(b_chown)),
        ("QlgLchown", at!(b_chown)),
        ("QlgLink", at!(b_link)),
        ("QlgSymlink", at!(b_symlink)),
        ("QlgMkdir", at!(b_mkdir)),
        ("QlgMkfifo", at!(b_mkfifo)),
        ("QlgRmdir", at!(b_rmdir)),
        ("Qp0lUnlink", at!(b_unlink_qlg)),
        ("QlgUnlink", at!(b_unlink_qlg)),
        ("QlgRenameKeep", at!(b_rename_keep)),
        ("QlgRenameUnlink", at!(b_rename_unlink)),
        ("QlgReadlink", at!(b_readlink)),
        ("QlgPathconf", at!(b_pathconf)),
        ("QlgStatvfs", at!(b_statvfs)),
        ("QlgStatvfs64", at!(b_statvfs)),
        ("QlgUtime", at!(b_utime)),
        ("QlgGetcwd", at!(b_getcwd)),
    ]
}
