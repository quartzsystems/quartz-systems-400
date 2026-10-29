// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Off Linux there are no QS/400 sockets: every call is `ENOSYS` and no
//! descriptor can exist, so the codecs, the tables and the resolver's
//! host-table path can still be tested on the build host.

use super::{Info, Received};
use crate::{Errno, SockAddr};

/// Never constructed.
#[derive(Debug)]
pub enum Fd {}

/// Unreachable.
#[must_use]
pub fn key(fd: &Fd) -> i32 {
    match *fd {}
}

/// Unreachable.
pub fn dup(fd: &Fd) -> Result<Fd, Errno> {
    match *fd {}
}

/// `ENOSYS`.
pub fn socket(_domain: i32, _ty: i32, _proto: i32) -> Result<Fd, Errno> {
    Err(Errno::ENOSYS)
}

/// `ENOSYS`.
pub fn socketpair(_domain: i32, _ty: i32, _proto: i32) -> Result<(Fd, Fd), Errno> {
    Err(Errno::ENOSYS)
}

/// Unreachable.
pub fn info(fd: &Fd) -> Result<Info, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn bind(fd: &Fd, _a: &SockAddr) -> Result<(), Errno> {
    match *fd {}
}

/// Unreachable.
pub fn listen(fd: &Fd, _backlog: i32) -> Result<(), Errno> {
    match *fd {}
}

/// Unreachable.
pub fn accept(fd: &Fd) -> Result<(Fd, Option<SockAddr>), Errno> {
    match *fd {}
}

/// Unreachable.
pub fn connect(fd: &Fd, _a: &SockAddr) -> Result<(), Errno> {
    match *fd {}
}

/// Unreachable.
pub fn send(fd: &Fd, _buf: &[u8], _flags: i32) -> Result<usize, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn sendto(fd: &Fd, _buf: &[u8], _flags: i32, _to: &SockAddr) -> Result<usize, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn sendv(fd: &Fd, _bufs: &[&[u8]], _flags: i32, _to: Option<&SockAddr>) -> Result<usize, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn recv(fd: &Fd, _buf: &mut [u8], _flags: i32) -> Result<usize, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn recvfrom(fd: &Fd, _buf: &mut [u8], _flags: i32) -> Result<(usize, Option<SockAddr>), Errno> {
    match *fd {}
}

/// Unreachable.
pub fn recvv(fd: &Fd, _bufs: &mut [&mut [u8]], _flags: i32) -> Result<Received, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn read(fd: &Fd, _buf: &mut [u8]) -> Result<usize, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn write(fd: &Fd, _buf: &[u8]) -> Result<usize, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn shutdown(fd: &Fd, _how: i32) -> Result<(), Errno> {
    match *fd {}
}

/// Unreachable.
pub fn sockname(fd: &Fd) -> Result<SockAddr, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn peername(fd: &Fd) -> Result<SockAddr, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn set_nonblock(fd: &Fd, _on: bool) -> Result<(), Errno> {
    match *fd {}
}

/// Unreachable.
pub fn pending(fd: &Fd) -> Result<usize, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn poll(items: &[(&Fd, i16)], _timeout_ms: i32) -> Result<Vec<i16>, Errno> {
    match items.first() {
        Some((fd, _)) => match **fd {},
        None => Ok(Vec::new()),
    }
}

/// Unreachable.
pub fn set_mark(fd: &Fd, _mark: u32) -> Result<(), Errno> {
    match *fd {}
}

/// Unreachable.
pub fn getopt(fd: &Fd, _level: i32, _name: i32) -> Result<Vec<u8>, Errno> {
    match *fd {}
}

/// Unreachable.
pub fn setopt(fd: &Fd, _level: i32, _name: i32, _value: &[u8]) -> Result<(), Errno> {
    match *fd {}
}

/// Unreachable.
pub fn send_fd(sock: &Fd, _fd: &Fd) -> Result<(), Errno> {
    match *sock {}
}

/// Unreachable.
pub fn recv_fd(sock: &Fd) -> Result<Fd, Errno> {
    match *sock {}
}
