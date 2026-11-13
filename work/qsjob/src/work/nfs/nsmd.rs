// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QNFSNSMD`: the status monitor, NSM version 1 on a port the system
//! gives, registered with the binder. Each start moves the state number
//! in `/etc/STATD` to the next odd one and tells the hosts monitored
//! before (`SM_NOTIFY`) that this one restarted.

use std::sync::Arc;
use std::time::Duration;

use nfs::nsm::state::{NsmConfig, NsmState};
use nfs::rpc::ClientConfig;
use nfs::server::run::{self, Common};
use nfs::types::nsm::SM_PROG;

use super::host::Host;
use super::running::{self, Fail, Prog, Running};
use super::statd::IfsStatd;
use super::{DaemonEnv, Serve, Started};

/// NSM's program and version.
pub const PROG: Prog = Prog {
    number: SM_PROG::NUMBER,
    versions: &[1],
};

/// How hosts are told of a restart: five rounds, five seconds apart.
fn config<H: Host>(host: &H) -> NsmConfig {
    NsmConfig {
        me: host.host_name(),
        locator: host.locator(),
        rpc: ClientConfig {
            timeout: Duration::from_secs(2),
            retransmits: 1,
            ..ClientConfig::default()
        },
        notify_rounds: 5,
        notify_pause: Duration::from_secs(5),
    }
}

/// Serve: the state moved up, bind, register, answer, tell.
pub fn serve<H: Host>(_env: &mut dyn DaemonEnv, _started: Started, host: &Arc<H>) -> Serve {
    let net = host.net();
    let listeners = match running::bind(net, 0) {
        Ok(l) => l,
        Err(f) => return Serve::Unavailable(f),
    };
    let store = Box::new(IfsStatd::new(Arc::clone(host)));
    let (state, before) = match NsmState::start(store, config(&**host)) {
        Ok(s) => s,
        Err(e) => return Serve::Unavailable(Fail::io("the status monitor's files", &e)),
    };
    let regs = running::entries(&listeners, PROG);
    if let Err(f) = running::register(net, &regs) {
        return Serve::Unavailable(f);
    }
    let r = Running::spawn("nfs-nsmd", move |stop, ready| {
        let c = Common {
            listeners,
            binder: None,
            stop,
            ready: Some(ready),
        };
        run::nsmd(c, &state, before).map_err(|e| Fail::io("NSM", &e))
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
