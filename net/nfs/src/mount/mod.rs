// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The MOUNT protocol, versions 3 (RFC 1813 appendix I) and 1 (for
//! `showmount`; its `MNT` answers the same 32-byte handle).
//!
//! `MNT` takes a path in an export (the export itself or a directory
//! under it; the longest export holding it wins), checks the client's
//! address against the export (`MNT3ERR_ACCES`), finds the directory as
//! the caller (`MNT3ERR_NOENT`, `MNT3ERR_NOTDIR`, `MNT3ERR_ACCES`) and
//! answers its handle with the export's flavors (`AUTH_SYS`). A path in
//! no export is `MNT3ERR_ACCES`. The path is in the host's path name
//! CCSID. `DUMP`, `UMNT`, `UMNTALL` keep the [`list::MountList`];
//! `EXPORT` lists every export and the hosts of its `ACCESS=`.

pub mod list;

use std::sync::Arc;

use self::list::MountList;
use crate::rpc::{
    AcceptError, Answer, CallContext, Credential, LocalProgram, decode_args, encode_res,
};
use crate::server::export::within;
use crate::server::{ExportView, Gate, Vfs, VfsError, text};
use crate::types::mount::{
    MOUNT_PROGRAM, dirpath, exportnode, exports, fhandle3, fhstatus, groupnode, groups, mountbody,
    mountlist, mountres3, mountres3_ok, mountstat3, name,
};
use crate::xdr::Decoder;

/// MOUNT for one thread.
pub struct Mountd<V: Vfs> {
    /// The thread's file system.
    pub vfs: V,
    /// The exports.
    pub gate: Gate,
    /// The mount list.
    pub list: Arc<dyn MountList>,
}

impl<V: Vfs> std::fmt::Debug for Mountd<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mountd").finish_non_exhaustive()
    }
}

fn mnt_status(e: VfsError) -> mountstat3 {
    match e {
        VfsError::NoEnt | VfsError::Stale => mountstat3::MNT3ERR_NOENT,
        VfsError::NotDir => mountstat3::MNT3ERR_NOTDIR,
        VfsError::Acces | VfsError::Perm => mountstat3::MNT3ERR_ACCES,
        VfsError::NameTooLong => mountstat3::MNT3ERR_NAMETOOLONG,
        VfsError::Inval => mountstat3::MNT3ERR_INVAL,
        VfsError::NotSupp => mountstat3::MNT3ERR_NOTSUPP,
        VfsError::Io => mountstat3::MNT3ERR_IO,
        _ => mountstat3::MNT3ERR_SERVERFAULT,
    }
}

/// The client's name for the list: its `AUTH_SYS` machine name, else its address.
fn host_of(ctx: &CallContext) -> String {
    match &ctx.cred {
        Credential::Sys(p) if !p.machinename.is_empty() => {
            String::from_utf8_lossy(&p.machinename).into_owned()
        }
        _ => ctx.peer.ip().to_string(),
    }
}

impl<V: Vfs> Mountd<V> {
    /// MOUNT over `vfs`.
    pub fn new(vfs: V, gate: Gate, list: Arc<dyn MountList>) -> Self {
        Self { vfs, gate, list }
    }

    /// The path as the IFS's, the handle and flavors, or why not.
    /// The export holding `path` (the client's bytes) and the path as the IFS's.
    fn exported(&self, ctx: &CallContext, path: &[u8]) -> Option<(Arc<dyn ExportView>, String)> {
        let peer = ctx.peer.ip();
        let mut all = self.gate.exports.all();
        all.sort_by_key(|e| std::cmp::Reverse(e.path().len()));
        all.into_iter().find_map(|e| {
            let p = text::from_client(e.host_opts(peer).path_ccsid, path).ok()?;
            within(e.path(), &p).is_some().then_some((e, p))
        })
    }

    fn mnt(
        &self,
        ctx: &CallContext,
        path: &[u8],
    ) -> Result<(String, [u8; 32], Vec<i32>), mountstat3> {
        let Some((e, p)) = self.exported(ctx, path) else {
            return Err(mountstat3::MNT3ERR_ACCES);
        };
        let req = self.gate.req(ctx, e, 3).map_err(mnt_status)?;
        let node = self.vfs.lookup_path(&req, path).map_err(mnt_status)?;
        self.list.add(&host_of(ctx), &p);
        Ok((p, node.fh.to_bytes(), req.export.flavors()))
    }

    fn dump(&self) -> mountlist {
        mountlist(
            self.list
                .list()
                .into_iter()
                .map(|(h, d)| mountbody {
                    ml_hostname: name(h.into_bytes()),
                    ml_directory: dirpath(d.into_bytes()),
                })
                .collect(),
        )
    }

    fn umnt(&self, ctx: &CallContext, path: &[u8]) {
        let p = match self.exported(ctx, path) {
            Some((_, p)) => p,
            None => String::from_utf8_lossy(path).into_owned(),
        };
        self.list.remove(&host_of(ctx), &p);
    }

    fn exports(&self) -> exports {
        exports(
            self.gate
                .exports
                .all()
                .into_iter()
                .map(|e| exportnode {
                    ex_dir: dirpath(e.path().as_bytes().to_vec()),
                    ex_groups: groups(
                        e.groups()
                            .into_iter()
                            .map(|g| groupnode {
                                gr_name: name(g.into_bytes()),
                            })
                            .collect(),
                    ),
                })
                .collect(),
        )
    }

    fn v3(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        use MOUNT_PROGRAM::MOUNT_V3::{Args, Res};
        let res = match decode_args::<Args>(ctx, d)? {
            Args::MOUNTPROC3_NULL => Res::MOUNTPROC3_NULL,
            Args::MOUNTPROC3_MNT(p) => Res::MOUNTPROC3_MNT(match self.mnt(ctx, &p.0) {
                Ok((_, fh, flavors)) => mountres3::MNT3_OK(mountres3_ok {
                    fhandle: fhandle3(fh.to_vec()),
                    auth_flavors: flavors,
                }),
                Err(s) => mountres3::default(s),
            }),
            Args::MOUNTPROC3_DUMP => Res::MOUNTPROC3_DUMP(self.dump()),
            Args::MOUNTPROC3_UMNT(p) => {
                self.umnt(ctx, &p.0);
                Res::MOUNTPROC3_UMNT
            }
            Args::MOUNTPROC3_UMNTALL => {
                self.list.remove_host(&host_of(ctx));
                Res::MOUNTPROC3_UMNTALL
            }
            Args::MOUNTPROC3_EXPORT => Res::MOUNTPROC3_EXPORT(self.exports()),
        };
        encode_res(&res)
    }

    fn v1(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        use MOUNT_PROGRAM::MOUNT_V1::{Args, Res};
        let res = match decode_args::<Args>(ctx, d)? {
            Args::MOUNTPROC_NULL => Res::MOUNTPROC_NULL,
            Args::MOUNTPROC_MNT(p) => Res::MOUNTPROC_MNT(match self.mnt(ctx, &p.0) {
                Ok((_, fh, _)) => fhstatus::V0(fh),
                Err(s) => fhstatus::default(u32::try_from(s.value()).unwrap_or(5)),
            }),
            Args::MOUNTPROC_DUMP => Res::MOUNTPROC_DUMP(self.dump()),
            Args::MOUNTPROC_UMNT(p) => {
                self.umnt(ctx, &p.0);
                Res::MOUNTPROC_UMNT
            }
            Args::MOUNTPROC_UMNTALL => {
                self.list.remove_host(&host_of(ctx));
                Res::MOUNTPROC_UMNTALL
            }
            Args::MOUNTPROC_EXPORT => Res::MOUNTPROC_EXPORT(self.exports()),
            Args::MOUNTPROC_EXPORTALL => Res::MOUNTPROC_EXPORTALL(self.exports()),
        };
        encode_res(&res)
    }
}

impl<V: Vfs> LocalProgram for Mountd<V> {
    fn program(&self) -> u32 {
        MOUNT_PROGRAM::NUMBER
    }
    fn versions(&self) -> &[u32] {
        &[1, 3]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        match ctx.vers {
            1 => self.v1(ctx, d),
            3 => self.v3(ctx, d),
            _ => Err(AcceptError::ProcUnavail),
        }
    }
}
