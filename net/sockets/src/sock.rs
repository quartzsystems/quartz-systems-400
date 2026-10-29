// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One job's sockets: the kernel descriptors it owns, keyed by kernel
//! number (the value a job descriptor table records), and the rules
//! that apply to every call. Arguments and results are IBM's numbers
//! and [`SockAddr`]s with IFS paths; the job's descriptor numbering is
//! the caller's.

// Off Linux a descriptor cannot exist, so the map holds nothing.
#![cfg_attr(not(target_os = "linux"), allow(clippy::zero_sized_map_values))]

use std::collections::HashMap;

use crate::consts::{AF_INET, AF_INET6, AF_UNIX, POLLNVAL};
use crate::pass::{DescriptorPassing, InProcess, JobId};
use crate::paths::{IfsPathMap, RootedPaths, UnixUse};
use crate::rules::{AllowAll, PortRules, protocol_of};
use crate::sys::{self, Fd, Info, Received};
use crate::{Errno, SockAddr};

type R<T> = Result<T, Errno>;

/// A job's sockets.
pub struct Sockets {
    open: HashMap<i32, Fd>,
    rules: Box<dyn PortRules>,
    paths: Box<dyn IfsPathMap>,
    passing: Box<dyn DescriptorPassing>,
    user: String,
    mark: Option<u32>,
}

impl std::fmt::Debug for Sockets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sockets")
            .field("open", &self.open.len())
            .field("user", &self.user)
            .field("mark", &self.mark)
            .finish_non_exhaustive()
    }
}

impl Default for Sockets {
    /// No restrictions, IFS paths as host paths, in-process passing,
    /// marking on.
    fn default() -> Self {
        Self::new(
            Box::new(AllowAll),
            Box::new(RootedPaths::default()),
            Box::new(InProcess::new([0; 16])),
        )
    }
}

impl Sockets {
    /// A job's sockets under these rules.
    #[must_use]
    pub fn new(
        rules: Box<dyn PortRules>,
        paths: Box<dyn IfsPathMap>,
        passing: Box<dyn DescriptorPassing>,
    ) -> Self {
        Self {
            open: HashMap::new(),
            rules,
            paths,
            passing,
            user: String::new(),
            mark: Some(netlink::nft::SOCKET_MARK),
        }
    }

    /// The profile port restrictions are checked for.
    pub fn set_user(&mut self, user: &str) {
        self.user = user.to_string();
    }

    /// The mark set on every bound socket; `None` turns marking off
    /// (a process without `CAP_NET_ADMIN`, where it would fail).
    pub fn set_mark(&mut self, mark: Option<u32>) {
        self.mark = mark;
    }

    /// Replace the port rules.
    pub fn set_rules(&mut self, rules: Box<dyn PortRules>) {
        self.rules = rules;
    }

    /// Replace the IFS path map.
    pub fn set_paths(&mut self, paths: Box<dyn IfsPathMap>) {
        self.paths = paths;
    }

    /// Replace the descriptor relay.
    pub fn set_passing(&mut self, passing: Box<dyn DescriptorPassing>) {
        self.passing = passing;
    }

    /// Kernel numbers open.
    #[must_use]
    pub fn count(&self) -> usize {
        self.open.len()
    }

    fn fd(&self, k: i32) -> R<&Fd> {
        self.open.get(&k).ok_or(Errno::EBADF)
    }

    fn keep(&mut self, fd: Fd) -> i32 {
        let k = sys::key(&fd);
        self.open.insert(k, fd);
        k
    }

    fn ifs_to_kernel(&self, a: &SockAddr, why: UnixUse) -> R<SockAddr> {
        match a {
            SockAddr::Unix(p) => Ok(SockAddr::Unix(self.paths.to_host(p, why)?)),
            a => Ok(a.clone()),
        }
    }

    fn kernel_to_ifs(&self, a: SockAddr) -> SockAddr {
        match a {
            SockAddr::Unix(p) if !p.is_empty() => SockAddr::Unix(self.paths.to_ifs(&p)),
            a => a,
        }
    }

    /// `socket()`: the kernel number.
    pub fn socket(&mut self, domain: i32, ty: i32, proto: i32) -> R<i32> {
        let fd = sys::socket(domain, ty, proto)?;
        Ok(self.keep(fd))
    }

    /// `socketpair()`.
    pub fn socketpair(&mut self, domain: i32, ty: i32, proto: i32) -> R<(i32, i32)> {
        if domain != AF_UNIX {
            return Err(Errno::EAFNOSUPPORT);
        }
        let (a, b) = sys::socketpair(domain, ty, proto)?;
        Ok((self.keep(a), self.keep(b)))
    }

    /// Family, type and protocol.
    pub fn info(&self, k: i32) -> R<Info> {
        sys::info(self.fd(k)?)
    }

    /// `bind()`: the port restriction check (`EACCES`), the bind, then
    /// the backstop mark.
    pub fn bind(&mut self, k: i32, a: &SockAddr) -> R<()> {
        let fd = self.fd(k)?;
        let info = sys::info(fd)?;
        if info.family != a.family() && !matches!(a, SockAddr::Unspec) {
            return Err(Errno::EAFNOSUPPORT);
        }
        if matches!(info.family, AF_INET | AF_INET6)
            && a.port() != 0
            && let Some(p) = protocol_of(info.socktype, info.protocol)
            && !self.rules.may_bind(p, a.port(), &self.user)
        {
            tracing::debug!(port = a.port(), user = %self.user, "bind refused by a port restriction");
            return Err(Errno::EACCES);
        }
        let ka = self.ifs_to_kernel(a, UnixUse::Bind)?;
        sys::bind(fd, &ka)?;
        if let SockAddr::Unix(p) = a {
            self.paths.bound(p);
        }
        if let Some(mark) = self.mark
            && matches!(info.family, AF_INET | AF_INET6)
            && let Err(e) = sys::set_mark(fd, mark)
        {
            tracing::warn!(?e, "SO_MARK not set: the port backstop will drop this socket's traffic on restricted ports");
        }
        Ok(())
    }

    /// `listen()`.
    pub fn listen(&self, k: i32, backlog: i32) -> R<()> {
        let backlog = backlog.clamp(0, crate::consts::SOMAXCONN);
        sys::listen(self.fd(k)?, backlog)
    }

    /// `accept()`: the new kernel number and the peer.
    pub fn accept(&mut self, k: i32) -> R<(i32, Option<SockAddr>)> {
        let (fd, from) = sys::accept(self.fd(k)?)?;
        let from = from.map(|a| self.kernel_to_ifs(a));
        Ok((self.keep(fd), from))
    }

    /// `connect()`.
    pub fn connect(&self, k: i32, a: &SockAddr) -> R<()> {
        let ka = self.ifs_to_kernel(a, UnixUse::Connect)?;
        sys::connect(self.fd(k)?, &ka)
    }

    /// `send()`.
    pub fn send(&self, k: i32, buf: &[u8], flags: i32) -> R<usize> {
        sys::send(self.fd(k)?, buf, flags)
    }

    /// `sendto()`.
    pub fn sendto(&self, k: i32, buf: &[u8], flags: i32, to: &SockAddr) -> R<usize> {
        let ka = self.ifs_to_kernel(to, UnixUse::Connect)?;
        sys::sendto(self.fd(k)?, buf, flags, &ka)
    }

    /// `sendmsg()`, `writev()`.
    pub fn sendv(&self, k: i32, bufs: &[&[u8]], flags: i32, to: Option<&SockAddr>) -> R<usize> {
        let ka = to.map(|a| self.ifs_to_kernel(a, UnixUse::Connect)).transpose()?;
        sys::sendv(self.fd(k)?, bufs, flags, ka.as_ref())
    }

    /// `recv()`.
    pub fn recv(&self, k: i32, buf: &mut [u8], flags: i32) -> R<usize> {
        sys::recv(self.fd(k)?, buf, flags)
    }

    /// `recvfrom()`.
    pub fn recvfrom(&self, k: i32, buf: &mut [u8], flags: i32) -> R<(usize, Option<SockAddr>)> {
        let (n, from) = sys::recvfrom(self.fd(k)?, buf, flags)?;
        Ok((n, from.map(|a| self.kernel_to_ifs(a))))
    }

    /// `recvmsg()`, `readv()`.
    pub fn recvv(&self, k: i32, bufs: &mut [&mut [u8]], flags: i32) -> R<Received> {
        let mut r = sys::recvv(self.fd(k)?, bufs, flags)?;
        r.from = r.from.map(|a| self.kernel_to_ifs(a));
        Ok(r)
    }

    /// `read()` on a socket.
    pub fn read(&self, k: i32, buf: &mut [u8]) -> R<usize> {
        sys::read(self.fd(k)?, buf)
    }

    /// `write()` on a socket.
    pub fn write(&self, k: i32, buf: &[u8]) -> R<usize> {
        sys::write(self.fd(k)?, buf)
    }

    /// `shutdown()`.
    pub fn shutdown(&self, k: i32, how: i32) -> R<()> {
        sys::shutdown(self.fd(k)?, how)
    }

    /// `getsockname()`.
    pub fn sockname(&self, k: i32) -> R<SockAddr> {
        sys::sockname(self.fd(k)?).map(|a| self.kernel_to_ifs(a))
    }

    /// `getpeername()`.
    pub fn peername(&self, k: i32) -> R<SockAddr> {
        sys::peername(self.fd(k)?).map(|a| self.kernel_to_ifs(a))
    }

    /// `O_NONBLOCK` in the kernel.
    pub fn set_nonblock(&self, k: i32, on: bool) -> R<()> {
        sys::set_nonblock(self.fd(k)?, on)
    }

    /// `FIONREAD`.
    pub fn pending(&self, k: i32) -> R<usize> {
        sys::pending(self.fd(k)?)
    }

    /// `getsockopt()`: the value in IBM's layout.
    pub fn getopt(&self, k: i32, level: i32, name: i32) -> R<Vec<u8>> {
        sys::getopt(self.fd(k)?, level, name)
    }

    /// `setsockopt()`.
    pub fn setopt(&self, k: i32, level: i32, name: i32, value: &[u8]) -> R<()> {
        sys::setopt(self.fd(k)?, level, name, value)
    }

    /// `poll()` over kernel numbers: the returned events of each, in
    /// order; a number this job does not hold is `POLLNVAL`. A negative
    /// timeout waits for ever.
    pub fn poll(&self, items: &[(i32, i16)], timeout_ms: i32) -> R<Vec<i16>> {
        let known: Vec<(&Fd, i16)> = items
            .iter()
            .filter_map(|(k, ev)| self.open.get(k).map(|fd| (fd, *ev)))
            .collect();
        if known.len() < items.len() {
            // Unknown numbers answer at once, as the kernel does.
            let mut ready = if known.is_empty() {
                Vec::new()
            } else {
                sys::poll(&known, 0)?
            }
            .into_iter();
            return Ok(items
                .iter()
                .map(|(k, _)| {
                    if self.open.contains_key(k) {
                        ready.next().unwrap_or(0)
                    } else {
                        POLLNVAL
                    }
                })
                .collect());
        }
        if known.is_empty() {
            if timeout_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(u64::try_from(timeout_ms).unwrap_or(0)));
            }
            return Ok(Vec::new());
        }
        sys::poll(&known, timeout_ms)
    }

    /// Close a kernel number.
    pub fn close(&mut self, k: i32) -> R<()> {
        self.open.remove(&k).map(drop).ok_or(Errno::EBADF)
    }

    /// `givedescriptor()`: a duplicate goes to `target`; this job keeps
    /// its own until it closes it, as on the machine.
    pub fn give(&mut self, k: i32, target: &JobId) -> R<()> {
        let dup = sys::dup(self.fd(k)?)?;
        self.passing.give(dup, target)
    }

    /// `takedescriptor()`: the new kernel number.
    pub fn take(&mut self, source: Option<&JobId>) -> R<i32> {
        let fd = self.passing.take(source)?;
        Ok(self.keep(fd))
    }

    /// Adopt a kernel descriptor made elsewhere (a relay, a server's
    /// listener); its number.
    pub fn adopt(&mut self, fd: Fd) -> i32 {
        self.keep(fd)
    }
}
