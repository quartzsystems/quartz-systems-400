// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QNFSNLMD`: the lock manager, NLM versions 1, 3 and 4 on a port the
//! system gives, registered with the binder (version 2, the monitor's
//! callback, is served but not registered). Blocked locks are granted
//! through the IFS's news (`lease_orders`) on a thread of their own;
//! the grace period begins at the start. `RLSIFSLCK RMTLOCNAME` reaches
//! it as an order the job takes at its pauses: the host's locks, waits
//! and shares go, and the job log says so.

use std::cell::OnceCell;
use std::sync::Arc;
use std::time::Duration;

use ifscmd::Msg;
use nfs::nlm::state::{GRACE, LockClient, NlmConfig, NlmState};
use nfs::rpc::ClientConfig;
use nfs::server::news::{ClientNews, NewsSource};
use nfs::server::run::{self, Common};
use nfs::server::{ExportTable, IfsVfs, Instance, SystemClock};
use nfs::types::nlm::NLM_PROG;
use qir::MsgType;
use stg::nfs::{NfsOp, NfsReply};

use super::host::{Host, thread_vfs};
use super::running::{self, Fail, Prog, Running};
use super::view::{self, JobExports};
use super::{DaemonEnv, Serve, Started};

/// NLM's registered versions.
pub const PROG: Prog = Prog {
    number: NLM_PROG::NUMBER,
    versions: &[1, 3, 4],
};

/// The lock manager's settings on this system.
fn config<H: Host>(host: &H) -> NlmConfig {
    NlmConfig {
        monitor: true,
        locator: host.locator(),
        rpc: ClientConfig {
            timeout: Duration::from_secs(2),
            retransmits: 2,
            ..ClientConfig::default()
        },
        grace: GRACE,
        grant_tries: 3,
        clock: Arc::new(SystemClock),
    }
}

/// The mounting client's side of this lock manager (`NlmState::set_client`):
/// what a server this system mounts sends back, its restart notices among
/// them. None yet: `NLM_GRANTED` to this host is then `DENIED` and the
/// `_RES` messages `PROC_UNAVAIL` (docs/NFS.md section 5.8).
fn lock_client<H: Host>(_host: &Arc<H>) -> Option<Arc<dyn LockClient>> {
    // BLOCKED(rel): the NFS client's restart notices through the lock manager (NFS-7): a LockClient whose restarted() sends NfsOp::ServerRestarted for a notice tagged nfs::client::nlm::PRIV_TAG.
    None
}

/// Carry out the release orders queued for this job.
fn orders<H: Host>(
    env: &mut dyn DaemonEnv,
    state: &NlmState,
    vfs: &OnceCell<IfsVfs<H::Fs>>,
    host: &H,
    instance: &Arc<Instance>,
) {
    let NfsReply::Orders { orders } = env.op(NfsOp::Orders) else {
        return;
    };
    for o in orders {
        let v = vfs.get_or_init(|| thread_vfs(host, instance));
        let n = state.release_client(v, o.host.as_bytes());
        env.log(&Msg::path("CPCA1C9", &o.host), MsgType::Comp);
        env.op(NfsOp::OrderDone {
            id: o.id,
            count: n,
            errno: 0,
        });
    }
}

/// Serve: bind, register, answer locks, grant the blocked ones.
pub fn serve<H: Host>(_env: &mut dyn DaemonEnv, _started: Started, host: &Arc<H>) -> Serve {
    let net = host.net();
    let listeners = match running::bind(net, 0) {
        Ok(l) => l,
        Err(f) => return Serve::Unavailable(f),
    };
    let exports = JobExports::new(host.names());
    match host.fs() {
        Ok(fs) => {
            view::refresh(&**host, &exports, &fs);
        }
        Err(e) => return Serve::Unavailable(Fail::new(e)),
    }
    let regs = running::entries(&listeners, PROG);
    if let Err(f) = running::register(net, &regs) {
        return Serve::Unavailable(f);
    }
    let state = NlmState::new(config(&**host));
    if let Some(c) = lock_client(host) {
        state.set_client(c);
    }
    let instance = Arc::new(Instance::new());
    let h = Arc::clone(host);
    let (st, inst) = (Arc::clone(&state), Arc::clone(&instance));
    let r = Running::spawn("nfs-nlmd", move |stop, ready| {
        let table: Arc<dyn ExportTable> = Arc::clone(&exports) as Arc<dyn ExportTable>;
        std::thread::scope(|sc| {
            sc.spawn(|| view::keep_current(&*h, &exports, stop));
            let c = Common {
                listeners,
                binder: None,
                stop,
                ready: Some(ready),
            };
            let news = || -> Box<dyn NewsSource> {
                let fs = h.fs().unwrap_or_else(|e| {
                    panic!("the lock manager's news thread has no link to the storage service: {e}")
                });
                Box::new(ClientNews(fs))
            };
            run::nlmd(c, &table, &st, || thread_vfs(&*h, &inst), news)
                .map_err(|e| Fail::io("NLM", &e))
        })
    });
    let ports = running::ports(&regs);
    match r {
        Ok(r) => {
            let vfs = OnceCell::new();
            let h = Arc::clone(host);
            Serve::Ready(
                r.with_ports(ports)
                    .with_tick(move |env| orders(env, &state, &vfs, &*h, &instance))
                    .on_end(move || running::unregister(net, &regs)),
            )
        }
        Err(f) => {
            running::unregister(net, &regs);
            Serve::Unavailable(f)
        }
    }
}
