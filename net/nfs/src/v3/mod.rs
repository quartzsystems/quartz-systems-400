// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS version 3 (RFC 1813) over a [`Vfs`]: all 22 procedures.
//!
//! [`call`] answers one call; [`Nfs3`] is the program of a thread that
//! serves version 3 alone. A program serving versions 3 and 4 on one
//! port (program 100003 is one program) calls [`call`] for version 3.
//! The procedures that change something are kept by the duplicate
//! request cache ([`crate::rpc::drc::NFS3_NONIDEMPOTENT`]; the daemon
//! sets it on its dispatcher, [`crate::server::run::nfs3_cache`]).
//! Every handle passes the [`Gate`]; a change through an export the
//! client may not write is `NFS3ERR_ROFS`.

pub mod conv;
mod dir;
mod proc;

use crate::rpc::{Answer, CallContext, LocalProgram, decode_args, encode_res};
use crate::server::{Gate, Vfs};
use crate::types::nfs3::NFS_PROGRAM;
use crate::xdr::Decoder;

/// The largest `READ` and `WRITE` (`FSINFO` `rtmax`, `wtmax`).
pub const MAX_IO: u32 = 1 << 20;

/// The preferred size (`rtpref`, `wtpref`): the largest, as each call
/// costs the service requests (and with `*SYNC` a sync) whatever its size.
pub const PREF_IO: u32 = MAX_IO;

/// The preferred `READDIR` size (`dtpref`).
pub const PREF_DIR: u32 = 64 * 1024;

/// One NFS version 3 call against `vfs`.
pub fn call<V: Vfs + ?Sized>(
    vfs: &V,
    gate: &Gate,
    ctx: &CallContext,
    d: &mut Decoder<'_>,
) -> Answer {
    use NFS_PROGRAM::NFS_V3::{Args, Res};
    let p = proc::Procs { vfs, gate, ctx };
    let res = match decode_args::<Args>(ctx, d)? {
        Args::NFSPROC3_NULL => Res::NFSPROC3_NULL,
        Args::NFSPROC3_GETATTR(a) => Res::NFSPROC3_GETATTR(p.getattr(&a)),
        Args::NFSPROC3_SETATTR(a) => Res::NFSPROC3_SETATTR(p.setattr(&a)),
        Args::NFSPROC3_LOOKUP(a) => Res::NFSPROC3_LOOKUP(p.lookup(&a)),
        Args::NFSPROC3_ACCESS(a) => Res::NFSPROC3_ACCESS(p.access(&a)),
        Args::NFSPROC3_READLINK(a) => Res::NFSPROC3_READLINK(p.readlink(&a)),
        Args::NFSPROC3_READ(a) => Res::NFSPROC3_READ(p.read(&a)),
        Args::NFSPROC3_WRITE(a) => Res::NFSPROC3_WRITE(p.write(&a)),
        Args::NFSPROC3_CREATE(a) => Res::NFSPROC3_CREATE(p.create(&a)),
        Args::NFSPROC3_MKDIR(a) => Res::NFSPROC3_MKDIR(p.mkdir(&a)),
        Args::NFSPROC3_SYMLINK(a) => Res::NFSPROC3_SYMLINK(p.symlink(&a)),
        Args::NFSPROC3_MKNOD(a) => Res::NFSPROC3_MKNOD(p.mknod(&a)),
        Args::NFSPROC3_REMOVE(a) => Res::NFSPROC3_REMOVE(p.remove(&a)),
        Args::NFSPROC3_RMDIR(a) => Res::NFSPROC3_RMDIR(p.rmdir(&a)),
        Args::NFSPROC3_RENAME(a) => Res::NFSPROC3_RENAME(p.rename(&a)),
        Args::NFSPROC3_LINK(a) => Res::NFSPROC3_LINK(p.link(&a)),
        Args::NFSPROC3_READDIR(a) => Res::NFSPROC3_READDIR(p.readdir(&a)),
        Args::NFSPROC3_READDIRPLUS(a) => Res::NFSPROC3_READDIRPLUS(p.readdirplus(&a)),
        Args::NFSPROC3_FSSTAT(a) => Res::NFSPROC3_FSSTAT(p.fsstat(&a)),
        Args::NFSPROC3_FSINFO(a) => Res::NFSPROC3_FSINFO(p.fsinfo(&a)),
        Args::NFSPROC3_PATHCONF(a) => Res::NFSPROC3_PATHCONF(p.pathconf(&a)),
        Args::NFSPROC3_COMMIT(a) => Res::NFSPROC3_COMMIT(p.commit(&a)),
    };
    encode_res(&res)
}

/// NFS version 3 for one thread: its [`Vfs`] and the export table.
#[derive(Debug)]
pub struct Nfs3<V: Vfs> {
    /// The thread's file system.
    pub vfs: V,
    /// The export table.
    pub gate: Gate,
}

impl<V: Vfs> Nfs3<V> {
    /// The program over `vfs`.
    pub fn new(vfs: V, gate: Gate) -> Self {
        Self { vfs, gate }
    }

    /// The file system underneath.
    pub fn vfs(&self) -> &V {
        &self.vfs
    }
}

impl<V: Vfs> LocalProgram for Nfs3<V> {
    fn program(&self) -> u32 {
        NFS_PROGRAM::NUMBER
    }
    fn versions(&self) -> &[u32] {
        &[3]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        call(&self.vfs, &self.gate, ctx, d)
    }
}
