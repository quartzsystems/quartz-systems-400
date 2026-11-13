// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What is not done, refused as the RFCs allow: named attributes
//! (`OPENATTR`, `NFS4ERR_NOTSUPP`). pNFS, directory delegations and
//! `WANT_DELEGATION` are refused in the dispatch (`NFS4ERR_NOTSUPP`).

use crate::types::nfs4::{OPENATTR4args, nfsstat4};
use crate::v4::compound::{Cx, Res};

impl Cx<'_> {
    pub(super) fn op_openattr(&mut self, a: &OPENATTR4args) -> Res {
        let _ = a;
        match self.need() {
            Ok(_) => Res::Err(nfsstat4::NFS4ERR_NOTSUPP),
            Err(s) => Res::Err(s),
        }
    }
}
