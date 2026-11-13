// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Protocol types generated from the descriptions in `x/` by xdrgen
//! (docs/NFS.md section 3). Every name is the description's: a type
//! `nfs_fh3`, a field `obj_attributes`, an enum member
//! `nfsstat3::NFS3ERR_NOENT`, a union arm `GETATTR3res::NFS3_OK(..)`
//! or `GETATTR3res::default(status)`. Each program has a module per
//! version with `Args` and `Res`: `nfs3::NFS_PROGRAM::NFS_V3::Args`.

pub mod mount;
pub mod nfs3;
pub mod nfs4;
pub mod nlm;
pub mod nsm;
pub mod rpc;
pub mod rpcb;
