// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QNFSMNTD`: MOUNT versions 1 and 3 on a port the system gives (TCP
//! and UDP), registered with the binder. The mount list is kept in
//! memory and starts empty with each start: IBM keeps no `rmtab`.

use std::sync::Arc;

use nfs::mount::list::{MemMountList, MountList};
use nfs::server::run::{self, Common};
use nfs::server::{ExportTable, Instance};
use nfs::types::mount::MOUNT_PROGRAM;

use super::host::{Host, thread_vfs};
use super::running::{self, Fail, Prog, Running};
use super::view::{self, JobExports};
use super::{DaemonEnv, Serve, Started};

/// MOUNT's program and versions.
pub const PROG: Prog = Prog {
    number: MOUNT_PROGRAM::NUMBER,
    versions: &[1, 3],
};

/// Serve: bind, register, answer `MNT`, `DUMP`, `UMNT`, `EXPORT`.
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
    let h = Arc::clone(host);
    let instance = Arc::new(Instance::new());
    let list: Arc<dyn MountList> = Arc::new(MemMountList::new());
    let r = Running::spawn("nfs-mntd", move |stop, ready| {
        let table: Arc<dyn ExportTable> = Arc::clone(&exports) as Arc<dyn ExportTable>;
        std::thread::scope(|sc| {
            sc.spawn(|| view::keep_current(&*h, &exports, stop));
            let c = Common {
                listeners,
                binder: None,
                stop,
                ready: Some(ready),
            };
            run::mountd(c, &table, &list, || thread_vfs(&*h, &instance))
                .map_err(|e| Fail::io("MOUNT", &e))
        })
    });
    let ports = running::ports(&regs);
    match r {
        Ok(r) => Serve::Ready(
            r.with_ports(ports)
                .on_end(move || running::unregister(net, &regs)),
        ),
        Err(f) => {
            running::unregister(net, &regs);
            Serve::Unavailable(f)
        }
    }
}
