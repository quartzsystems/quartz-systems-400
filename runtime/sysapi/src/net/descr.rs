// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QP0LLIB1`'s descriptor calls sockets and IFS files share (`read`,
//! `write`, `readv`, `writev`, `close`, `dup`, `dup2`, `fcntl`,
//! `ioctl`, `select`), `QC2UTIL1`'s `__errno` and `strerror`, and the
//! byte order helpers.

use mi::Job;
use mi::fd::Kind;
use sockets::consts::{
    F_DUPFD, F_GETFD, F_GETFL, F_SETFD, F_SETFL, FD_CLOEXEC, FIONBIO, FIONREAD, O_NONBLOCK, O_RDWR, POLLIN,
    POLLOUT, POLLPRI,
};
use sockets::layout::misc::{FdSet, timeval};
use sockets::Errno;

use super::abi::{bound_c, errno, get_int, int, is_null, mem, opt_mem, put_int, raw, ret, uint, ushort};
use super::calls::{iovecs, poll_items};
use super::net;

type R<T> = Result<T, Errno>;

fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn len_of(v: i64) -> R<usize> {
    usize::try_from(int(v)).map_err(|_| Errno::EINVAL)
}

fn ifs_read(j: &mut Job, h: u32, b: &mut [u8]) -> R<usize> {
    let ops = j.fds.ifs_ops().ok_or(Errno::ENOSYS)?;
    ops.read(h, b).map_err(errno)
}

fn ifs_write(j: &mut Job, h: u32, b: &[u8]) -> R<usize> {
    let ops = j.fds.ifs_ops().ok_or(Errno::ENOSYS)?;
    ops.write(h, b).map_err(errno)
}

fn kind(j: &Job, fd: i64) -> R<Kind> {
    j.fds.kind(int(fd)).map_err(errno)
}

fn read(j: &mut Job, fd: i64, buf: u128, n: i64) -> i64 {
    let r = (|| {
        let k = kind(j, fd)?;
        let b = mem(j, buf, len_of(n)?)?;
        match k {
            Kind::Socket(s) => net(j).borrow().sockets.borrow().read(s, b).map(count),
            Kind::Ifs(h) => ifs_read(j, h, b).map(count),
        }
    })();
    ret(j, r)
}

fn write(j: &mut Job, fd: i64, buf: u128, n: i64) -> i64 {
    let r = (|| {
        let k = kind(j, fd)?;
        let b = mem(j, buf, len_of(n)?)?;
        match k {
            Kind::Socket(s) => net(j).borrow().sockets.borrow().write(s, b).map(count),
            Kind::Ifs(h) => ifs_write(j, h, b).map(count),
        }
    })();
    ret(j, r)
}

fn readv(j: &mut Job, fd: i64, iov: u128, n: i64) -> i64 {
    let r = (|| {
        let k = kind(j, fd)?;
        let mut bufs = iovecs(j, iov, n)?;
        match k {
            Kind::Socket(s) => {
                let mut views: Vec<&mut [u8]> = bufs.iter_mut().map(|b| &mut **b).collect();
                net(j).borrow().sockets.borrow().recvv(s, &mut views, 0).map(|r| count(r.bytes))
            }
            Kind::Ifs(h) => {
                let mut total = 0;
                for b in &mut bufs {
                    let got = ifs_read(j, h, b)?;
                    total += got;
                    if got < b.len() {
                        break;
                    }
                }
                Ok(count(total))
            }
        }
    })();
    ret(j, r)
}

fn writev(j: &mut Job, fd: i64, iov: u128, n: i64) -> i64 {
    let r = (|| {
        let k = kind(j, fd)?;
        let bufs = iovecs(j, iov, n)?;
        match k {
            Kind::Socket(s) => {
                let views: Vec<&[u8]> = bufs.iter().map(|b| &**b).collect();
                net(j).borrow().sockets.borrow().sendv(s, &views, 0, None).map(count)
            }
            Kind::Ifs(h) => {
                let mut total = 0;
                for b in &bufs {
                    let put = ifs_write(j, h, b)?;
                    total += put;
                    if put < b.len() {
                        break;
                    }
                }
                Ok(count(total))
            }
        }
    })();
    ret(j, r)
}

fn close(j: &mut Job, fd: i64) -> i64 {
    let r = j.fds.close(int(fd)).map(|()| 0).map_err(errno);
    ret(j, r)
}

fn dup(j: &mut Job, fd: i64) -> i64 {
    let r = j.fds.dup(int(fd)).map(i64::from).map_err(errno);
    ret(j, r)
}

fn dup2(j: &mut Job, fd: i64, to: i64) -> i64 {
    let r = j.fds.dup2(int(fd), int(to)).map(i64::from).map_err(errno);
    ret(j, r)
}

/// `O_NONBLOCK` on the description and, for a socket, in the kernel.
fn set_nonblock(j: &mut Job, fd: i32, on: bool) -> R<()> {
    if let Kind::Socket(s) = j.fds.kind(fd).map_err(errno)? {
        net(j).borrow().sockets.borrow().set_nonblock(s, on)?;
    }
    j.fds.set_nonblock(fd, on).map_err(errno)
}

/// `fcntl()`: the descriptor flags and `F_DUPFD` are the table's; the
/// status flags are the socket's or the IFS file's; the byte-range
/// locks are the IFS's. `p` is the third argument when it is a
/// pointer: a pointer that finds one register left goes on the stack
/// and leaves the register unused, where an integer third argument is.
fn fcntl(j: &mut Job, fd: i64, cmd: i64, arg: i64, p: u128) -> i64 {
    let fd = int(fd);
    let arg = int(arg);
    let r = (|| match int(cmd) {
        F_GETFD => Ok(i64::from(if j.fds.cloexec(fd).map_err(errno)? { FD_CLOEXEC } else { 0 })),
        F_SETFD => j.fds.set_cloexec(fd, arg & FD_CLOEXEC != 0).map(|()| 0).map_err(errno),
        F_DUPFD => j.fds.dup_from(fd, arg).map(i64::from).map_err(errno),
        c => match j.fds.kind(fd).map_err(errno)? {
            Kind::Ifs(h) => {
                let svc = i32::try_from(h).map_err(|_| Errno::EBADF)?;
                let v = crate::ifs::fcntl_ifs(j, svc, c, arg, p)?;
                if c == F_SETFL {
                    j.fds.set_nonblock(fd, arg & O_NONBLOCK != 0).map_err(errno)?;
                }
                Ok(v)
            }
            Kind::Socket(_) => match c {
                F_GETFL => {
                    let nb = j.fds.nonblock(fd).map_err(errno)?;
                    Ok(i64::from(O_RDWR | if nb { O_NONBLOCK } else { 0 }))
                }
                F_SETFL => set_nonblock(j, fd, arg & O_NONBLOCK != 0).map(|()| 0),
                _ => Err(Errno::EINVAL),
            },
        },
    })();
    ret(j, r)
}

fn ioctl(j: &mut Job, fd: i64, req: i64, argp: u128) -> i64 {
    let fd = int(fd);
    let r = (|| match int(req) {
        FIONBIO => {
            let on = get_int(j, argp)? != 0;
            set_nonblock(j, fd, on).map(|()| 0)
        }
        FIONREAD => {
            let n = match j.fds.kind(fd).map_err(errno)? {
                Kind::Socket(s) => net(j).borrow().sockets.borrow().pending(s)?,
                Kind::Ifs(_) => return Err(Errno::ENOTSUP),
            };
            put_int(j, argp, i32::try_from(n).unwrap_or(i32::MAX))?;
            Ok(0)
        }
        _ => {
            j.fds.kind(fd).map_err(errno)?;
            Err(Errno::EINVAL)
        }
    })();
    ret(j, r)
}

fn select(j: &mut Job, nfds: i64, rd: u128, wr: u128, ex: u128, tv: u128) -> i64 {
    let r = (|| {
        let nfds = int(nfds);
        if nfds < 0 {
            return Err(Errno::EINVAL);
        }
        let sets: Vec<Option<FdSet>> = [rd, wr, ex]
            .iter()
            .map(|p| opt_mem(j, *p, 28).map(|b| b.map(|b| FdSet::from_bytes(b))))
            .collect::<R<_>>()?;
        let timeout = match opt_mem(j, tv, 8)? {
            None => -1,
            Some(b) => {
                let d = timeval(b).ok_or(Errno::EINVAL)?;
                i32::try_from(d.as_millis()).unwrap_or(i32::MAX)
            }
        };
        let mut items: Vec<(i32, i16)> = Vec::new();
        for fd in 0..nfds.min(i32::try_from(sockets::consts::FD_SETSIZE).expect("small")) {
            let mut ev = 0;
            for (set, bit) in sets.iter().zip([POLLIN, POLLOUT, POLLPRI]) {
                if set.is_some_and(|s| s.contains(fd)) {
                    ev |= bit;
                }
            }
            if ev != 0 {
                if !j.fds.is_open(fd) {
                    return Err(Errno::EBADF);
                }
                items.push((fd, ev));
            }
        }
        let rev = poll_items(j, &items, timeout)?;
        let mut out = [FdSet::new(), FdSet::new(), FdSet::new()];
        let mut ready = 0;
        for ((fd, ev), r) in items.iter().zip(rev) {
            let hup = r & (sockets::consts::POLLHUP | sockets::consts::POLLERR) != 0;
            for (i, bit) in [POLLIN, POLLOUT, POLLPRI].into_iter().enumerate() {
                // A hang-up or error makes a descriptor readable and
                // writable: the next call reports it.
                let hit = r & bit != 0 || (hup && i < 2);
                if ev & bit != 0 && hit {
                    out[i].insert(*fd);
                    ready += 1;
                }
            }
        }
        for (p, s) in [rd, wr, ex].iter().zip(out) {
            if !is_null(*p) {
                mem(j, *p, 28)?.copy_from_slice(&s.to_bytes());
            }
        }
        Ok(ready)
    })();
    ret(j, r)
}

fn errno_ptr(j: &mut Job) -> u128 {
    raw(j.errno.pointer(&mut j.spaces))
}

fn strerror(j: &mut Job, code: i64) -> u128 {
    let code = int(code);
    let text = sockets::strerror(code);
    let n = net(j);
    raw(n.borrow_mut().text(j, "strerror", code, &text))
}

fn htonl(_j: &mut Job, v: i64) -> i64 {
    i64::from(sockets::htonl(uint(v)))
}

fn htons(_j: &mut Job, v: i64) -> i64 {
    i64::from(sockets::htons(ushort(v)))
}

bound_c!(b_read(fd: i64, b: u128, n: i64) -> i64 = read);
bound_c!(b_write(fd: i64, b: u128, n: i64) -> i64 = write);
bound_c!(b_readv(fd: i64, v: u128, n: i64) -> i64 = readv);
bound_c!(b_writev(fd: i64, v: u128, n: i64) -> i64 = writev);
bound_c!(b_close(fd: i64) -> i64 = close);
bound_c!(b_dup(fd: i64) -> i64 = dup);
bound_c!(b_dup2(fd: i64, to: i64) -> i64 = dup2);
bound_c!(b_fcntl(fd: i64, c: i64, a: i64, p: u128) -> i64 = fcntl);
bound_c!(b_ioctl(fd: i64, r: i64, a: u128) -> i64 = ioctl);
bound_c!(b_select(n: i64, r: u128, w: u128, e: u128, t: u128) -> i64 = select);
bound_c!(b_errno() -> u128 = errno_ptr);
bound_c!(b_strerror(c: i64) -> u128 = strerror);
bound_c!(b_htonl(v: i64) -> i64 = htonl);
bound_c!(b_htons(v: i64) -> i64 = htons);

/// The procedures with their code addresses.
pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("read", at!(b_read)),
        ("write", at!(b_write)),
        ("readv", at!(b_readv)),
        ("writev", at!(b_writev)),
        ("close", at!(b_close)),
        ("dup", at!(b_dup)),
        ("dup2", at!(b_dup2)),
        ("fcntl", at!(b_fcntl)),
        ("ioctl", at!(b_ioctl)),
        ("select", at!(b_select)),
        ("__errno", at!(b_errno)),
        ("strerror", at!(b_strerror)),
        ("htonl", at!(b_htonl)),
        ("ntohl", at!(b_htonl)),
        ("htons", at!(b_htons)),
        ("ntohs", at!(b_htons)),
    ]
}
