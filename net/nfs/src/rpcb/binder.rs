// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The binder as a [`Program`]: program 100000, portmap version 2 and
//! rpcbind versions 3 and 4 (RFC 1833).
//!
//! - `SET` and `UNSET` are taken from the local host only (the caller's
//!   address is a loopback one); from anywhere else they answer FALSE,
//!   as rpcbind does. `UNSET` removes only the caller's registrations
//!   unless the caller is the superuser (`AUTH_SYS` UID 0 or owner `0`).
//! - Remote calls (`PMAPPROC_CALLIT`, `RPCBPROC_CALLIT`, `RPCBPROC_BCAST`,
//!   `RPCBPROC_INDIRECT`) are not made for anyone: over TCP they answer
//!   `PROC_UNAVAIL`; over UDP, where a failed indirect call has no reply
//!   (RFC 1833), nothing is sent.
//! - Wildcard addresses are answered as registered (`0.0.0.0.8.1`,
//!   `::.8.1`): the client uses the port with the address it called.

use std::sync::{Mutex, PoisonError};

use super::table::{Stats, Table};
use super::uaddr::{Netid, from_taddr, port_of, to_taddr};
use crate::rpc::{
    AcceptError, Answer, CallContext, Credential, Program, Transport, decode_args, encode_res,
};
use crate::types::rpcb::{
    PMAP_PROG, RPCBPROG, mapping, netbuf, pmaplist, pmaplist_ptr, rp__list, rpcb, rpcb_entry,
    rpcb_entry_list, rpcb_entry_list_ptr, rpcb_stat, rpcb_stat_byvers, rpcb_string, rpcblist_ptr,
    rpcbs_addrlist, rpcbs_addrlist_ptr, rpcbs_proc, rpcbs_rmtcalllist_ptr,
};
use crate::xdr::Decoder;

/// The binder's program number.
pub const PROGRAM: u32 = 100_000;

/// The owner the oracle writes for a registration of unknown owner.
pub const UNKNOWN_OWNER: &[u8] = b"4294967295";

/// Called with every registration after a change.
pub type OnChange = Box<dyn Fn(&[rpcb]) + Send + Sync>;

/// The binder.
pub struct Binder {
    state: Mutex<(Table, Stats)>,
    on_change: Option<OnChange>,
}

impl std::fmt::Debug for Binder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Binder").finish_non_exhaustive()
    }
}

impl Default for Binder {
    fn default() -> Self {
        Self::new(Table::new())
    }
}

/// The owner a caller's credential names.
fn owner_of(cred: &Credential) -> Vec<u8> {
    match cred.uid() {
        Some(uid) => uid.to_string().into_bytes(),
        None => UNKNOWN_OWNER.to_vec(),
    }
}

impl Binder {
    /// A binder over `table`.
    pub fn new(table: Table) -> Self {
        Self {
            state: Mutex::new((table, Stats::default())),
            on_change: None,
        }
    }

    /// Calls `f` with the registrations after each change (to write `/etc/RPCBTAB`).
    #[must_use]
    pub fn on_change(mut self, f: impl Fn(&[rpcb]) + Send + Sync + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    fn state(&self) -> std::sync::MutexGuard<'_, (Table, Stats)> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The registrations.
    pub fn entries(&self) -> Vec<rpcb> {
        self.state().0.entries().to_vec()
    }

    fn changed(&self, t: &Table) {
        if let Some(f) = &self.on_change {
            f(t.entries());
        }
    }

    /// Registers `r` from within this host (the daemon's own entries).
    pub fn register(&self, r: rpcb) -> bool {
        let mut s = self.state();
        let ok = s.0.set(r);
        if ok {
            self.changed(&s.0);
        }
        ok
    }

    fn set(&self, ctx: &CallContext, r: rpcb) -> bool {
        if !ctx.peer.ip().is_loopback() {
            return false;
        }
        let mut s = self.state();
        let ok = s.0.set(r);
        if ok {
            s.1.changed(ctx.vers, true);
            self.changed(&s.0);
        }
        ok
    }

    fn unset(&self, ctx: &CallContext, prog: u32, vers: u32, netid: &[u8], owner: &[u8]) -> bool {
        if !ctx.peer.ip().is_loopback() {
            return false;
        }
        let owner = if ctx.cred.uid() == Some(0) {
            b"0".as_slice()
        } else {
            owner
        };
        let mut s = self.state();
        let ok = s.0.unset(prog, vers, netid, owner);
        if ok {
            s.1.changed(ctx.vers, false);
            self.changed(&s.0);
        }
        ok
    }

    fn lookup(
        &self,
        ctx: &CallContext,
        prog: u32,
        vers: u32,
        netid: &[u8],
        exact: bool,
    ) -> Option<Vec<u8>> {
        let mut s = self.state();
        let hit = s.0.find(prog, vers, netid, exact).map(|r| r.r_addr.clone());
        s.1.lookup(ctx.vers, prog, vers, netid, hit.is_some());
        hit
    }

    fn pmap(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        use PMAP_PROG::PMAP_VERS::{Args, Res};
        let res = match decode_args::<Args>(ctx, d)? {
            Args::PMAPPROC_NULL => Res::PMAPPROC_NULL,
            Args::PMAPPROC_SET(m) => Res::PMAPPROC_SET(self.pmap_set(ctx, &m)),
            Args::PMAPPROC_UNSET(m) => {
                Res::PMAPPROC_UNSET(self.unset(ctx, m.prog, m.vers, b"", &owner_of(&ctx.cred)))
            }
            Args::PMAPPROC_GETPORT(m) => {
                let port = Netid::of_protocol(m.prot)
                    .and_then(|n| self.lookup(ctx, m.prog, m.vers, n.name().as_bytes(), false))
                    .and_then(|a| port_of(std::str::from_utf8(&a).ok()?));
                Res::PMAPPROC_GETPORT(u32::from(port.unwrap_or(0)))
            }
            Args::PMAPPROC_DUMP => {
                let s = self.state();
                let list =
                    s.0.entries()
                        .iter()
                        .filter_map(|e| {
                            let n = Netid::parse(&e.r_netid)
                                .filter(|n| matches!(n, Netid::Tcp | Netid::Udp))?;
                            let port = port_of(std::str::from_utf8(&e.r_addr).ok()?)?;
                            Some(pmaplist {
                                map: mapping {
                                    prog: e.r_prog,
                                    vers: e.r_vers,
                                    prot: n.protocol(),
                                    port: u32::from(port),
                                },
                            })
                        })
                        .collect();
                Res::PMAPPROC_DUMP(pmaplist_ptr(list))
            }
            Args::PMAPPROC_CALLIT(_) => return remote_call(ctx),
        };
        encode_res(&res)
    }

    fn pmap_set(&self, ctx: &CallContext, m: &mapping) -> bool {
        let (Some(n), Ok(port)) = (Netid::of_protocol(m.prot), u16::try_from(m.port)) else {
            return false;
        };
        self.set(
            ctx,
            rpcb {
                r_prog: m.prog,
                r_vers: m.vers,
                r_netid: n.name().as_bytes().to_vec(),
                r_addr: format!("0.0.0.0.{}.{}", port >> 8, port & 0xff).into_bytes(),
                r_owner: owner_of(&ctx.cred),
            },
        )
    }

    fn rpcb_set(&self, ctx: &CallContext, mut r: rpcb) -> bool {
        if r.r_owner.is_empty() {
            r.r_owner = owner_of(&ctx.cred);
        }
        // An address the binder cannot read is not registered.
        if std::str::from_utf8(&r.r_addr)
            .ok()
            .and_then(port_of)
            .is_none()
        {
            return false;
        }
        self.set(ctx, r)
    }

    fn rpcb_unset(&self, ctx: &CallContext, r: &rpcb) -> bool {
        let owner = if r.r_owner.is_empty() {
            owner_of(&ctx.cred)
        } else {
            r.r_owner.clone()
        };
        self.unset(ctx, r.r_prog, r.r_vers, &r.r_netid, &owner)
    }

    fn dump(&self) -> rpcblist_ptr {
        let s = self.state();
        rpcblist_ptr(
            s.0.entries()
                .iter()
                .map(|e| rp__list {
                    rpcb_map: e.clone(),
                })
                .collect(),
        )
    }

    fn addrlist(&self, ctx: &CallContext, r: &rpcb) -> rpcb_entry_list_ptr {
        let s = self.state();
        let list =
            s.0.entries()
                .iter()
                .filter(|e| e.r_prog == r.r_prog && e.r_vers == r.r_vers)
                .filter(|e| r.r_netid.is_empty() || e.r_netid.eq_ignore_ascii_case(&r.r_netid))
                .filter_map(|e| {
                    let n = Netid::parse(&e.r_netid)?;
                    Some(rpcb_entry_list {
                        rpcb_entry_map: rpcb_entry {
                            r_maddr: e.r_addr.clone(),
                            r_nc_netid: e.r_netid.clone(),
                            r_nc_semantics: n.semantics(),
                            r_nc_protofmly: n.family().as_bytes().to_vec(),
                            r_nc_proto: n.proto().as_bytes().to_vec(),
                        },
                    })
                })
                .collect::<Vec<_>>();
        drop(s);
        let mut st = self.state();
        st.1.lookup(ctx.vers, r.r_prog, r.r_vers, &r.r_netid, !list.is_empty());
        rpcb_entry_list_ptr(list)
    }

    fn stats(&self) -> rpcb_stat_byvers {
        let s = self.state();
        rpcb_stat_byvers(
            s.1.by_vers
                .iter()
                .map(|v| rpcb_stat {
                    info: rpcbs_proc(v.info.to_vec()),
                    setinfo: v.setinfo,
                    unsetinfo: v.unsetinfo,
                    addrinfo: rpcbs_addrlist_ptr(
                        v.addrinfo
                            .iter()
                            .map(|(prog, vers, netid, ok, no)| rpcbs_addrlist {
                                prog: *prog,
                                vers: *vers,
                                success: *ok,
                                failure: *no,
                                netid: netid.clone(),
                            })
                            .collect(),
                    ),
                    rmtinfo: rpcbs_rmtcalllist_ptr(Vec::new()),
                })
                .collect(),
        )
    }

    fn uaddr2taddr(u: &rpcb_string) -> netbuf {
        let buf = std::str::from_utf8(&u.0)
            .ok()
            .and_then(to_taddr)
            .unwrap_or_default();
        netbuf {
            maxlen: u32::try_from(buf.len()).unwrap_or(0),
            buf,
        }
    }

    fn taddr2uaddr(n: &netbuf) -> rpcb_string {
        rpcb_string(from_taddr(&n.buf).unwrap_or_default().into_bytes())
    }

    fn v3(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        use RPCBPROG::RPCBVERS::{Args, Res};
        let res = match decode_args::<Args>(ctx, d)? {
            Args::RPCBPROC_NULL => Res::RPCBPROC_NULL,
            Args::RPCBPROC_SET(r) => Res::RPCBPROC_SET(self.rpcb_set(ctx, r)),
            Args::RPCBPROC_UNSET(r) => Res::RPCBPROC_UNSET(self.rpcb_unset(ctx, &r)),
            Args::RPCBPROC_GETADDR(r) => Res::RPCBPROC_GETADDR(rpcb_string(
                self.lookup(ctx, r.r_prog, r.r_vers, &r.r_netid, false)
                    .unwrap_or_default(),
            )),
            Args::RPCBPROC_DUMP => Res::RPCBPROC_DUMP(self.dump()),
            Args::RPCBPROC_CALLIT(_) => return remote_call(ctx),
            Args::RPCBPROC_GETTIME => Res::RPCBPROC_GETTIME(now()),
            Args::RPCBPROC_UADDR2TADDR(u) => Res::RPCBPROC_UADDR2TADDR(Self::uaddr2taddr(&u)),
            Args::RPCBPROC_TADDR2UADDR(n) => Res::RPCBPROC_TADDR2UADDR(Self::taddr2uaddr(&n)),
        };
        encode_res(&res)
    }

    fn v4(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        use RPCBPROG::RPCBVERS4::{Args, Res};
        let res = match decode_args::<Args>(ctx, d)? {
            Args::RPCBPROC_NULL => Res::RPCBPROC_NULL,
            Args::RPCBPROC_SET(r) => Res::RPCBPROC_SET(self.rpcb_set(ctx, r)),
            Args::RPCBPROC_UNSET(r) => Res::RPCBPROC_UNSET(self.rpcb_unset(ctx, &r)),
            Args::RPCBPROC_GETADDR(r) => Res::RPCBPROC_GETADDR(rpcb_string(
                self.lookup(ctx, r.r_prog, r.r_vers, &r.r_netid, false)
                    .unwrap_or_default(),
            )),
            Args::RPCBPROC_DUMP => Res::RPCBPROC_DUMP(self.dump()),
            Args::RPCBPROC_BCAST(_) | Args::RPCBPROC_INDIRECT(_) => return remote_call(ctx),
            Args::RPCBPROC_GETTIME => Res::RPCBPROC_GETTIME(now()),
            Args::RPCBPROC_UADDR2TADDR(u) => Res::RPCBPROC_UADDR2TADDR(Self::uaddr2taddr(&u)),
            Args::RPCBPROC_TADDR2UADDR(n) => Res::RPCBPROC_TADDR2UADDR(Self::taddr2uaddr(&n)),
            Args::RPCBPROC_GETVERSADDR(r) => Res::RPCBPROC_GETVERSADDR(rpcb_string(
                self.lookup(ctx, r.r_prog, r.r_vers, &r.r_netid, true)
                    .unwrap_or_default(),
            )),
            Args::RPCBPROC_GETADDRLIST(r) => Res::RPCBPROC_GETADDRLIST(self.addrlist(ctx, &r)),
            Args::RPCBPROC_GETSTAT => Res::RPCBPROC_GETSTAT(self.stats()),
        };
        encode_res(&res)
    }
}

/// A remote call: refused over TCP, no reply over UDP.
fn remote_call(ctx: &CallContext) -> Answer {
    match ctx.transport {
        Transport::Tcp => Err(AcceptError::ProcUnavail),
        Transport::Udp => Ok(None),
    }
}

fn now() -> u32 {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    u32::try_from(s & 0xffff_ffff).unwrap_or(0)
}

impl Program for Binder {
    fn program(&self) -> u32 {
        PROGRAM
    }

    fn versions(&self) -> &[u32] {
        &[2, 3, 4]
    }

    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        self.state().1.call(ctx.vers, ctx.procedure);
        match ctx.vers {
            2 => self.pmap(ctx, d),
            3 => self.v3(ctx, d),
            4 => self.v4(ctx, d),
            _ => Err(AcceptError::ProgMismatch { low: 2, high: 4 }),
        }
    }
}
