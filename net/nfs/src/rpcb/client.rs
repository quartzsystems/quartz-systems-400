// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A binder client for the daemons (registering) and the NFS client
//! (finding a program's port).

use std::net::{IpAddr, SocketAddr};

use super::uaddr::from_uaddr;
use crate::rpc::{ClientConfig, Credential, Proto, RpcClient, RpcError};
use crate::types::rpcb::{PMAP_PORT, PMAP_PROG, RPCBPROG, mapping, rpcb};

/// A client of the binder on one host.
#[derive(Debug)]
pub struct BinderClient {
    rpc: RpcClient,
    host: IpAddr,
    cred: Credential,
}

impl BinderClient {
    /// A client of the binder at `host` port 111.
    pub fn new(host: IpAddr, proto: Proto, cfg: ClientConfig) -> Self {
        Self::at(
            SocketAddr::new(host, u16::try_from(PMAP_PORT).unwrap_or(111)),
            proto,
            cfg,
        )
    }

    /// A client of a binder at `addr`.
    pub fn at(addr: SocketAddr, proto: Proto, cfg: ClientConfig) -> Self {
        Self {
            rpc: RpcClient::new(addr, proto, cfg),
            host: addr.ip(),
            cred: Credential::None,
        }
    }

    /// Calls with `cred` from now on.
    pub fn set_credential(&mut self, cred: Credential) {
        self.cred = cred;
    }

    /// The transport underneath.
    pub fn rpc(&mut self) -> &mut RpcClient {
        &mut self.rpc
    }

    /// Portmap `GETPORT`: the port of `prog` and `vers` over `prot`
    /// (6 TCP, 17 UDP); `None` when not registered.
    pub fn getport(
        &mut self,
        prog: u32,
        vers: u32,
        protocol: u32,
    ) -> Result<Option<u16>, RpcError> {
        use PMAP_PROG::PMAP_VERS::{Args, Res};
        let m = mapping {
            prog,
            vers,
            prot: protocol,
            port: 0,
        };
        match self
            .rpc
            .call_proc::<Args, Res>(&self.cred, &Args::PMAPPROC_GETPORT(m))?
        {
            Res::PMAPPROC_GETPORT(p) => Ok(u16::try_from(p).ok().filter(|p| *p != 0)),
            _ => Ok(None),
        }
    }

    fn query(prog: u32, vers: u32, netid: &str) -> rpcb {
        rpcb {
            r_prog: prog,
            r_vers: vers,
            r_netid: netid.as_bytes().to_vec(),
            r_addr: Vec::new(),
            r_owner: Vec::new(),
        }
    }

    /// The address of a universal address, a wildcard host made the binder's.
    fn address(&self, uaddr: &[u8]) -> Option<SocketAddr> {
        let a = from_uaddr(std::str::from_utf8(uaddr).ok()?)?;
        Some(if a.ip().is_unspecified() {
            SocketAddr::new(self.host, a.port())
        } else {
            a
        })
    }

    /// Rpcbind 4 `GETADDR` (3 when the binder has no 4): the address of
    /// `prog` and `vers` on `netid`.
    pub fn getaddr(
        &mut self,
        prog: u32,
        vers: u32,
        netid: &str,
    ) -> Result<Option<SocketAddr>, RpcError> {
        let q = Self::query(prog, vers, netid);
        let r = {
            use RPCBPROG::RPCBVERS4::{Args, Res};
            match self
                .rpc
                .call_proc::<Args, Res>(&self.cred, &Args::RPCBPROC_GETADDR(q.clone()))
            {
                Ok(Res::RPCBPROC_GETADDR(s)) => s.0,
                Ok(_) => Vec::new(),
                Err(e) if e.mismatch().is_some() => {
                    use RPCBPROG::RPCBVERS::{Args, Res};
                    match self
                        .rpc
                        .call_proc::<Args, Res>(&self.cred, &Args::RPCBPROC_GETADDR(q))?
                    {
                        Res::RPCBPROC_GETADDR(s) => s.0,
                        _ => Vec::new(),
                    }
                }
                Err(e) => return Err(e),
            }
        };
        Ok(self.address(&r))
    }

    /// Rpcbind 4 `SET`.
    pub fn set(&mut self, r: &rpcb) -> Result<bool, RpcError> {
        use RPCBPROG::RPCBVERS4::{Args, Res};
        match self
            .rpc
            .call_proc::<Args, Res>(&self.cred, &Args::RPCBPROC_SET(r.clone()))?
        {
            Res::RPCBPROC_SET(ok) => Ok(ok),
            _ => Ok(false),
        }
    }

    /// Rpcbind 4 `UNSET`.
    pub fn unset(&mut self, r: &rpcb) -> Result<bool, RpcError> {
        use RPCBPROG::RPCBVERS4::{Args, Res};
        match self
            .rpc
            .call_proc::<Args, Res>(&self.cred, &Args::RPCBPROC_UNSET(r.clone()))?
        {
            Res::RPCBPROC_UNSET(ok) => Ok(ok),
            _ => Ok(false),
        }
    }

    /// Rpcbind 4 `DUMP`.
    pub fn dump(&mut self) -> Result<Vec<rpcb>, RpcError> {
        use RPCBPROG::RPCBVERS4::{Args, Res};
        match self
            .rpc
            .call_proc::<Args, Res>(&self.cred, &Args::RPCBPROC_DUMP)?
        {
            Res::RPCBPROC_DUMP(l) => Ok(l.0.into_iter().map(|e| e.rpcb_map).collect()),
            _ => Ok(Vec::new()),
        }
    }
}
