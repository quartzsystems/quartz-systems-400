// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Socket options: IBM's level and option numbers and value layouts
//! (an `int`, a `linger`, a `timeval`, an `in_addr`, an `ip_mreq`),
//! each mapped to the rustix call that sets or reads it. An option
//! IBM names but this layer cannot reach is `ENOPROTOOPT`, as a kernel
//! without it would answer.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::os::fd::OwnedFd;
use std::time::Duration;

use rustix::net::sockopt::{self, Timeout};

use super::linux::errno;
use crate::consts::{
    IP_ADD_MEMBERSHIP, IP_DROP_MEMBERSHIP, IP_MULTICAST_IF, IP_MULTICAST_LOOP, IP_MULTICAST_TTL,
    IP_TOS, IP_TTL, IPPROTO_IP, IPPROTO_IPV6, IPPROTO_TCP, IPV6_JOIN_GROUP, IPV6_LEAVE_GROUP,
    IPV6_MULTICAST_HOPS, IPV6_MULTICAST_IF, IPV6_MULTICAST_LOOP, IPV6_UNICAST_HOPS, IPV6_V6ONLY,
    SO_ACCEPTCONN, SO_BROADCAST, SO_DEBUG, SO_ERROR, SO_KEEPALIVE, SO_LINGER, SO_OOBINLINE,
    SO_RCVBUF, SO_RCVTIMEO, SO_REUSEADDR, SO_SNDBUF, SO_SNDTIMEO, SO_TYPE, SO_USELOOPBACK,
    SOL_SOCKET, TCP_KEEPALIVE, TCP_NODELAY,
};
use crate::layout::misc::{linger, put_linger, put_timeval, timeval};
use crate::layout::{i32_at, put_i32};
use crate::Errno;

type R<T> = Result<T, Errno>;

fn int(v: &[u8]) -> R<i32> {
    if v.len() < 4 {
        return Err(Errno::EINVAL);
    }
    Ok(i32_at(v, 0))
}

fn flag(v: &[u8]) -> R<bool> {
    Ok(int(v)? != 0)
}

fn int_bytes(v: i32) -> Vec<u8> {
    let mut b = vec![0u8; 4];
    put_i32(&mut b, 0, v);
    b
}

fn bool_bytes(v: bool) -> Vec<u8> {
    int_bytes(i32::from(v))
}

fn v4(v: &[u8], off: usize) -> R<Ipv4Addr> {
    let b = v.get(off..off + 4).ok_or(Errno::EINVAL)?;
    Ok(Ipv4Addr::new(b[0], b[1], b[2], b[3]))
}

fn v6(v: &[u8], off: usize) -> R<Ipv6Addr> {
    let b: [u8; 16] = v
        .get(off..off + 16)
        .ok_or(Errno::EINVAL)?
        .try_into()
        .expect("16 bytes");
    Ok(Ipv6Addr::from(b))
}

fn secs(v: &[u8]) -> R<Duration> {
    let n = int(v)?;
    Ok(Duration::from_secs(u64::try_from(n).map_err(|_| Errno::EINVAL)?))
}

fn timeout(v: &[u8]) -> R<Option<Duration>> {
    let d = timeval(v).ok_or(Errno::EINVAL)?;
    Ok((!d.is_zero()).then_some(d))
}

fn timeout_bytes(d: Option<Duration>) -> Vec<u8> {
    let mut b = vec![0u8; 8];
    put_timeval(&mut b, d.unwrap_or_default());
    b
}

/// `setsockopt()`.
pub fn set(fd: &OwnedFd, level: i32, name: i32, v: &[u8]) -> R<()> {
    let r = match (level, name) {
        (SOL_SOCKET, SO_BROADCAST) => sockopt::set_socket_broadcast(fd, flag(v)?),
        (SOL_SOCKET, SO_KEEPALIVE) => sockopt::set_socket_keepalive(fd, flag(v)?),
        (SOL_SOCKET, SO_REUSEADDR) => sockopt::set_socket_reuseaddr(fd, flag(v)?),
        (SOL_SOCKET, SO_OOBINLINE) => sockopt::set_socket_oobinline(fd, flag(v)?),
        (SOL_SOCKET, SO_LINGER) => {
            if v.len() < 8 {
                return Err(Errno::EINVAL);
            }
            let (on, s) = linger(v);
            let d = on.then(|| Duration::from_secs(u64::try_from(s).unwrap_or(0)));
            sockopt::set_socket_linger(fd, d)
        }
        (SOL_SOCKET, SO_RCVBUF) => sockopt::set_socket_recv_buffer_size(fd, usize::try_from(int(v)?).map_err(|_| Errno::EINVAL)?),
        (SOL_SOCKET, SO_SNDBUF) => sockopt::set_socket_send_buffer_size(fd, usize::try_from(int(v)?).map_err(|_| Errno::EINVAL)?),
        (SOL_SOCKET, SO_RCVTIMEO) => sockopt::set_socket_timeout(fd, Timeout::Recv, timeout(v)?),
        (SOL_SOCKET, SO_SNDTIMEO) => sockopt::set_socket_timeout(fd, Timeout::Send, timeout(v)?),
        // Accepted and ignored: the kernel has no equivalent.
        (SOL_SOCKET, SO_DEBUG | SO_USELOOPBACK) => {
            int(v)?;
            Ok(())
        }
        (IPPROTO_TCP, TCP_NODELAY) => sockopt::set_tcp_nodelay(fd, flag(v)?),
        (IPPROTO_TCP, TCP_KEEPALIVE) => sockopt::set_tcp_keepidle(fd, secs(v)?),
        (IPPROTO_IP, IP_TOS) => sockopt::set_ip_tos(fd, u8::try_from(int(v)?).map_err(|_| Errno::EINVAL)?),
        (IPPROTO_IP, IP_TTL) => sockopt::set_ip_ttl(fd, u32::try_from(int(v)?).map_err(|_| Errno::EINVAL)?),
        (IPPROTO_IP, IP_MULTICAST_IF) => sockopt::set_ip_multicast_if(fd, &v4(v, 0)?),
        (IPPROTO_IP, IP_MULTICAST_TTL) => sockopt::set_ip_multicast_ttl(fd, u32::try_from(int(v)?).map_err(|_| Errno::EINVAL)?),
        (IPPROTO_IP, IP_MULTICAST_LOOP) => sockopt::set_ip_multicast_loop(fd, flag(v)?),
        (IPPROTO_IP, IP_ADD_MEMBERSHIP) => sockopt::set_ip_add_membership(fd, &v4(v, 0)?, &v4(v, 4)?),
        (IPPROTO_IP, IP_DROP_MEMBERSHIP) => sockopt::set_ip_drop_membership(fd, &v4(v, 0)?, &v4(v, 4)?),
        (IPPROTO_IPV6, IPV6_V6ONLY) => sockopt::set_ipv6_v6only(fd, flag(v)?),
        (IPPROTO_IPV6, IPV6_UNICAST_HOPS) => sockopt::set_ipv6_unicast_hops(fd, u8::try_from(int(v)?).ok()),
        (IPPROTO_IPV6, IPV6_MULTICAST_HOPS) => sockopt::set_ipv6_multicast_hops(fd, u32::try_from(int(v)?).map_err(|_| Errno::EINVAL)?),
        (IPPROTO_IPV6, IPV6_MULTICAST_LOOP) => sockopt::set_ipv6_multicast_loop(fd, flag(v)?),
        (IPPROTO_IPV6, IPV6_MULTICAST_IF) => sockopt::set_ipv6_multicast_if(fd, u32::try_from(int(v)?).map_err(|_| Errno::EINVAL)?),
        (IPPROTO_IPV6, IPV6_JOIN_GROUP) => sockopt::set_ipv6_add_membership(fd, &v6(v, 0)?, u32::try_from(i32_at(v, 16)).unwrap_or(0)),
        (IPPROTO_IPV6, IPV6_LEAVE_GROUP) => sockopt::set_ipv6_drop_membership(fd, &v6(v, 0)?, u32::try_from(i32_at(v, 16)).unwrap_or(0)),
        _ => return Err(Errno::ENOPROTOOPT),
    };
    r.map_err(errno)
}

/// `getsockopt()`.
pub fn get(fd: &OwnedFd, level: i32, name: i32) -> R<Vec<u8>> {
    let r = match (level, name) {
        (SOL_SOCKET, SO_BROADCAST) => sockopt::socket_broadcast(fd).map(bool_bytes),
        (SOL_SOCKET, SO_KEEPALIVE) => sockopt::socket_keepalive(fd).map(bool_bytes),
        (SOL_SOCKET, SO_REUSEADDR) => sockopt::socket_reuseaddr(fd).map(bool_bytes),
        (SOL_SOCKET, SO_OOBINLINE) => sockopt::socket_oobinline(fd).map(bool_bytes),
        (SOL_SOCKET, SO_ACCEPTCONN) => sockopt::socket_acceptconn(fd).map(bool_bytes),
        (SOL_SOCKET, SO_DEBUG | SO_USELOOPBACK) => Ok(int_bytes(0)),
        (SOL_SOCKET, SO_ERROR) => sockopt::socket_error(fd).map(|r| int_bytes(r.err().map_or(0, |e| errno(e).code()))),
        (SOL_SOCKET, SO_LINGER) => sockopt::socket_linger(fd).map(|d| {
            let mut b = vec![0u8; 8];
            put_linger(&mut b, d.is_some(), i32::try_from(d.map_or(0, |d| d.as_secs())).unwrap_or(i32::MAX));
            b
        }),
        (SOL_SOCKET, SO_RCVBUF) => sockopt::socket_recv_buffer_size(fd).map(|n| int_bytes(i32::try_from(n).unwrap_or(i32::MAX))),
        (SOL_SOCKET, SO_SNDBUF) => sockopt::socket_send_buffer_size(fd).map(|n| int_bytes(i32::try_from(n).unwrap_or(i32::MAX))),
        (SOL_SOCKET, SO_RCVTIMEO) => sockopt::socket_timeout(fd, Timeout::Recv).map(timeout_bytes),
        (SOL_SOCKET, SO_SNDTIMEO) => sockopt::socket_timeout(fd, Timeout::Send).map(timeout_bytes),
        (SOL_SOCKET, SO_TYPE) => sockopt::socket_type(fd).map(|t| int_bytes(super::linux::socktype_num(t))),
        (IPPROTO_TCP, TCP_NODELAY) => sockopt::tcp_nodelay(fd).map(bool_bytes),
        (IPPROTO_TCP, TCP_KEEPALIVE) => sockopt::tcp_keepidle(fd).map(|d| int_bytes(i32::try_from(d.as_secs()).unwrap_or(i32::MAX))),
        (IPPROTO_IP, IP_TOS) => sockopt::ip_tos(fd).map(|t| int_bytes(i32::from(t))),
        (IPPROTO_IP, IP_TTL) => sockopt::ip_ttl(fd).map(|t| int_bytes(i32::try_from(t).unwrap_or(0))),
        (IPPROTO_IP, IP_MULTICAST_IF) => sockopt::ip_multicast_if(fd).map(|a| a.octets().to_vec()),
        (IPPROTO_IP, IP_MULTICAST_TTL) => sockopt::ip_multicast_ttl(fd).map(|t| int_bytes(i32::try_from(t).unwrap_or(0))),
        (IPPROTO_IP, IP_MULTICAST_LOOP) => sockopt::ip_multicast_loop(fd).map(bool_bytes),
        (IPPROTO_IPV6, IPV6_V6ONLY) => sockopt::ipv6_v6only(fd).map(bool_bytes),
        (IPPROTO_IPV6, IPV6_UNICAST_HOPS) => sockopt::ipv6_unicast_hops(fd).map(|h| int_bytes(i32::from(h))),
        (IPPROTO_IPV6, IPV6_MULTICAST_HOPS) => sockopt::ipv6_multicast_hops(fd).map(|h| int_bytes(i32::try_from(h).unwrap_or(0))),
        (IPPROTO_IPV6, IPV6_MULTICAST_LOOP) => sockopt::ipv6_multicast_loop(fd).map(bool_bytes),
        (IPPROTO_IPV6, IPV6_MULTICAST_IF) => sockopt::ipv6_multicast_if(fd).map(|i| int_bytes(i32::try_from(i).unwrap_or(0))),
        _ => return Err(Errno::ENOPROTOOPT),
    };
    r.map_err(errno)
}
