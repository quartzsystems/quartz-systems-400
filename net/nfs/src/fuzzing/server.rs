// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The servers end to end for the fuzz targets: MOUNT, NFS version 3,
//! NLM and NSM over an `IfsVfs` on an IFS in memory (an export of
//! `/home/fz` holding a file and a directory), each input a call from
//! a client. Input: the procedure, a byte choosing the version, the
//! client (this host or another) and the credential, then the
//! arguments. Every `opaque<>` of 32 bytes `0xAA` in the arguments is
//! made the handle of the export's root, so mutations reach past the
//! handle check. A reply's results must decode as the version's
//! results of the procedure, else it panics. The system is made again
//! every [`RUNS`] inputs.

use std::cell::RefCell;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs};
use ifs::ops::audit::{FileId, MemAudit};
use ifs::ops::{Config, Ifs, OpenArgs, Owner};
use ifs::vfs::qsys::MemQsys;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;

use crate::mount::Mountd;
use crate::mount::list::MemMountList;
use crate::nlm::Nlm;
use crate::nlm::state::{NlmConfig, NlmState};
use crate::nsm::Nsm;
use crate::nsm::record::MemStatdStore;
use crate::nsm::state::{NsmConfig, NsmState};
use crate::rpc::msg::encode_call;
use crate::rpc::{ClientConfig, Credential, Dispatcher, LocalProgram, Transport, auth};
use crate::server::ifsvfs::{self, IfsVfs};
use crate::server::locate::FixedLocator;
use crate::server::{ExportTable, FileHandle, Gate, Instance, MemExport, MemExports, SystemClock};
use crate::types::rpc::call_body;
use crate::types::{mount, nfs3, nlm, nsm};
use crate::v3::Nfs3;
use crate::v4::{Nfs34, Server};
use crate::xdr::{Decoder, ProcBody};

/// Inputs served before the system is made again.
pub const RUNS: u32 = 512;

type Fs = IfsVfs<LocalIfs<MemBackend>>;

struct Sys {
    nfs3: Nfs3<Fs>,
    nfs4: Nfs34<Fs>,
    mountd: Mountd<Fs>,
    nlm: Nlm<Fs>,
    nsm: Nsm,
    nfs_d: Dispatcher,
    nlm_d: Dispatcher,
    plain: Dispatcher,
    root: [u8; 32],
    runs: u32,
}

fn principal(name: &str) -> Principal {
    let mut p = Principal {
        user: name.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if name == "QSECOFR" {
        p.spcaut = SpecialAuth::ALLOBJ.0 | SpecialAuth::IOSYSCFG.0;
    }
    p
}

#[allow(clippy::too_many_lines)]
fn build() -> Option<Sys> {
    let env = MemEnv::default()
        .user("QSECOFR", 0)
        .user("BOB", 100)
        .user("QDFTOWN", 104)
        .principal(principal("QSECOFR"));
    let ifs = Ifs::new(
        MemBackend::new(),
        Box::new(MemQsys::new(sec::auth::bits::Auth::USE)),
        Config::default(),
    );
    ifs.ensure_layout(&env).ok()?;
    let ifs = Arc::new(ifs);
    let job = || LocalIfs {
        ifs: Arc::clone(&ifs),
        env: Arc::new(env.clone()),
        audit: Arc::new(MemAudit::default()),
        principal: principal("QSECOFR"),
        owner: Owner(1),
        job_ccsid: 37,
    };
    let j = job();
    j.mkdir("/home/fz", 0o777).ok()?;
    j.chmod("/home/fz", 0o777).ok()?;
    j.mkdir("/home/fz/d", 0o777).ok()?;
    let fd = j
        .open("/home/fz/f", OpenArgs::new(0o2 | 0o10).mode(0o666))
        .ok()?;
    j.write(fd, b"fuzz data").ok()?;
    j.close(fd).ok()?;
    let st = j.stat("/home/fz", false).ok()?;
    let rid = FileId {
        fs: st.vfs,
        ino: st.ino,
        generation: st.ino_gen,
    };
    let exports = Arc::new(MemExports::new());
    exports.put(MemExport::new(1, "/home/fz", rid));
    let table: Arc<dyn ExportTable> = exports;
    let gate = Gate::new(Arc::clone(&table));
    let instance = Arc::new(Instance::new());
    let vfs = || IfsVfs::new(job(), Arc::clone(&instance), ifsvfs::Config::default());
    let rpc = ClientConfig {
        timeout: Duration::from_millis(1),
        retransmits: 0,
        reserved_port: false,
        ..ClientConfig::default()
    };
    let locator = Arc::new(FixedLocator::new());
    let locks = NlmState::new(NlmConfig {
        monitor: false,
        locator: Arc::clone(&locator) as _,
        rpc: rpc.clone(),
        grace: Duration::ZERO,
        grant_tries: 1,
        clock: Arc::new(SystemClock),
    });
    let (monitor, _) = NsmState::start(
        Box::new(MemStatdStore::default()),
        NsmConfig {
            me: "fuzz".into(),
            locator: locator as _,
            rpc,
            notify_rounds: 1,
            notify_pause: Duration::ZERO,
        },
    )
    .ok()?;
    // NFSv4 with no callbacks reachable and a short lease, so inputs
    // reach expiry and revocation too.
    let clock: Arc<dyn crate::server::Clock> = Arc::new(SystemClock);
    let v4 = Server::new(
        crate::v4::Config {
            lease: Duration::from_millis(5),
            grace: Duration::ZERO,
            courtesy: Duration::from_millis(5),
            recall_wait: Duration::from_millis(5),
            ..crate::v4::Config::default()
        },
        Arc::clone(&table),
        Arc::clone(&instance),
        Arc::clone(&clock),
        Arc::new(crate::registry::Registry::new(
            Arc::new(crate::registry::MemIdMap {
                domain: "fuzz".into(),
                users: vec![("BOB".into(), 100)],
                groups: Vec::new(),
            }),
            crate::registry::TTL,
            clock,
        )),
        Arc::new(crate::v4::state::stable::MemStable::new()),
        Arc::new(crate::v4::cb::NoCallbacks),
    );
    let mut nfs_d = Dispatcher::new();
    nfs_d.set_cache(crate::server::run::nfs3_cache());
    let mut nlm_d = Dispatcher::new();
    nlm_d.set_cache(crate::server::run::nlm_cache());
    Some(Sys {
        nfs3: Nfs3::new(vfs(), gate.clone()),
        nfs4: v4.program(vfs()),
        mountd: Mountd::new(vfs(), gate.clone(), Arc::new(MemMountList::new())),
        nlm: Nlm::new(vfs(), gate, locks),
        nsm: Nsm::new(monitor),
        nfs_d,
        nlm_d,
        plain: Dispatcher::new(),
        root: FileHandle {
            export: 1,
            fid: rid,
            root_ino: rid.ino,
        }
        .to_bytes(),
        runs: 0,
    })
}

thread_local! {
    static SYS: RefCell<Option<Sys>> = const { RefCell::new(None) };
}

/// Every `opaque<32>` of `0xAA` in `args` made `root`.
fn place_handles(args: &mut [u8], root: &[u8; 32]) {
    let mut i = 0;
    while i + 36 <= args.len() {
        if args[i..i + 4] == [0, 0, 0, 32] && args[i + 4..i + 36].iter().all(|b| *b == 0xAA) {
            args[i + 4..i + 36].copy_from_slice(root);
            i += 36;
        } else {
            i += 4;
        }
    }
}

/// Which program a call is for.
#[derive(Clone, Copy)]
pub(super) enum Prog {
    Mount,
    Nfs3,
    Nfs4,
    Nlm,
    Nsm,
}

/// One call of `procedure` at `vers` to `prog`; `R` the version's results.
pub(super) fn call<R: ProcBody + std::fmt::Debug>(
    prog: Prog,
    vers: u32,
    procedure: u32,
    sel: u8,
    args: &[u8],
) {
    SYS.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.as_ref().is_none_or(|s| s.runs >= RUNS) {
            *slot = build();
        }
        let Some(s) = slot.as_mut() else { return };
        s.runs += 1;
        let mut a = args.to_vec();
        place_handles(&mut a, &s.root);
        let cred = match sel >> 4 & 3 {
            0 => Credential::None,
            1 => Credential::sys(b"fuzz", 0, 0, &[]),
            _ => Credential::sys(b"fuzz", 100, 100, &[100]),
        };
        let peer: SocketAddr = if sel & 0x40 == 0 {
            SocketAddr::from(([127, 0, 0, 1], 700))
        } else {
            SocketAddr::from(([10, 0, 2, 2], 1023))
        };
        let (number, d, local): (u32, &Dispatcher, &dyn LocalProgram) = match prog {
            Prog::Mount => (mount::MOUNT_PROGRAM::NUMBER, &s.plain, &s.mountd),
            Prog::Nfs3 => (nfs3::NFS_PROGRAM::NUMBER, &s.nfs_d, &s.nfs3),
            Prog::Nfs4 => (nfs3::NFS_PROGRAM::NUMBER, &s.nfs_d, &s.nfs4),
            Prog::Nlm => (nlm::NLM_PROG::NUMBER, &s.nlm_d, &s.nlm),
            Prog::Nsm => (nsm::SM_PROG::NUMBER, &s.plain, &s.nsm),
        };
        let body = call_body {
            rpcvers: 2,
            prog: number,
            vers,
            proc: procedure,
            cred: cred.to_opaque().unwrap_or_else(|_| auth::none()),
            verf: auth::none(),
        };
        let Ok(m) = encode_call(u32::from(sel), &body, &a) else {
            return;
        };
        let Some(reply) = d.handle_with(&m, peer, Transport::Udp, &[local]) else {
            return;
        };
        if let Ok(res) = crate::rpc::client::results(&reply) {
            let mut dec = Decoder::new(&res);
            match R::decode_body(procedure, &mut dec) {
                Ok(Some(_)) if dec.is_empty() => {}
                other => panic!("the server's results do not decode: {other:?}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_builds_and_answers() {
        let s = build().expect("the fuzz system");
        let mut args = vec![0, 0, 0, 32];
        args.extend([0xAA; 32]);
        place_handles(&mut args, &s.root);
        assert_eq!(&args[4..], &s.root[..]);
        drop(s);
        // A GETATTR of the root through the whole path.
        call::<nfs3::NFS_PROGRAM::NFS_V3::Res>(Prog::Nfs3, 3, 1, 0x20, &args);
        SYS.with(|c| assert_eq!(c.borrow().as_ref().map(|s| s.runs), Some(1)));
    }
}
