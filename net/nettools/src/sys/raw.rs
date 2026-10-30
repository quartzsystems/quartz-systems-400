// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The socket calls the tools need that the safe crates do not offer:
//! `setsockopt` of an integer (`IP_RECVERR`, `IP_RECVTTL`, `IP_TTL`),
//! `recvmsg` with a control buffer (the error queue), and `AF_PACKET`
//! bound to one link with the `sockaddr_ll` of each frame. Each is one
//! libc call on memory this module owns; this is the crate's only
//! `unsafe`.

#![allow(unsafe_code)]

use std::io;
use std::mem::{size_of, zeroed};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

fn check(r: libc::c_int) -> io::Result<libc::c_int> {
    if r < 0 { Err(io::Error::last_os_error()) } else { Ok(r) }
}

fn check_len(r: libc::ssize_t) -> io::Result<usize> {
    usize::try_from(r).map_err(|_| io::Error::last_os_error())
}

fn len_of<T>() -> libc::socklen_t {
    libc::socklen_t::try_from(size_of::<T>()).unwrap_or(libc::socklen_t::MAX)
}

/// A new close-on-exec socket.
pub fn socket(domain: i32, ty: i32, proto: i32) -> io::Result<OwnedFd> {
    // SAFETY: `socket` takes plain integers and returns a new descriptor
    // or -1; the descriptor is owned by the `OwnedFd` from here on.
    let fd = check(unsafe { libc::socket(domain, ty | libc::SOCK_CLOEXEC, proto) })?;
    // SAFETY: `fd` was just returned by `socket` and nothing else owns it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// `setsockopt` of an `int`.
pub fn set_int(fd: &OwnedFd, level: i32, name: i32, value: i32) -> io::Result<()> {
    // SAFETY: the option value is a local `int` and its size is passed
    // with it; the descriptor is open for the call (borrowed).
    check(unsafe {
        libc::setsockopt(
            fd.as_raw_fd(),
            level,
            name,
            std::ptr::from_ref(&value).cast(),
            len_of::<libc::c_int>(),
        )
    })?;
    Ok(())
}

/// A `sockaddr_in` or `sockaddr_in6` and its length.
fn sockaddr(a: &SocketAddr) -> (libc::sockaddr_storage, libc::socklen_t) {
    // SAFETY: `sockaddr_storage` is plain bytes; all zeros is valid.
    let mut ss: libc::sockaddr_storage = unsafe { zeroed() };
    match a {
        SocketAddr::V4(v4) => {
            // SAFETY: `sockaddr_storage` is large enough and suitably
            // aligned for a `sockaddr_in`; the reference ends here.
            let sin = unsafe { &mut *std::ptr::from_mut(&mut ss).cast::<libc::sockaddr_in>() };
            sin.sin_family = libc::sa_family_t::try_from(libc::AF_INET).unwrap_or(2);
            sin.sin_port = v4.port().to_be();
            sin.sin_addr.s_addr = u32::from_ne_bytes(v4.ip().octets());
            (ss, len_of::<libc::sockaddr_in>())
        }
        SocketAddr::V6(v6) => {
            // SAFETY: as above, for a `sockaddr_in6`.
            let sin6 = unsafe { &mut *std::ptr::from_mut(&mut ss).cast::<libc::sockaddr_in6>() };
            sin6.sin6_family = libc::sa_family_t::try_from(libc::AF_INET6).unwrap_or(10);
            sin6.sin6_port = v6.port().to_be();
            sin6.sin6_addr.s6_addr = v6.ip().octets();
            sin6.sin6_scope_id = v6.scope_id();
            (ss, len_of::<libc::sockaddr_in6>())
        }
    }
}

/// The address in a `sockaddr_storage` the kernel filled.
fn from_storage(ss: &libc::sockaddr_storage) -> Option<IpAddr> {
    match i32::from(ss.ss_family) {
        libc::AF_INET => {
            // SAFETY: the family says the storage holds a `sockaddr_in`.
            let sin = unsafe { &*std::ptr::from_ref(ss).cast::<libc::sockaddr_in>() };
            Some(IpAddr::V4(Ipv4Addr::from(sin.sin_addr.s_addr.to_ne_bytes())))
        }
        libc::AF_INET6 => {
            // SAFETY: the family says the storage holds a `sockaddr_in6`.
            let sin6 = unsafe { &*std::ptr::from_ref(ss).cast::<libc::sockaddr_in6>() };
            Some(IpAddr::V6(Ipv6Addr::from(sin6.sin6_addr.s6_addr)))
        }
        _ => None,
    }
}

/// `bind` to an IP address.
pub fn bind(fd: &OwnedFd, a: &SocketAddr) -> io::Result<()> {
    let (ss, len) = sockaddr(a);
    // SAFETY: `ss` holds a valid address of `len` bytes for the call.
    check(unsafe { libc::bind(fd.as_raw_fd(), std::ptr::from_ref(&ss).cast(), len) })?;
    Ok(())
}

/// `connect` to an IP address.
pub fn connect(fd: &OwnedFd, a: &SocketAddr) -> io::Result<()> {
    let (ss, len) = sockaddr(a);
    // SAFETY: as `bind`.
    check(unsafe { libc::connect(fd.as_raw_fd(), std::ptr::from_ref(&ss).cast(), len) })?;
    Ok(())
}

/// `send` on a connected socket.
pub fn send(fd: &OwnedFd, buf: &[u8]) -> io::Result<usize> {
    // SAFETY: `buf` is valid for `buf.len()` bytes for the call.
    check_len(unsafe { libc::send(fd.as_raw_fd(), buf.as_ptr().cast(), buf.len(), libc::MSG_NOSIGNAL) })
}

/// `poll` one descriptor; the returned events, 0 on time-out.
pub fn poll(fd: &OwnedFd, events: i16, timeout_ms: i32) -> io::Result<i16> {
    let mut p = libc::pollfd { fd: fd.as_raw_fd(), events, revents: 0 };
    // SAFETY: one `pollfd` that lives across the call.
    let n = check(unsafe { libc::poll(&raw mut p, 1, timeout_ms) })?;
    Ok(if n == 0 { 0 } else { p.revents })
}

/// What `recvmsg` returned.
pub struct Received {
    /// Data bytes.
    pub len: usize,
    /// Control bytes.
    pub ctrl_len: usize,
    /// The sender.
    pub from: Option<IpAddr>,
}

/// `recvmsg` into `buf` and `ctrl` with `flags` (`MSG_ERRQUEUE`,
/// `MSG_DONTWAIT`).
pub fn recvmsg(fd: &OwnedFd, buf: &mut [u8], ctrl: &mut [u8], flags: i32) -> io::Result<Received> {
    // SAFETY: plain bytes; zeros are valid.
    let mut ss: libc::sockaddr_storage = unsafe { zeroed() };
    let mut iov = libc::iovec { iov_base: buf.as_mut_ptr().cast(), iov_len: buf.len() };
    // SAFETY: `msghdr` is plain data; zeros are valid before it is set.
    let mut m: libc::msghdr = unsafe { zeroed() };
    m.msg_name = std::ptr::from_mut(&mut ss).cast();
    m.msg_namelen = len_of::<libc::sockaddr_storage>();
    m.msg_iov = &raw mut iov;
    m.msg_iovlen = 1;
    m.msg_control = ctrl.as_mut_ptr().cast();
    // `socklen_t` on musl, `size_t` on glibc.
    m.msg_controllen = ctrl.len().try_into().unwrap_or(0);
    // SAFETY: every pointer in `m` points at memory borrowed mutably for
    // the call with its true length.
    let n = check_len(unsafe { libc::recvmsg(fd.as_raw_fd(), &raw mut m, flags) })?;
    Ok(Received {
        len: n,
        ctrl_len: usize::try_from(m.msg_controllen).unwrap_or(0).min(ctrl.len()),
        from: if m.msg_namelen > 0 { from_storage(&ss) } else { None },
    })
}

/// `ETH_P_ALL`.
const ETH_P_ALL: u16 = 0x0003;

/// An `AF_PACKET` raw socket on link `ifindex`, every protocol.
pub fn packet_socket(ifindex: i32) -> io::Result<OwnedFd> {
    let fd = socket(libc::AF_PACKET, libc::SOCK_RAW, i32::from(ETH_P_ALL.to_be()))?;
    // SAFETY: plain data; zeros are valid.
    let mut ll: libc::sockaddr_ll = unsafe { zeroed() };
    ll.sll_family = libc::c_ushort::try_from(libc::AF_PACKET).unwrap_or(17);
    ll.sll_protocol = ETH_P_ALL.to_be();
    ll.sll_ifindex = ifindex;
    // SAFETY: `ll` is a complete `sockaddr_ll` passed with its size.
    check(unsafe { libc::bind(fd.as_raw_fd(), std::ptr::from_ref(&ll).cast(), len_of::<libc::sockaddr_ll>()) })?;
    Ok(fd)
}

/// One frame from a packet socket: its length on the wire (it may be
/// longer than `buf`), and whether this host sent it.
pub fn packet_recv(fd: &OwnedFd, buf: &mut [u8]) -> io::Result<(usize, bool)> {
    // SAFETY: plain data; zeros are valid.
    let mut ll: libc::sockaddr_ll = unsafe { zeroed() };
    let mut len = len_of::<libc::sockaddr_ll>();
    // SAFETY: `buf` and `ll` are valid for the lengths given; `MSG_TRUNC`
    // makes the kernel report the frame's full length.
    let n = check_len(unsafe {
        libc::recvfrom(
            fd.as_raw_fd(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            libc::MSG_TRUNC,
            std::ptr::from_mut(&mut ll).cast(),
            &raw mut len,
        )
    })?;
    Ok((n, ll.sll_pkttype == libc::PACKET_OUTGOING))
}
