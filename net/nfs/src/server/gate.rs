// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The check every call passes (docs/NFS.md section 5): the handle is
//! one of ours, its export is still in the table with the same root
//! (so `EXPORTFS -U` takes effect at once), the client's address may
//! use it with this protocol version, and the caller is mapped.

use std::sync::Arc;

use super::cred;
use super::error::VfsError;
use super::export::{ExportTable, ExportView};
use super::handle::FileHandle;
use super::vfs::Req;
use crate::rpc::CallContext;

/// The export table, as the protocols check against it.
#[derive(Clone)]
pub struct Gate {
    /// The exports.
    pub exports: Arc<dyn ExportTable>,
}

impl std::fmt::Debug for Gate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gate").finish_non_exhaustive()
    }
}

impl Gate {
    /// A gate over `exports`.
    pub fn new(exports: Arc<dyn ExportTable>) -> Self {
        Self { exports }
    }

    /// The request for a call through `export` at NFS version `vers`.
    ///
    /// # Errors
    ///
    /// [`VfsError::Acces`] when the client's address may not use the
    /// export, the version is not one it allows, or the caller is
    /// anonymous and refused.
    pub fn req(&self, ctx: &CallContext, export: Arc<dyn ExportView>, vers: u32) -> Result<Req, VfsError> {
        let peer = ctx.peer.ip();
        if !export.allows(peer) || !export.version_allowed(vers) {
            return Err(VfsError::Acces);
        }
        let caller = cred::caller(&ctx.cred, peer, &*export)?;
        let opts = export.host_opts(peer);
        Ok(Req {
            caller,
            export,
            peer,
            opts,
        })
    }

    /// The handle `fh` and the request through its export.
    ///
    /// # Errors
    ///
    /// [`VfsError::BadHandle`] for bytes that are not a handle,
    /// [`VfsError::Stale`] when its export is gone or exports another
    /// directory now, and those of [`Gate::req`].
    pub fn open(&self, ctx: &CallContext, fh: &[u8], vers: u32) -> Result<(Req, FileHandle), VfsError> {
        let h = FileHandle::from_bytes(fh)?;
        let e = self.exports.by_id(h.export).ok_or(VfsError::Stale)?;
        if e.root_id().ino != h.root_ino {
            return Err(VfsError::Stale);
        }
        Ok((self.req(ctx, e, vers)?, h))
    }

    /// A second handle of the same call (`RENAME`, `LINK`): in the same
    /// export as `r`, else [`VfsError::XDev`].
    ///
    /// # Errors
    ///
    /// Those of [`FileHandle::from_bytes`] and `XDev`.
    pub fn second(&self, r: &Req, fh: &[u8]) -> Result<FileHandle, VfsError> {
        let h = FileHandle::from_bytes(fh)?;
        if h.export != r.export.id() {
            let other = self.exports.by_id(h.export).ok_or(VfsError::Stale)?;
            if other.root_id().ino != h.root_ino {
                return Err(VfsError::Stale);
            }
            return Err(VfsError::XDev);
        }
        if r.export.root_id().ino != h.root_ino {
            return Err(VfsError::Stale);
        }
        Ok(h)
    }
}

/// Whether `r`'s client may change the export.
#[must_use]
pub fn writable(r: &Req) -> bool {
    r.export.writable(r.peer)
}
