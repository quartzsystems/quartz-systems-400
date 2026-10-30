// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QP0LLIB1`'s calls on open files: `open`, `open64`, `openat`,
//! `creat`, `creat64`, `pread`, `pwrite` (and `64`), `lseek`,
//! `lseek64`, `fsync`, `ftruncate`, `ftruncate64`, `fstat`, `fstat64`,
//! `fchmod`, `fchown`, `fchdir`, `fpathconf`, `fstatvfs` (and `64`),
//! and `fcntl`'s IFS commands (`descr.rs` has the call itself). `read`,
//! `write` and `close` are the descriptor calls sockets share.

use ifs::Errno;
use ifs::lock::range::RangeKind;
use ifs::ops::flags as fl;
use ifs::ops::{OpenArgs, Stat};
use mi::Job;
use mi::fd::Kind;

use super::consts::{AT_FDCWD, F_RDLCK, F_UNLCK, F_WRLCK};
use super::layout::{self as lay, FLOCK, FLOCK64};
use super::path::cpath;
use super::{R, adopt, client, service_fd};
use crate::net::abi::{bound_c, fail, int, mem, ret, uint};

/// A bound procedure that also sees the argument count, for the calls
/// whose trailing arguments are optional (`open`'s mode and CCSIDs).
macro_rules! bound_argc {
    ($name:ident($($a:ident: $t:ty),*) -> $r:ty = $f:path) => {
        #[allow(clippy::too_many_arguments)]
        extern "sysv64" fn $name(
            env: *mut codegen::abi::Env,
            argc: i64,
            _od: i64,
            $($a: $t),*
        ) -> $r {
            $f(mi::storage::job_of(env), argc, $($a),*)
        }
    };
}
pub(crate) use bound_argc;

/// A bound procedure whose pointer argument finds only `r9` left, with
/// integer arguments after it. Generated code passes the pointer on the
/// stack and leaves `r9` unused; the Rust convention would put the next
/// integer there, so `r9` is taken by a filler first (`;` marks where).
macro_rules! bound_r9 {
    ($name:ident($($a:ident: $t:ty),* ; $($b:ident: $u:ty),*) -> $r:ty = $f:path) => {
        #[allow(clippy::too_many_arguments)]
        extern "sysv64" fn $name(
            env: *mut codegen::abi::Env,
            _argc: i64,
            _od: i64,
            $($a: $t,)*
            _r9: i64,
            $($b: $u),*
        ) -> $r {
            $f(mi::storage::job_of(env), $($a,)* $($b),*)
        }
    };
}
pub(crate) use bound_r9;

fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// The arguments after the flags, as many as were passed.
fn open_args(argc: i64, first: usize, oflag: u32, mode: i64, conv: i64, tconv: i64) -> OpenArgs {
    let has = |i: usize| argc > i64::try_from(i).unwrap_or(i64::MAX);
    let mut a = OpenArgs::new(oflag);
    if has(first) {
        a = a.mode(uint(mode));
    }
    if has(first + 1) {
        a = a.conv(uint(conv));
    }
    if has(first + 2) {
        a = a.text_conv(uint(tconv));
    }
    a
}

/// Open `path` with `args` (relative to `dirfd`'s directory when given);
/// the job's new descriptor.
pub(crate) fn open_path(j: &mut Job, dirfd: Option<i32>, path: &str, args: OpenArgs) -> R<i64> {
    let c = client(j);
    let fd = match dirfd {
        None => c.open(path, args)?,
        Some(d) => c.openat(Some(d), path, args)?,
    };
    adopt(j, fd).map(i64::from)
}

fn open(j: &mut Job, argc: i64, p: u128, oflag: i64, mode: i64, conv: i64, tconv: i64) -> i64 {
    let r = (|| {
        let path = cpath(j, p)?;
        open_path(j, None, &path, open_args(argc, 2, uint(oflag), mode, conv, tconv))
    })();
    ret(j, r)
}

fn open64(j: &mut Job, argc: i64, p: u128, oflag: i64, mode: i64, conv: i64, tconv: i64) -> i64 {
    let flags = uint(oflag) | fl::O_LARGEFILE;
    open(j, argc, p, i64::from(flags), mode, conv, tconv)
}

#[allow(clippy::too_many_arguments)]
fn openat(j: &mut Job, argc: i64, dirfd: i64, p: u128, oflag: i64, mode: i64, conv: i64, tconv: i64) -> i64 {
    let r = (|| {
        let path = cpath(j, p)?;
        let d = match int(dirfd) {
            AT_FDCWD => None,
            n => Some(service_fd(j, n).map_err(|_| Errno::EBADF)?),
        };
        open_path(j, d, &path, open_args(argc, 3, uint(oflag), mode, conv, tconv))
    })();
    ret(j, r)
}

/// `creat()`: `open()` with `O_CREAT | O_WRONLY | O_TRUNC`.
pub(crate) fn creat_path(j: &mut Job, path: &str, mode: u32, large: bool) -> R<i64> {
    let mut flags = fl::O_CREAT | fl::O_WRONLY | fl::O_TRUNC;
    if large {
        flags |= fl::O_LARGEFILE;
    }
    open_path(j, None, path, OpenArgs::new(flags).mode(mode))
}

fn creat(j: &mut Job, p: u128, mode: i64) -> i64 {
    let r = cpath(j, p).and_then(|path| creat_path(j, &path, uint(mode), false));
    ret(j, r)
}

fn creat64(j: &mut Job, p: u128, mode: i64) -> i64 {
    let r = cpath(j, p).and_then(|path| creat_path(j, &path, uint(mode), true));
    ret(j, r)
}

fn len_of(v: i64) -> R<usize> {
    usize::try_from(uint(v)).map_err(|_| Errno::EINVAL)
}

fn pread_at(j: &mut Job, fd: i64, buf: u128, n: i64, off: i64) -> R<i64> {
    let svc = service_fd(j, int(fd)).map_err(|_| Errno::EBADF)?;
    let off = u64::try_from(off).map_err(|_| Errno::EINVAL)?;
    let want = len_of(n)?;
    let b = mem(j, buf, want)?;
    let c = client(j);
    let mut done = 0;
    while done < want {
        let got = c.pread(svc, want - done, off + done as u64)?;
        if got.is_empty() {
            break;
        }
        b[done..done + got.len()].copy_from_slice(&got);
        done += got.len();
    }
    Ok(count(done))
}

fn pwrite_at(j: &mut Job, fd: i64, buf: u128, n: i64, off: i64) -> R<i64> {
    let svc = service_fd(j, int(fd)).map_err(|_| Errno::EBADF)?;
    let off = u64::try_from(off).map_err(|_| Errno::EINVAL)?;
    let want = len_of(n)?;
    let b = mem(j, buf, want)?;
    let c = client(j);
    let mut done = 0;
    while done < want {
        let put = c.pwrite(svc, &b[done..], off + done as u64)?;
        if put == 0 {
            break;
        }
        done += put;
    }
    Ok(count(done))
}

fn pread(j: &mut Job, fd: i64, buf: u128, n: i64, off: i64) -> i64 {
    let r = pread_at(j, fd, buf, n, i64::from(int(off)));
    ret(j, r)
}

fn pread64(j: &mut Job, fd: i64, buf: u128, n: i64, off: i64) -> i64 {
    let r = pread_at(j, fd, buf, n, off);
    ret(j, r)
}

fn pwrite(j: &mut Job, fd: i64, buf: u128, n: i64, off: i64) -> i64 {
    let r = pwrite_at(j, fd, buf, n, i64::from(int(off)));
    ret(j, r)
}

fn pwrite64(j: &mut Job, fd: i64, buf: u128, n: i64, off: i64) -> i64 {
    let r = pwrite_at(j, fd, buf, n, off);
    ret(j, r)
}

fn seek(j: &mut Job, fd: i64, off: i64, whence: i64) -> R<u64> {
    let svc = service_fd(j, int(fd)).map_err(|e| if e == Errno::ENOTSUP { Errno::ESPIPE } else { Errno::EBADF })?;
    client(j).lseek(svc, off, int(whence))
}

fn lseek(j: &mut Job, fd: i64, off: i64, whence: i64) -> i64 {
    let r = seek(j, fd, i64::from(int(off)), whence)
        .and_then(|p| i32::try_from(p).map(i64::from).map_err(|_| Errno::EOVERFLOW));
    ret(j, r)
}

fn lseek64(j: &mut Job, fd: i64, off: i64, whence: i64) -> i64 {
    let r = seek(j, fd, off, whence).and_then(|p| i64::try_from(p).map_err(|_| Errno::EOVERFLOW));
    ret(j, r)
}

fn fsync(j: &mut Job, fd: i64) -> i64 {
    let r = service_fd(j, int(fd))
        .map_err(|_| Errno::EBADF)
        .and_then(|svc| client(j).fsync(svc, false))
        .map(|()| 0);
    ret(j, r)
}

fn truncate_to(j: &mut Job, fd: i64, len: i64) -> R<i64> {
    let svc = service_fd(j, int(fd)).map_err(|_| Errno::EBADF)?;
    let len = u64::try_from(len).map_err(|_| Errno::EINVAL)?;
    client(j).ftruncate(svc, len).map(|()| 0)
}

fn ftruncate(j: &mut Job, fd: i64, len: i64) -> i64 {
    let r = truncate_to(j, fd, i64::from(int(len)));
    ret(j, r)
}

fn ftruncate64(j: &mut Job, fd: i64, len: i64) -> i64 {
    let r = truncate_to(j, fd, len);
    ret(j, r)
}

/// The attributes behind a descriptor: a socket's are made up as
/// IBM's are (`*SOCKET`, `S_IFSOCK`).
pub(crate) fn fd_stat(j: &mut Job, fd: i32) -> R<Stat> {
    match j.fds.kind(fd).map_err(|_| Errno::EBADF)? {
        Kind::Ifs(_) => {
            let svc = service_fd(j, fd)?;
            client(j).fstat(svc)
        }
        Kind::Socket(_) => Ok(Stat {
            mode: fl::S_IFSOCK | 0o777,
            objtype: "*SOCKET".into(),
            nlink: 1,
            blksize: 4096,
            codepage: 819,
            ccsid: 819,
            ..Stat::default()
        }),
    }
}

/// Store a `stat` (`large`: a `stat64`) at a pointer.
pub(crate) fn put_stat(j: &Job, buf: u128, s: &Stat, large: bool) -> R<()> {
    let bytes = if large {
        lay::stat64(s)
    } else {
        lay::stat(s).map_err(|_| Errno::EOVERFLOW)?
    };
    mem(j, buf, bytes.len())?.copy_from_slice(&bytes);
    Ok(())
}

fn fstat(j: &mut Job, fd: i64, buf: u128) -> i64 {
    let r = fd_stat(j, int(fd)).and_then(|s| put_stat(j, buf, &s, false)).map(|()| 0);
    ret(j, r)
}

fn fstat64(j: &mut Job, fd: i64, buf: u128) -> i64 {
    let r = fd_stat(j, int(fd)).and_then(|s| put_stat(j, buf, &s, true)).map(|()| 0);
    ret(j, r)
}

/// The path a job descriptor was opened by.
pub(crate) fn fd_path(j: &mut Job, fd: i64) -> R<String> {
    let svc = service_fd(j, int(fd)).map_err(|_| Errno::EBADF)?;
    client(j).fd_path(svc)
}

fn fchmod(j: &mut Job, fd: i64, mode: i64) -> i64 {
    let r = fd_path(j, fd).and_then(|p| client(j).chmod(&p, uint(mode))).map(|()| 0);
    ret(j, r)
}

fn fchown(j: &mut Job, fd: i64, uid: i64, gid: i64) -> i64 {
    let r = fd_path(j, fd)
        .and_then(|p| client(j).chown(&p, uint(uid), uint(gid)))
        .map(|()| 0);
    ret(j, r)
}

fn fchdir(j: &mut Job, fd: i64) -> i64 {
    let r = service_fd(j, int(fd))
        .map_err(|_| Errno::EBADF)
        .and_then(|svc| client(j).fchdir(svc))
        .map(|()| 0);
    ret(j, r)
}

fn fpathconf(j: &mut Job, fd: i64, name: i64) -> i64 {
    let r = fd_path(j, fd).and_then(|p| client(j).pathconf(&p, int(name))).map(|v| v.unwrap_or(-1));
    match r {
        Ok(v) => v,
        Err(e) => fail(j, e),
    }
}

fn fstatvfs(j: &mut Job, fd: i64, buf: u128) -> i64 {
    let r = (|| {
        let p = fd_path(j, fd)?;
        let v = client(j).statvfs(&p)?;
        let b = lay::statvfs(&v);
        mem(j, buf, b.len())?.copy_from_slice(&b);
        Ok(0)
    })();
    ret(j, r)
}

/// `fcntl()` on an IFS descriptor for the commands the table does not
/// answer itself: the status flags and the byte-range locks. `arg` is
/// the integer argument, `p` the `struct flock` pointer (a pointer
/// argument arrives on the stack, past the register an integer uses).
pub(crate) fn fcntl_ifs(j: &mut Job, svc: i32, cmd: i32, arg: i32, p: u128) -> R<i64> {
    let c = client(j);
    match cmd {
        fl::F_GETLK | fl::F_SETLK | fl::F_SETLKW => lock(j, svc, cmd, p, false),
        fl::F_GETLK64 | fl::F_SETLK64 | fl::F_SETLKW64 => lock(j, svc, cmd, p, true),
        _ => c.fcntl(svc, cmd, arg).map(i64::from),
    }
}

fn lock(j: &mut Job, svc: i32, cmd: i32, p: u128, large: bool) -> R<i64> {
    let l = if large { &FLOCK64 } else { &FLOCK };
    let b = mem(j, p, l.size)?;
    let kind = match i32::try_from(lay::get_i(b, l, "l_type")).unwrap_or(0) {
        F_RDLCK => RangeKind::Read,
        F_WRLCK => RangeKind::Write,
        F_UNLCK => RangeKind::Unlock,
        _ => return Err(Errno::EINVAL),
    };
    let whence = i32::try_from(lay::get_i(b, l, "l_whence")).unwrap_or(-1);
    let start = lay::get_i(b, l, "l_start");
    let len = lay::get_i(b, l, "l_len");
    let c = client(j);
    let blocking = matches!(cmd, fl::F_SETLKW | fl::F_SETLKW64);
    let got = loop {
        match c.lock_range(svc, cmd, kind, whence, start, len) {
            // The service never waits: the job does, retrying.
            Err(Errno::EAGAIN) if blocking => std::thread::sleep(std::time::Duration::from_millis(20)),
            other => break other?,
        }
    };
    if matches!(cmd, fl::F_GETLK | fl::F_GETLK64) {
        match got {
            None => lay::put_n(b, l, "l_type", u64::from(F_UNLCK.cast_unsigned())),
            Some(h) => {
                let t = match h.kind {
                    RangeKind::Read => F_RDLCK,
                    RangeKind::Write => F_WRLCK,
                    RangeKind::Unlock => F_UNLCK,
                };
                lay::put_n(b, l, "l_type", u64::from(t.cast_unsigned()));
                lay::put_n(b, l, "l_whence", 0);
                let n = if h.end == u64::MAX { 0 } else { h.end - h.start };
                let bits = if large { u64::MAX } else { 0xFFFF_FFFF };
                lay::put_n(b, l, "l_start", h.start & bits);
                lay::put_n(b, l, "l_len", n & bits);
                lay::put_n(b, l, "l_pid", h.owner.0 & 0x7FFF_FFFF);
            }
        }
    }
    Ok(0)
}

bound_argc!(b_open(p: u128, o: i64, m: i64, c: i64, t: i64) -> i64 = open);
bound_argc!(b_open64(p: u128, o: i64, m: i64, c: i64, t: i64) -> i64 = open64);
bound_argc!(b_openat(d: i64, p: u128, o: i64, m: i64, c: i64, t: i64) -> i64 = openat);
bound_c!(b_creat(p: u128, m: i64) -> i64 = creat);
bound_c!(b_creat64(p: u128, m: i64) -> i64 = creat64);
bound_c!(b_pread(fd: i64, b: u128, n: i64, o: i64) -> i64 = pread);
bound_c!(b_pread64(fd: i64, b: u128, n: i64, o: i64) -> i64 = pread64);
bound_c!(b_pwrite(fd: i64, b: u128, n: i64, o: i64) -> i64 = pwrite);
bound_c!(b_pwrite64(fd: i64, b: u128, n: i64, o: i64) -> i64 = pwrite64);
bound_c!(b_lseek(fd: i64, o: i64, w: i64) -> i64 = lseek);
bound_c!(b_lseek64(fd: i64, o: i64, w: i64) -> i64 = lseek64);
bound_c!(b_fsync(fd: i64) -> i64 = fsync);
bound_c!(b_ftruncate(fd: i64, l: i64) -> i64 = ftruncate);
bound_c!(b_ftruncate64(fd: i64, l: i64) -> i64 = ftruncate64);
bound_c!(b_fstat(fd: i64, b: u128) -> i64 = fstat);
bound_c!(b_fstat64(fd: i64, b: u128) -> i64 = fstat64);
bound_c!(b_fchmod(fd: i64, m: i64) -> i64 = fchmod);
bound_c!(b_fchown(fd: i64, u: i64, g: i64) -> i64 = fchown);
bound_c!(b_fchdir(fd: i64) -> i64 = fchdir);
bound_c!(b_fpathconf(fd: i64, n: i64) -> i64 = fpathconf);
bound_c!(b_fstatvfs(fd: i64, b: u128) -> i64 = fstatvfs);

/// The procedures with their code addresses.
pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("open", at!(b_open)),
        ("open64", at!(b_open64)),
        ("openat", at!(b_openat)),
        ("creat", at!(b_creat)),
        ("creat64", at!(b_creat64)),
        ("pread", at!(b_pread)),
        ("pread64", at!(b_pread64)),
        ("pwrite", at!(b_pwrite)),
        ("pwrite64", at!(b_pwrite64)),
        ("lseek", at!(b_lseek)),
        ("lseek64", at!(b_lseek64)),
        ("fsync", at!(b_fsync)),
        ("ftruncate", at!(b_ftruncate)),
        ("ftruncate64", at!(b_ftruncate64)),
        ("fstat", at!(b_fstat)),
        ("fstat64", at!(b_fstat64)),
        ("fchmod", at!(b_fchmod)),
        ("fchown", at!(b_fchown)),
        ("fchdir", at!(b_fchdir)),
        ("fpathconf", at!(b_fpathconf)),
        ("fstatvfs", at!(b_fstatvfs)),
        ("fstatvfs64", at!(b_fstatvfs)),
    ]
}
