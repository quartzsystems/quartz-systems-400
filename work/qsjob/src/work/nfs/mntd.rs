// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QNFSMNTD`: the MOUNT protocol, versions 1 and 3.

use super::{DaemonEnv, Serve, Started};

/// Serve: register with the binder and answer requests, reporting
/// ready with what it registered.
pub fn serve(_env: &mut dyn DaemonEnv, _started: Started) -> Serve {
    // BLOCKED(rel): the RPC service of this daemon (NFS wave 2): the MOUNT protocol over net/nfs's RPC layer.
    Serve::Unavailable
}
