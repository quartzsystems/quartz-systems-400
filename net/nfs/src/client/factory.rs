// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the IFS calls to mount `TYPE(*NFS)`: the options parsed, the
//! system's environment asked at each mount.

use ifs::Errno;
use ifs::remote::{Cred, MountCcsid, Mounted, RResult, RemoteError, RemoteFactory};

use super::env::ClientEnv;

/// The environment of a mount, asked when it is made.
pub type EnvSource = Box<dyn Fn() -> ClientEnv + Send + Sync>;

/// The NFS client as the IFS's factory of `*NFS` mounts.
pub struct Factory {
    env: EnvSource,
}

impl std::fmt::Debug for Factory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Factory").finish_non_exhaustive()
    }
}

impl Factory {
    /// A factory whose mounts are made in the environment `env` gives.
    #[must_use]
    pub fn new(env: EnvSource) -> Self {
        Self { env }
    }

    /// A factory with a fixed environment.
    #[must_use]
    pub fn fixed(env: ClientEnv) -> Self {
        Self::new(Box::new(move || env.clone()))
    }
}

impl RemoteFactory for Factory {
    fn supports(&self, kind: &str) -> bool {
        kind.trim().eq_ignore_ascii_case("*NFS")
    }

    fn mount(
        &self,
        kind: &str,
        mfs: &str,
        options: &str,
        ccsid: MountCcsid,
        cred: &Cred,
    ) -> RResult<Mounted> {
        if !self.supports(kind) {
            return Err(RemoteError::Errno(Errno::ENOTSUP));
        }
        // `MOUNT` checked the list with its messages before; here it is
        // only taken apart.
        let o =
            crate::exports::mount::parse(options).map_err(|_| RemoteError::Errno(Errno::EINVAL))?;
        super::mount(mfs, &o, ccsid, cred, &(self.env)())
    }
}
