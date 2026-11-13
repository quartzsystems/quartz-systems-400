// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QNFSRPCD`: the binder: portmap 2 and rpcbind 3 and 4 on port 111, `/etc/RPCBTAB`.

use super::{DaemonEnv, Serve, Started};

/// Serve: register with the binder and answer requests, reporting
/// ready with what it registered.
pub fn serve(_env: &mut dyn DaemonEnv, _started: Started) -> Serve {
    // BLOCKED(rel): the RPC service of this daemon (NFS wave 2): the binder (portmap and rpcbind) protocol over net/nfs's RPC layer.
    Serve::Unavailable
}
