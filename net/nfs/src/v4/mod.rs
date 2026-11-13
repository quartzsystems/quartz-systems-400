// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS versions 4.0 (RFC 7530) and 4.1 (RFC 8881) over a [`Vfs`]
//! (docs/NFS.md section 5.10).
//!
//! - [`Server`]: what a `QNFSNFSD` job's threads share (the state, the
//!   start, the registry, the stable records, callbacks) and its
//!   background thread.
//! - [`Nfs34`]: one connection thread's program 100003, versions 3 and
//!   4 ([`Server::program`]); `server::run::nfsd4` serves it.
//! - [`attr`]: the attribute table and `fattr4`'s codec.
//! - [`pseudo`]: the pseudo file system.
//! - [`state`]: clients, sessions, owners, stateids, the stable records.
//! - [`cb`]: callbacks.
//!
//! What is not done is refused as docs/NFS.md section 10 says: pNFS,
//! directory delegations, write delegations, named attributes, ACLs,
//! referrals, `SP4_MACH_CRED` and `SP4_SSV`.

#![allow(clippy::many_single_char_names)]

pub mod attr;
pub mod cb;
mod compound;
pub mod conv;
mod ops;
pub mod pseudo;
pub mod server;
pub mod state;

use std::cell::RefCell;
use std::sync::Arc;

pub use server::{Config, Server};

use crate::rpc::back::BackChannel;
use crate::rpc::{AcceptError, Answer, CallContext, LocalProgram};
use crate::server::{FileHandle, Gate, Req, Vfs};
use crate::types::nfs4::NFS4_PROGRAM;
use crate::xdr::Decoder;

/// The current or saved filehandle of a compound.
#[derive(Debug, Clone)]
pub(crate) enum Fh {
    /// None set.
    None,
    /// A directory of the pseudo file system.
    Pseudo(u64),
    /// An object of an export, with the request through it.
    Real(Req, FileHandle),
}

/// Program 100003, versions 3 and 4, for one connection thread: version
/// 3 is [`crate::v3::call`], version 4 the compound.
pub struct Nfs34<V: Vfs> {
    srv: Arc<Server>,
    vfs: V,
    gate: Gate,
    back: RefCell<Option<Arc<BackChannel>>>,
}

impl<V: Vfs> std::fmt::Debug for Nfs34<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Nfs34").finish_non_exhaustive()
    }
}

impl Server {
    /// The program of one connection thread over its own `vfs`.
    pub fn program<V: Vfs>(self: &Arc<Self>, vfs: V) -> Nfs34<V> {
        Nfs34 {
            srv: Arc::clone(self),
            vfs,
            gate: Gate::new(Arc::clone(&self.exports)),
            back: RefCell::new(None),
        }
    }
}

impl<V: Vfs> Nfs34<V> {
    /// The thread's file system.
    pub fn vfs(&self) -> &V {
        &self.vfs
    }

    /// The server.
    pub fn server(&self) -> &Arc<Server> {
        &self.srv
    }
}

impl<V: Vfs> LocalProgram for Nfs34<V> {
    fn program(&self) -> u32 {
        NFS4_PROGRAM::NUMBER
    }
    fn versions(&self) -> &[u32] {
        &[3, 4]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        if ctx.vers == 3 {
            return crate::v3::call(&self.vfs, &self.gate, ctx, d);
        }
        match ctx.procedure {
            0 if d.is_empty() => Ok(Some(Vec::new())),
            0 => Err(AcceptError::GarbageArgs),
            1 => {
                let back = self.back.borrow().clone();
                compound::compound(&self.srv, &self.vfs, ctx, back.as_ref(), d)
            }
            _ => Err(AcceptError::ProcUnavail),
        }
    }
    fn attach(&self, back: &Arc<BackChannel>) {
        *self.back.borrow_mut() = Some(Arc::clone(back));
    }
}

impl<V: Vfs> Drop for Nfs34<V> {
    fn drop(&mut self) {
        if let Some(b) = self.back.borrow().as_ref() {
            self.srv.conn_gone(b.peer());
        }
    }
}
