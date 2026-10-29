// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Linux socket calls through rustix. `SO_MARK` goes through
//! socket2, which rustix 1.1 has no setter for; both are pure Rust and
//! neither needs an `unsafe` block here.

use std::io::{IoSlice, IoSliceMut};
use std::mem::MaybeUninit;
use std::net::{SocketAddrV4, SocketAddrV6};
use std::num::NonZeroU32;
use std::os::fd::{AsFd, AsRawFd, OwnedFd};

use rustix::event::{PollFd, PollFlags};
use rustix::net::{
    AddressFamily, Protocol, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags,
    SendAncillaryBuffer, SendAncillaryMessage, SendFlags, Shutdown, SocketAddrAny, SocketAddrUnix,
    SocketFlags, SocketType,
};
use rustix::event::Timespec;
use rustix::net::ipproto;

use super::{Info, Received};
use crate::consts::{
    AF_INET, AF_INET6, AF_UNIX, MSG_CTRUNC, MSG_DONTROUTE, MSG_EOR, MSG_OOB, MSG_PEEK, MSG_TRUNC,
    MSG_WAITALL, POLLERR, POLLHUP, POLLIN, POLLNVAL, POLLOUT, POLLPRI, POLLRDBAND, POLLRDNORM,
    POLLWRBAND, SHUT_RD, SHUT_RDWR, SHUT_WR, SOCK_DGRAM, SOCK_RAW, SOCK_SEQPACKET, SOCK_STREAM,
};
use crate::{Errno, SockAddr};

/// An owned kernel descriptor.
#[derive(Debug)]
pub struct Fd(pub(crate) OwnedFd);

impl From<OwnedFd> for Fd {
    fn from(fd: OwnedFd) -> Self {
        Self(fd)
    }
}

impl Fd {
    /// The kernel descriptor, for a relay that passes it on.
    #[must_use]
    pub fn into_owned(self) -> OwnedFd {
        self.0
    }
}

/// A second kernel descriptor for the same open socket (close-on-exec).
pub fn dup(fd: &Fd) -> Result<Fd, Errno> {
    fd.0.try_clone()
        .map(Fd)
        .map_err(|e| ifs::errno::linux::from_linux(e.raw_os_error().unwrap_or(0)))
}

/// The kernel number, the registry's key.
#[must_use]
pub fn key(fd: &Fd) -> i32 {
    fd.0.as_raw_fd()
}

/// The IBM `errno` of a Linux one.
pub(crate) fn errno(e: rustix::io::Errno) -> Errno {
    ifs::errno::linux::from_linux(e.raw_os_error())
}

fn family(domain: i32) -> Result<AddressFamily, Errno> {
    match domain {
        AF_UNIX => Ok(AddressFamily::UNIX),
        AF_INET => Ok(AddressFamily::INET),
        AF_INET6 => Ok(AddressFamily::INET6),
        _ => Err(Errno::EAFNOSUPPORT),
    }
}

fn socktype(ty: i32) -> Result<SocketType, Errno> {
    match ty {
        SOCK_STREAM => Ok(SocketType::STREAM),
        SOCK_DGRAM => Ok(SocketType::DGRAM),
        SOCK_RAW => Ok(SocketType::RAW),
        SOCK_SEQPACKET => Ok(SocketType::SEQPACKET),
        _ => Err(Errno::ESOCKTNOSUPPORT),
    }
}

fn protocol(proto: i32) -> Result<Option<Protocol>, Errno> {
    match u32::try_from(proto).ok().and_then(NonZeroU32::new) {
        None if proto == 0 => Ok(None),
        None => Err(Errno::EPROTONOSUPPORT),
        Some(n) => Ok(Some(Protocol::from_raw(n))),
    }
}

/// `socket()`: the descriptor is close-on-exec in the process; the
/// job's own `FD_CLOEXEC` is the table's.
pub fn socket(domain: i32, ty: i32, proto: i32) -> Result<Fd, Errno> {
    let fd = rustix::net::socket_with(family(domain)?, socktype(ty)?, SocketFlags::CLOEXEC, protocol(proto)?)
        .map_err(errno)?;
    Ok(Fd(fd))
}

/// `socketpair()`.
pub fn socketpair(domain: i32, ty: i32, proto: i32) -> Result<(Fd, Fd), Errno> {
    let (a, b) = rustix::net::socketpair(family(domain)?, socktype(ty)?, SocketFlags::CLOEXEC, protocol(proto)?)
        .map_err(errno)?;
    Ok((Fd(a), Fd(b)))
}

/// IBM's number for a socket type.
pub(crate) fn socktype_num(ty: SocketType) -> i32 {
    if ty == SocketType::STREAM {
        SOCK_STREAM
    } else if ty == SocketType::DGRAM {
        SOCK_DGRAM
    } else if ty == SocketType::RAW {
        SOCK_RAW
    } else if ty == SocketType::SEQPACKET {
        SOCK_SEQPACKET
    } else {
        0
    }
}

/// Family, type and protocol of a socket.
pub fn info(fd: &Fd) -> Result<Info, Errno> {
    use rustix::net::sockopt;
    let fam = sockopt::socket_domain(&fd.0).map_err(errno)?;
    let family = if fam == AddressFamily::INET {
        AF_INET
    } else if fam == AddressFamily::INET6 {
        AF_INET6
    } else if fam == AddressFamily::UNIX {
        AF_UNIX
    } else {
        0
    };
    let socktype = socktype_num(sockopt::socket_type(&fd.0).map_err(errno)?);
    let protocol = match sockopt::socket_protocol(&fd.0).map_err(errno)? {
        Some(p) if p == ipproto::TCP => 6,
        Some(p) if p == ipproto::UDP => 17,
        Some(p) if p == ipproto::ICMP => 1,
        Some(p) if p == ipproto::ICMPV6 => 58,
        Some(p) if p == ipproto::RAW => 255,
        _ => 0,
    };
    Ok(Info {
        family,
        socktype,
        protocol,
    })
}

/// `bind()`; a Unix path is a host path already.
pub fn bind(fd: &Fd, a: &SockAddr) -> Result<(), Errno> {
    match a {
        SockAddr::V4(v) => rustix::net::bind(&fd.0, v),
        SockAddr::V6(v) => rustix::net::bind(&fd.0, v),
        SockAddr::Unix(p) => rustix::net::bind(&fd.0, &unix(p)?),
        SockAddr::Unspec => return Err(Errno::EAFNOSUPPORT),
    }
    .map_err(errno)
}

fn unix(p: &str) -> Result<SocketAddrUnix, Errno> {
    SocketAddrUnix::new(p).map_err(|_| Errno::ENAMETOOLONG)
}

/// `listen()`.
pub fn listen(fd: &Fd, backlog: i32) -> Result<(), Errno> {
    rustix::net::listen(&fd.0, backlog).map_err(errno)
}

fn from_any(a: SocketAddrAny) -> Option<SockAddr> {
    if let Ok(v) = SocketAddrV4::try_from(a.clone()) {
        return Some(SockAddr::V4(v));
    }
    if let Ok(v) = SocketAddrV6::try_from(a.clone()) {
        return Some(SockAddr::V6(v));
    }
    if let Ok(u) = SocketAddrUnix::try_from(a) {
        let path = u.path().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        return Some(SockAddr::Unix(path));
    }
    None
}

/// `accept()` with the peer's address.
pub fn accept(fd: &Fd) -> Result<(Fd, Option<SockAddr>), Errno> {
    let (new, addr) = rustix::net::acceptfrom_with(&fd.0, SocketFlags::CLOEXEC).map_err(errno)?;
    Ok((Fd(new), addr.and_then(from_any)))
}

/// `connect()`; `AF_UNSPEC` dissolves a datagram association.
pub fn connect(fd: &Fd, a: &SockAddr) -> Result<(), Errno> {
    match a {
        SockAddr::V4(v) => rustix::net::connect(&fd.0, v),
        SockAddr::V6(v) => rustix::net::connect(&fd.0, v),
        SockAddr::Unix(p) => rustix::net::connect(&fd.0, &unix(p)?),
        SockAddr::Unspec => rustix::net::connect_unspec(&fd.0),
    }
    .map_err(errno)
}

fn send_flags(flags: i32) -> SendFlags {
    let mut f = SendFlags::NOSIGNAL;
    if flags & MSG_DONTROUTE != 0 {
        f |= SendFlags::DONTROUTE;
    }
    if flags & MSG_OOB != 0 {
        f |= SendFlags::OOB;
    }
    if flags & MSG_EOR != 0 {
        f |= SendFlags::EOR;
    }
    f
}

fn recv_flags(flags: i32) -> RecvFlags {
    let mut f = RecvFlags::empty();
    if flags & MSG_OOB != 0 {
        f |= RecvFlags::OOB;
    }
    if flags & MSG_PEEK != 0 {
        f |= RecvFlags::PEEK;
    }
    if flags & MSG_WAITALL != 0 {
        f |= RecvFlags::WAITALL;
    }
    if flags & MSG_TRUNC != 0 {
        f |= RecvFlags::TRUNC;
    }
    f
}

fn return_flags(f: ReturnFlags) -> i32 {
    let mut r = 0;
    if f.contains(ReturnFlags::OOB) {
        r |= MSG_OOB;
    }
    if f.contains(ReturnFlags::EOR) {
        r |= MSG_EOR;
    }
    if f.contains(ReturnFlags::TRUNC) {
        r |= MSG_TRUNC;
    }
    if f.contains(ReturnFlags::CTRUNC) {
        r |= MSG_CTRUNC;
    }
    r
}

/// `send()`.
pub fn send(fd: &Fd, buf: &[u8], flags: i32) -> Result<usize, Errno> {
    rustix::net::send(&fd.0, buf, send_flags(flags)).map_err(errno)
}

/// `sendto()`.
pub fn sendto(fd: &Fd, buf: &[u8], flags: i32, to: &SockAddr) -> Result<usize, Errno> {
    let f = send_flags(flags);
    match to {
        SockAddr::V4(v) => rustix::net::sendto(&fd.0, buf, f, v),
        SockAddr::V6(v) => rustix::net::sendto(&fd.0, buf, f, v),
        SockAddr::Unix(p) => rustix::net::sendto(&fd.0, buf, f, &unix(p)?),
        SockAddr::Unspec => return Err(Errno::EDESTADDRREQ),
    }
    .map_err(errno)
}

/// `sendmsg()` and `writev()`: several buffers, to an address or not.
pub fn sendv(fd: &Fd, bufs: &[&[u8]], flags: i32, to: Option<&SockAddr>) -> Result<usize, Errno> {
    let iov: Vec<IoSlice<'_>> = bufs.iter().map(|b| IoSlice::new(b)).collect();
    let mut control = SendAncillaryBuffer::new(&mut []);
    let f = send_flags(flags);
    match to {
        None | Some(SockAddr::Unspec) => rustix::net::sendmsg(&fd.0, &iov, &mut control, f),
        Some(SockAddr::V4(v)) => rustix::net::sendmsg_addr(&fd.0, v, &iov, &mut control, f),
        Some(SockAddr::V6(v)) => rustix::net::sendmsg_addr(&fd.0, v, &iov, &mut control, f),
        Some(SockAddr::Unix(p)) => rustix::net::sendmsg_addr(&fd.0, &unix(p)?, &iov, &mut control, f),
    }
    .map_err(errno)
}

/// `recv()`.
pub fn recv(fd: &Fd, buf: &mut [u8], flags: i32) -> Result<usize, Errno> {
    let (n, _) = rustix::net::recv(&fd.0, buf, recv_flags(flags)).map_err(errno)?;
    Ok(n)
}

/// `recvfrom()`.
pub fn recvfrom(fd: &Fd, buf: &mut [u8], flags: i32) -> Result<(usize, Option<SockAddr>), Errno> {
    let (n, _, from) = rustix::net::recvfrom(&fd.0, buf, recv_flags(flags)).map_err(errno)?;
    Ok((n, from.and_then(from_any)))
}

/// `recvmsg()` and `readv()`: several buffers.
pub fn recvv(fd: &Fd, bufs: &mut [&mut [u8]], flags: i32) -> Result<Received, Errno> {
    let mut iov: Vec<IoSliceMut<'_>> = bufs.iter_mut().map(|b| IoSliceMut::new(b)).collect();
    let mut control = RecvAncillaryBuffer::new(&mut []);
    let r = rustix::net::recvmsg(&fd.0, &mut iov, &mut control, recv_flags(flags)).map_err(errno)?;
    Ok(Received {
        bytes: r.bytes,
        from: r.address.and_then(from_any),
        flags: return_flags(r.flags),
    })
}

/// `read()`.
pub fn read(fd: &Fd, buf: &mut [u8]) -> Result<usize, Errno> {
    rustix::io::read(&fd.0, buf).map_err(errno)
}

/// `write()`.
pub fn write(fd: &Fd, buf: &[u8]) -> Result<usize, Errno> {
    rustix::io::write(&fd.0, buf).map_err(errno)
}

/// `shutdown()`.
pub fn shutdown(fd: &Fd, how: i32) -> Result<(), Errno> {
    let h = match how {
        SHUT_RD => Shutdown::Read,
        SHUT_WR => Shutdown::Write,
        SHUT_RDWR => Shutdown::Both,
        _ => return Err(Errno::EINVAL),
    };
    rustix::net::shutdown(&fd.0, h).map_err(errno)
}

/// `getsockname()`.
pub fn sockname(fd: &Fd) -> Result<SockAddr, Errno> {
    let a = rustix::net::getsockname(&fd.0).map_err(errno)?;
    from_any(a).ok_or(Errno::EAFNOSUPPORT)
}

/// `getpeername()`.
pub fn peername(fd: &Fd) -> Result<SockAddr, Errno> {
    let a = rustix::net::getpeername(&fd.0).map_err(errno)?.ok_or(Errno::ENOTCONN)?;
    from_any(a).ok_or(Errno::EAFNOSUPPORT)
}

/// `FIONBIO`.
pub fn set_nonblock(fd: &Fd, on: bool) -> Result<(), Errno> {
    rustix::io::ioctl_fionbio(&fd.0, on).map_err(errno)
}

/// `FIONREAD`.
pub fn pending(fd: &Fd) -> Result<usize, Errno> {
    let n = rustix::io::ioctl_fionread(&fd.0).map_err(errno)?;
    Ok(usize::try_from(n).unwrap_or(usize::MAX))
}

fn poll_flags(events: i16) -> PollFlags {
    let mut f = PollFlags::empty();
    let pairs = [
        (POLLIN, PollFlags::IN),
        (POLLOUT, PollFlags::OUT),
        (POLLPRI, PollFlags::PRI),
        (POLLRDNORM, PollFlags::RDNORM),
        (POLLRDBAND, PollFlags::RDBAND),
        (POLLWRBAND, PollFlags::WRBAND),
    ];
    for (ibm, lx) in pairs {
        if events & ibm != 0 {
            f |= lx;
        }
    }
    f
}

fn revents(f: PollFlags) -> i16 {
    let mut r = 0i16;
    let pairs = [
        (PollFlags::IN, POLLIN),
        (PollFlags::OUT, POLLOUT),
        (PollFlags::PRI, POLLPRI),
        (PollFlags::RDNORM, POLLRDNORM),
        (PollFlags::RDBAND, POLLRDBAND),
        (PollFlags::WRBAND, POLLWRBAND),
        (PollFlags::HUP, POLLHUP),
        (PollFlags::ERR, POLLERR),
        (PollFlags::NVAL, POLLNVAL),
    ];
    for (lx, ibm) in pairs {
        if f.contains(lx) {
            r |= ibm;
        }
    }
    r
}

/// `poll()`: the returned events of each item, in order. A negative
/// timeout waits for ever.
pub fn poll(items: &[(&Fd, i16)], timeout_ms: i32) -> Result<Vec<i16>, Errno> {
    let mut fds: Vec<PollFd<'_>> = items
        .iter()
        .map(|(fd, ev)| PollFd::new(&fd.0, poll_flags(*ev)))
        .collect();
    let ts = (timeout_ms >= 0).then(|| Timespec {
        tv_sec: i64::from(timeout_ms / 1000),
        tv_nsec: i64::from(timeout_ms % 1000) * 1_000_000,
    });
    rustix::event::poll(&mut fds, ts.as_ref()).map_err(errno)?;
    Ok(fds.iter().map(|f| revents(f.revents())).collect())
}

/// `SO_MARK`, through socket2 (needs `CAP_NET_ADMIN`).
pub fn set_mark(fd: &Fd, mark: u32) -> Result<(), Errno> {
    socket2::SockRef::from(&fd.0)
        .set_mark(mark)
        .map_err(|e| ifs::errno::linux::from_linux(e.raw_os_error().unwrap_or(0)))
}

/// `getsockopt()`: the value in IBM's layout.
pub fn getopt(fd: &Fd, level: i32, name: i32) -> Result<Vec<u8>, Errno> {
    super::opts::get(&fd.0, level, name)
}

/// `setsockopt()`: the value in IBM's layout.
pub fn setopt(fd: &Fd, level: i32, name: i32, value: &[u8]) -> Result<(), Errno> {
    super::opts::set(&fd.0, level, name, value)
}

/// Pass a descriptor over a Unix socket (`SCM_RIGHTS`) with one byte of
/// data, as the descriptor relay does.
pub fn send_fd(sock: &Fd, fd: &Fd) -> Result<(), Errno> {
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    let fds = [fd.0.as_fd()];
    control.push(SendAncillaryMessage::ScmRights(&fds));
    rustix::net::sendmsg(&sock.0, &[IoSlice::new(&[1u8])], &mut control, SendFlags::NOSIGNAL)
        .map_err(errno)?;
    Ok(())
}

/// Receive a descriptor passed with [`send_fd`].
pub fn recv_fd(sock: &Fd) -> Result<Fd, Errno> {
    let mut space = [MaybeUninit::<u8>::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = RecvAncillaryBuffer::new(&mut space);
    let mut byte = [0u8; 1];
    rustix::net::recvmsg(
        &sock.0,
        &mut [IoSliceMut::new(&mut byte)],
        &mut control,
        RecvFlags::CMSG_CLOEXEC,
    )
    .map_err(errno)?;
    for m in control.drain() {
        if let RecvAncillaryMessage::ScmRights(mut fds) = m
            && let Some(fd) = fds.next()
        {
            return Ok(Fd(fd));
        }
    }
    Err(Errno::ENOENT)
}
