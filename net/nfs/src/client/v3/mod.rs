// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS version 3 mounts: the binder for the MOUNT and NFS ports (unless
//! `mountport=` and `port=` name them: then the binder is not asked),
//! MOUNT's `MNT` (and `UMNT` at the end), `FSINFO` and `PATHCONF` for
//! the sizes and properties, and every call of the remote file system
//! ([`fs`]) as one procedure.

pub mod attr;
mod fs;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use ifs::Errno;
use ifs::remote::{RHandle, RResult, RemoteError};

use super::Target;
use super::channel::{Channel, Status, rpc_errno};
use super::nlm::Locker;
use super::settings::{Policy, Settled, Version};
use crate::exports::mount::Proto as OptProto;
use crate::rpc::{ClientConfig, Credential, Proto};
use crate::rpcb::BinderClient;
use crate::types::mount::MOUNT_PROGRAM::MOUNT_V3;
use crate::types::mount::{dirpath, mountres3};
use crate::types::nfs3::NFS_PROGRAM::NFS_V3::{Args, Res};
use crate::types::nfs3::{FSINFO3args, FSINFO3res, PATHCONF3args, PATHCONF3res, nfs_fh3};

/// The most a datagram carries.
const UDP_MAX: u32 = 32 * 1024;

/// A version 3 mount.
pub struct V3 {
    nfs: Channel,
    mnt: Option<Channel>,
    path: Vec<u8>,
    root: RHandle,
    host: Vec<u8>,
    /// Who mounted: the root is asked and the mount ended as them.
    owner: Credential,
    status: Arc<Status>,
    locker: Option<Arc<Locker>>,
    plus: AtomicBool,
    policy: Policy,
    rsize: u32,
}

impl std::fmt::Debug for V3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V3")
            .field("nfs", &self.nfs)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

fn proto_num(p: Proto) -> u32 {
    match p {
        Proto::Tcp => 6,
        Proto::Udp => 17,
    }
}

/// The ports of NFS and MOUNT over `proto`: the options', else the
/// binder's (`None` when a program is not registered on it).
fn ports(t: &Target<'_>, proto: Proto, mount_policy: Policy) -> RResult<Option<(u16, u16)>> {
    let o = t.opts;
    let mut binder = None;
    let mut ask = |prog: u32| -> RResult<Option<u16>> {
        let b = binder.get_or_insert_with(|| {
            BinderClient::at(
                t.env.binder(t.ip),
                Proto::Udp,
                ClientConfig {
                    timeout: mount_policy.timeo,
                    retransmits: mount_policy.retrans,
                    reserved_port: false,
                    ..ClientConfig::default()
                },
            )
        });
        b.getport(prog, 3, proto_num(proto))
            .map_err(|e| rpc_errno(&e))
    };
    let nport = match o.port {
        Some(p) => p,
        None => match ask(crate::types::nfs3::NFS_PROGRAM::NUMBER)? {
            Some(p) => p,
            None => return Ok(None),
        },
    };
    let mport = match o.mountport {
        Some(p) => p,
        None => match ask(crate::types::mount::MOUNT_PROGRAM::NUMBER)? {
            Some(p) => p,
            None => return Ok(None),
        },
    };
    Ok(Some((nport, mport)))
}

/// Whether a mount over another transport may do better.
fn next_proto(e: RemoteError) -> bool {
    matches!(
        e,
        RemoteError::Errno(Errno::ECONNREFUSED | Errno::ECONNRESET | Errno::EPROTONOSUPPORT)
    )
}

/// Mount `t` with version 3.
pub(super) fn mount(t: &Target<'_>, status: &Arc<Status>) -> RResult<(V3, Settled)> {
    let o = t.opts;
    let protos: &[Proto] = match o.proto {
        Some(OptProto::Tcp) => &[Proto::Tcp],
        Some(OptProto::Udp) => &[Proto::Udp],
        None => &[Proto::Tcp, Proto::Udp],
    };
    let policy = Policy::of(o);
    let soft = Policy {
        soft: true,
        ..policy
    };
    let mut last = RemoteError::Errno(Errno::EPROTONOSUPPORT);
    for (i, &proto) in protos.iter().enumerate() {
        let more = i + 1 < protos.len();
        let r = ports(t, proto, soft).and_then(|p| match p {
            Some(p) => mount_over(t, status, proto, p, policy, soft),
            None => Err(RemoteError::Errno(Errno::EPROTONOSUPPORT)),
        });
        match r {
            Ok(m) => return Ok(m),
            Err(e) if more && next_proto(e) => last = e,
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

fn mount_over(
    t: &Target<'_>,
    status: &Arc<Status>,
    proto: Proto,
    (nport, mport): (u16, u16),
    policy: Policy,
    soft: Policy,
) -> RResult<(V3, Settled)> {
    let env = t.env;
    let host = env.host_name.as_bytes().to_vec();
    let cred = super::wire_cred(&host, t.cred);
    let chan = |port: u16, p: Policy| {
        Channel::new(
            SocketAddr::new(t.ip, port),
            proto,
            p,
            Arc::clone(status),
            env.reserved_port,
            env.connections,
        )
    };
    let mnt = chan(mport, soft);
    let root = match mnt.call::<MOUNT_V3::Args, MOUNT_V3::Res>(
        &cred,
        &MOUNT_V3::Args::MOUNTPROC3_MNT(dirpath(t.path.clone())),
    )? {
        MOUNT_V3::Res::MOUNTPROC3_MNT(mountres3::MNT3_OK(ok)) => {
            if !ok.auth_flavors.is_empty() && !ok.auth_flavors.contains(&1) {
                // AUTH_SYS is all this client sends.
                return Err(RemoteError::Errno(Errno::EACCES));
            }
            ok.fhandle.0
        }
        MOUNT_V3::Res::MOUNTPROC3_MNT(mountres3::default(s)) => {
            return Err(super::status::mnt3(s));
        }
        _ => return Err(RemoteError::Errno(Errno::EIO)),
    };
    let nfs_soft = chan(nport, soft);
    let fh = nfs_fh3 { data: root.clone() };
    let info = match nfs_soft.call::<Args, Res>(
        &cred,
        &Args::NFSPROC3_FSINFO(FSINFO3args { fsroot: fh.clone() }),
    )? {
        Res::NFSPROC3_FSINFO(FSINFO3res::NFS3_OK(ok)) => ok,
        Res::NFSPROC3_FSINFO(FSINFO3res::default(s, _)) => return Err(super::status::nfs3(s)),
        _ => return Err(RemoteError::Errno(Errno::EIO)),
    };
    let case_sensitive = match nfs_soft.call::<Args, Res>(
        &cred,
        &Args::NFSPROC3_PATHCONF(PATHCONF3args { object: fh }),
    )? {
        Res::NFSPROC3_PATHCONF(PATHCONF3res::NFS3_OK(ok)) => !ok.case_insensitive,
        _ => true,
    };
    let cap = |want: u32, max: u32| {
        let mut v = if max == 0 { want } else { want.min(max) };
        if proto == Proto::Udp {
            v = v.min(UDP_MAX);
        }
        v.max(512)
    };
    let rsize = cap(t.opts.rsize, info.rtmax);
    let wsize = cap(t.opts.wsize, info.wtmax);
    let locker = env
        .local
        .is_some()
        .then(|| Locker::new(env.clone(), t.ip, t.host, proto, policy, Arc::clone(status)));
    let settled = Settled {
        version: Version::V3,
        proto,
        port: nport,
        mountport: Some(mport),
        rsize,
        wsize,
        case_sensitive,
        locks: locker.is_some(),
    };
    let v = V3 {
        nfs: chan(nport, policy),
        mnt: Some(mnt),
        path: t.path.clone(),
        root: RHandle(root),
        owner: cred.clone(),
        host,
        status: Arc::clone(status),
        locker,
        plus: AtomicBool::new(true),
        policy,
        rsize,
    };
    Ok((v, settled))
}

impl V3 {
    /// The `AUTH_SYS` credential of an IFS caller.
    fn cred(&self, c: &ifs::remote::Cred) -> Credential {
        super::wire_cred(&self.host, c)
    }

    /// The lock client (none without this system's status monitor).
    #[must_use]
    pub fn locker(&self) -> Option<&Arc<Locker>> {
        self.locker.as_ref()
    }

    /// Where NFS is called.
    #[must_use]
    pub fn server(&self) -> SocketAddr {
        self.nfs.addr()
    }

    /// The server's address.
    #[must_use]
    pub fn ip(&self) -> IpAddr {
        self.nfs.addr().ip()
    }

    /// Tell MOUNT the mount ends (once, briefly) and end every call.
    fn end(&self) -> RResult<()> {
        let r = match &self.mnt {
            Some(m) => {
                let quick = m.with_policy(Policy {
                    soft: true,
                    retrans: 1,
                    ..self.policy
                });
                let cred = self.owner.clone();
                quick
                    .call::<MOUNT_V3::Args, MOUNT_V3::Res>(
                        &cred,
                        &MOUNT_V3::Args::MOUNTPROC3_UMNT(dirpath(self.path.clone())),
                    )
                    .map(|_| ())
            }
            None => Ok(()),
        };
        self.status.stop();
        r
    }
}
