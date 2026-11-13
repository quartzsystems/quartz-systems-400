// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A mounted export as a remote file system (docs/NFS.md section 7.7):
//! [`mount`] makes a [`Mount`], an [`ifs::remote::RemoteFs`] over NFS
//! version 3 ([`v3`], with byte-range locks through the server's lock
//! manager, [`nlm`]) or 4.0 and 4.1 ([`v4`]). Calls follow `timeo`,
//! `retrans`, `soft`/`hard` and `intr` ([`channel`]); the export path
//! is sent in the mount's path name CCSID, every other name is the
//! IFS's to convert; data conversion is the IFS's. A server's restart
//! reaches the mounts' lock clients through [`restart`].

pub mod channel;
pub mod env;
pub mod factory;
pub mod nlm;
pub mod ranges;
pub mod restart;
pub mod settings;
pub mod status;
pub mod v3;
pub mod v4;

use std::net::IpAddr;
use std::sync::Arc;

use ifs::Errno;
use ifs::remote::{
    CcsidSel, CreateMode, Cred, DirPage, FsStat, LockConflict, LockKind, LockOwner, MountCcsid,
    Mounted, OpenToken, PathConf, RAttr, RHandle, RKind, RResult, RemoteError, RemoteFs,
    RemoteInfo, SetAttr, Share, Stable, WriteReply,
};

pub use env::{ClientEnv, Hook, Hooks, Resolver, SystemResolver};
pub use factory::Factory;
pub use settings::{Policy, Version};

use crate::exports::mount::MountOptions;
use crate::rpc::Credential;
use channel::Status;

/// The most groups `AUTH_SYS` carries.
const MAX_GIDS: usize = 16;

/// An IFS caller as `AUTH_SYS` from this system (`host`).
#[must_use]
pub fn wire_cred(host: &[u8], c: &Cred) -> Credential {
    let gids: Vec<u32> = c.gids.iter().copied().take(MAX_GIDS).collect();
    let host = &host[..host.len().min(255)];
    Credential::sys(host, c.uid, c.gid, &gids)
}

/// What one mount attempt works on.
pub(crate) struct Target<'a> {
    /// The server as `MFS` names it.
    pub host: &'a str,
    /// Its address.
    pub ip: IpAddr,
    /// The export path, in the server's path name CCSID.
    pub path: Vec<u8>,
    pub opts: &'a MountOptions,
    pub env: &'a ClientEnv,
    pub cred: &'a Cred,
}

/// The export path in the mount's path name CCSID: UTF-8 or a
/// single-byte CCSID; a double-byte or mixed one is `ENOTSUP`.
///
/// # Errors
///
/// `ENOTSUP` for such a CCSID, `ECONVERT` for a path it cannot hold.
pub fn path_bytes(path: &str, ccsid: MountCcsid) -> RResult<Vec<u8>> {
    let c = match ccsid.path {
        CcsidSel::Ccsid(n) => n,
        // The IFS resolves the others first; these are its defaults.
        CcsidSel::Binary | CcsidSel::Ascii | CcsidSel::Job => ccsid::conv::UTF8,
    };
    if c == ccsid::conv::UTF8 || c == ccsid::conv::BINARY {
        return Ok(path.as_bytes().to_vec());
    }
    if double_byte(c) {
        return Err(RemoteError::Errno(Errno::ENOTSUP));
    }
    ccsid::conv::from_str(c, path).map_err(|_| RemoteError::Errno(Errno::ECONVERT))
}

/// Whether a CCSID is double-byte or mixed (refused for names; for
/// data, `MOUNT` refuses it with `CPDA1CA`).
#[must_use]
pub fn double_byte(c: u16) -> bool {
    matches!(c, 1200 | 13488 | 17584 | 300)
        || matches!(ccsid::conv::Codec::of(c), Ok(Some(k)) if k.is_mixed())
}

/// A mounted export.
pub struct Mount {
    inner: Inner,
    status: Arc<Status>,
    info: RemoteInfo,
}

enum Inner {
    V3(Box<v3::V3>),
    V4(v4::V4),
}

impl std::fmt::Debug for Mount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mount")
            .field("source", &self.info.source)
            .field("options", &self.info.options)
            .finish_non_exhaustive()
    }
}

/// The transport conditions worth another mount attempt (`retry=`).
fn again(e: RemoteError) -> bool {
    matches!(
        e,
        RemoteError::TimedOut
            | RemoteError::Errno(
                Errno::ECONNREFUSED | Errno::ECONNRESET | Errno::EHOSTUNREACH | Errno::ENETUNREACH
            )
    )
}

/// Mount `mfs` (`host:/path`) with `opts`, as `cred`.
///
/// # Errors
///
/// `EINVAL` for an `MFS` not `host:/path`, `EHOSTUNREACH` for a host
/// with no address, the server's refusal (`EACCES`, `ENOENT` ...), or,
/// after `retry` attempts, the transport's (`ETIMEDOUT`,
/// `ECONNREFUSED`).
pub fn connect(
    mfs: &str,
    opts: &MountOptions,
    ccsid: MountCcsid,
    cred: &Cred,
    env: &ClientEnv,
) -> RResult<Mount> {
    let (host, path) =
        crate::exports::mount::parse_mfs(mfs).map_err(|_| RemoteError::Errno(Errno::EINVAL))?;
    let path = path_bytes(&path, ccsid)?;
    let ips = env::addresses(&*env.resolver, &host);
    let ip = *ips.first().ok_or(RemoteError::Errno(Errno::EHOSTUNREACH))?;
    let policy = Policy::of(opts);
    let t = Target {
        host: &host,
        ip,
        path,
        opts,
        env,
        cred,
    };
    let attempts = opts.retry.max(1);
    let mut n = 0;
    loop {
        n += 1;
        match attempt(&t, mfs) {
            Ok(m) => return Ok(m),
            Err(e) if again(e) && n < attempts => {
                tracing::info!(?e, attempt = n, mfs, "mount attempt failed, trying again");
                std::thread::sleep(policy.timeo);
            }
            Err(e) => return Err(e),
        }
    }
}

fn attempt(t: &Target<'_>, source: &str) -> RResult<Mount> {
    let mut last = RemoteError::Errno(Errno::EPROTONOSUPPORT);
    for &v in settings::versions(t.opts.vers) {
        let status = Status::new(t.env.hooks.clone(), t.opts.intr);
        let r = match v {
            Version::V3 => v3::mount(t, &status).map(|(m, s)| (Inner::V3(Box::new(m)), s)),
            Version::V40 | Version::V41 => {
                v4::mount(t, &status, v == Version::V41).map(|(m, s)| (Inner::V4(m), s))
            }
        };
        match r {
            Ok((inner, s)) => {
                let info = settings::info(t.opts, &s, source);
                return Ok(Mount {
                    inner,
                    status,
                    info,
                });
            }
            // `vers=4`: 4.1 refused, 4.0 next.
            Err(RemoteError::Errno(Errno::EPROTONOSUPPORT)) => {
                last = RemoteError::Errno(Errno::EPROTONOSUPPORT);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

/// Mount `mfs` as the IFS's factory does: the file system and what the
/// IFS is told.
///
/// # Errors
///
/// As [`connect`].
pub fn mount(
    mfs: &str,
    opts: &MountOptions,
    ccsid: MountCcsid,
    cred: &Cred,
    env: &ClientEnv,
) -> RResult<Mounted> {
    let m = connect(mfs, opts, ccsid, cred, env)?;
    let info = m.info.clone();
    Ok(Mounted {
        fs: Box::new(m),
        info,
    })
}

impl Mount {
    /// What the IFS is told.
    #[must_use]
    pub fn info(&self) -> &RemoteInfo {
        &self.info
    }

    /// Whether the server answers.
    #[must_use]
    pub fn responding(&self) -> bool {
        self.status.responding()
    }

    /// `intr`: calls waiting now end `Interrupted` (nothing without
    /// `intr`). Whether one was waiting.
    pub fn interrupt(&self) -> bool {
        self.status.interrupt()
    }

    /// The version in use.
    #[must_use]
    pub fn version(&self) -> Version {
        match &self.inner {
            Inner::V3(_) => Version::V3,
            Inner::V4(m) => m.version(),
        }
    }

    /// Byte-range locks held on the server through this mount.
    #[must_use]
    pub fn locks_held(&self) -> usize {
        match &self.inner {
            Inner::V3(m) => m.locker().map_or(0, |l| l.held()),
            Inner::V4(m) => m.locks_held(),
        }
    }
}

macro_rules! each {
    ($s:ident . $f:ident ( $($a:expr),* )) => {
        match &$s.inner {
            Inner::V3(m) => m.$f($($a),*),
            Inner::V4(m) => m.$f($($a),*),
        }
    };
}

impl RemoteFs for Mount {
    fn root(&self) -> RResult<(RHandle, RAttr)> {
        each!(self.root())
    }
    fn getattr(&self, cred: &Cred, h: &RHandle) -> RResult<RAttr> {
        each!(self.getattr(cred, h))
    }
    fn setattr(
        &self,
        cred: &Cred,
        h: &RHandle,
        set: &SetAttr,
        guard: Option<i64>,
    ) -> RResult<RAttr> {
        each!(self.setattr(cred, h, set, guard))
    }
    fn lookup(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<(RHandle, RAttr)> {
        each!(self.lookup(cred, dir, name))
    }
    fn access(&self, cred: &Cred, h: &RHandle, mask: u32) -> RResult<u32> {
        each!(self.access(cred, h, mask))
    }
    fn readlink(&self, cred: &Cred, h: &RHandle) -> RResult<Vec<u8>> {
        each!(self.readlink(cred, h))
    }
    fn read(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<(Vec<u8>, bool)> {
        each!(self.read(cred, h, off, len))
    }
    fn write(
        &self,
        cred: &Cred,
        h: &RHandle,
        off: u64,
        data: &[u8],
        stable: Stable,
    ) -> RResult<WriteReply> {
        each!(self.write(cred, h, off, data, stable))
    }
    fn commit(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<u64> {
        each!(self.commit(cred, h, off, len))
    }
    fn create(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        how: CreateMode,
    ) -> RResult<(RHandle, RAttr)> {
        each!(self.create(cred, dir, name, how))
    }
    fn mkdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        each!(self.mkdir(cred, dir, name, set))
    }
    fn symlink(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        target: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        each!(self.symlink(cred, dir, name, target, set))
    }
    fn mknod(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        kind: RKind,
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        each!(self.mknod(cred, dir, name, kind, set))
    }
    fn remove(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<()> {
        each!(self.remove(cred, dir, name))
    }
    fn rmdir(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<()> {
        each!(self.rmdir(cred, dir, name))
    }
    fn rename(
        &self,
        cred: &Cred,
        from_dir: &RHandle,
        from: &[u8],
        to_dir: &RHandle,
        to: &[u8],
    ) -> RResult<()> {
        each!(self.rename(cred, from_dir, from, to_dir, to))
    }
    fn link(&self, cred: &Cred, h: &RHandle, dir: &RHandle, name: &[u8]) -> RResult<()> {
        each!(self.link(cred, h, dir, name))
    }
    fn readdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        cookie: u64,
        verifier: u64,
        max: u32,
    ) -> RResult<DirPage> {
        each!(self.readdir(cred, dir, cookie, verifier, max))
    }
    fn statfs(&self, cred: &Cred, h: &RHandle) -> RResult<FsStat> {
        each!(self.statfs(cred, h))
    }
    fn pathconf(&self, cred: &Cred, h: &RHandle) -> RResult<PathConf> {
        each!(self.pathconf(cred, h))
    }
    fn open(&self, cred: &Cred, h: &RHandle, share: Share, owner: LockOwner) -> RResult<OpenToken> {
        each!(self.open(cred, h, share, owner))
    }
    fn close(&self, cred: &Cred, h: &RHandle, token: &OpenToken) -> RResult<()> {
        each!(self.close(cred, h, token))
    }
    fn lock(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
        token: &OpenToken,
    ) -> RResult<()> {
        each!(self.lock(cred, h, owner, kind, off, len, token))
    }
    fn unlock(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        off: u64,
        len: u64,
    ) -> RResult<()> {
        each!(self.unlock(cred, h, owner, off, len))
    }
    fn test(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
    ) -> RResult<Option<LockConflict>> {
        each!(self.test(cred, h, owner, kind, off, len))
    }
    fn unmount(&self) -> RResult<()> {
        each!(self.unmount())
    }
}
