// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where each program listens: explicit ports first, then the binder's
//! answers through the port map.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

use onc_rpcbind::{mapping, pmaplist};

use crate::rpc::{Client, Cred, Proto};
use crate::{Error, Target};

/// Program numbers.
pub mod prog {
    /// The binder.
    pub const PMAP: u32 = 100_000;
    /// NFS.
    pub const NFS: u32 = 100_003;
    /// MOUNT.
    pub const MOUNT: u32 = 100_005;
    /// NLM.
    pub const NLM: u32 = 100_021;
    /// NSM.
    pub const NSM: u32 = 100_024;
}

/// The target and its programs' addresses.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// The command line.
    pub t: Target,
    /// The server's address.
    pub ip: IpAddr,
    /// The binder.
    pub pmap: SocketAddr,
    /// NFS.
    pub nfs: SocketAddr,
    /// MOUNT, when known.
    pub mount: Option<SocketAddr>,
    /// NLM, when known.
    pub nlm: Option<SocketAddr>,
    /// NSM, when known.
    pub nsm: Option<SocketAddr>,
}

impl Ctx {
    /// Resolves the host and the ports.
    pub fn resolve(t: &Target) -> Result<Ctx, Error> {
        let ip = (t.host.as_str(), 0)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| Error::Usage(format!("no address for {}", t.host)))?
            .ip();
        let pmap = SocketAddr::new(ip, t.pmap_port);
        let mut c = Ctx {
            t: t.clone(),
            ip,
            pmap,
            nfs: SocketAddr::new(ip, t.port.unwrap_or(2049)),
            mount: t.mount_port.map(|p| SocketAddr::new(ip, p)),
            nlm: t.nlm_port.map(|p| SocketAddr::new(ip, p)),
            nsm: t.nsm_port.map(|p| SocketAddr::new(ip, p)),
        };
        let need = t.port.is_none()
            || t.mount_port.is_none()
            || t.nlm_port.is_none()
            || t.nsm_port.is_none();
        if need && let Ok(list) = dump(&c) {
            let find = |prog: u32, vers: u32| {
                list.iter()
                    .find(|m| m.prog == prog && m.vers == vers && m.prot == t.proto.number())
                    .and_then(|m| u16::try_from(m.port).ok())
                    .map(|p| SocketAddr::new(ip, t.map_port(p)))
            };
            if t.port.is_none()
                && let Some(a) = find(prog::NFS, 3).or_else(|| find(prog::NFS, 4))
            {
                c.nfs = a;
            }
            if c.mount.is_none() {
                c.mount = find(prog::MOUNT, 3);
            }
            if c.nlm.is_none() {
                c.nlm = find(prog::NLM, 4);
            }
            if c.nsm.is_none() {
                c.nsm = find(prog::NSM, 1);
            }
        }
        Ok(c)
    }

    /// A client of `addr` over the chosen transport as the chosen user.
    pub fn client(&self, addr: SocketAddr) -> Client {
        self.client_as(addr, self.t.proto, Cred::sys(self.t.uid, self.t.gid))
    }

    /// A client of `addr` with this transport and credential.
    pub fn client_as(&self, addr: SocketAddr, proto: Proto, cred: Cred) -> Client {
        Client::new(addr, proto, self.t.timeout, cred)
    }

    /// MOUNT's address, or a skip.
    pub fn mount_addr(&self) -> Result<SocketAddr, Error> {
        self.mount
            .ok_or_else(|| Error::Skip("MOUNT's port is not known (--mountport)".into()))
    }

    /// NLM's address, or a skip.
    pub fn nlm_addr(&self) -> Result<SocketAddr, Error> {
        self.nlm
            .ok_or_else(|| Error::Skip("NLM's port is not known (--nlmport)".into()))
    }

    /// NSM's address, or a skip.
    pub fn nsm_addr(&self) -> Result<SocketAddr, Error> {
        self.nsm
            .ok_or_else(|| Error::Skip("NSM's port is not known (--nsmport)".into()))
    }
}

/// The binder's table (portmap version 2 `DUMP`), over TCP then UDP.
pub fn dump(c: &Ctx) -> Result<Vec<mapping>, Error> {
    let mut last = None;
    for proto in [Proto::Tcp, Proto::Udp] {
        let mut cl = c.client_as(c.pmap, proto, Cred::None);
        match cl.call::<onc_xdr::Void, pmaplist>(prog::PMAP, 2, 4, &onc_xdr::Void) {
            Ok(l) => return Ok(l.0),
            Err(e) => last = Some(e),
        }
    }
    Err(last.expect("two tries"))
}

/// The binder's `GETPORT` answer for one program.
pub fn getport(c: &Ctx, prog: u32, vers: u32, proto: Proto) -> Result<u32, Error> {
    let mut cl = c.client_as(c.pmap, Proto::Tcp, Cred::None);
    cl.call::<mapping, u32>(
        prog::PMAP,
        2,
        3,
        &mapping {
            prog,
            vers,
            prot: proto.number(),
            port: 0,
        },
    )
}
