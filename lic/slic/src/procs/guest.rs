// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `slic`'s side of the guest agent (`/init qsga`, `lic/guestagent`):
//! started once the IPL completes when the hypervisor gives the guest an
//! agent channel (the `org.qemu.guest_agent.0` port, or vsock), started
//! again should it end. A power down it asks for arrives as a job's
//! does: the request file and `SIGUSR1`.

use std::path::Path;

use super::spawn::{SELF, spawn};
use crate::ipl::system::System;

/// The running agent.
pub struct GuestProc {
    /// Its process ID.
    pub pid: u32,
    /// Kept open for the life of the agent.
    _chan: ipc::chan::Chan,
}

/// Start the agent if there is a channel for it.
pub fn start(system: &mut System) {
    let sys = Path::new("/sys");
    let port = guestagent::port::find(sys, guestagent::PORT_NAME);
    let vsock = guestagent::port::vsock(sys);
    if port.is_none() && !vsock {
        return;
    }
    match spawn(SELF, &["qsga"]) {
        Ok((pid, chan)) => {
            let on = match (&port, vsock) {
                (Some(p), true) => format!("/dev/{p} and vsock port {}", guestagent::VSOCK_PORT),
                (Some(p), false) => format!("/dev/{p}"),
                (None, _) => format!("vsock port {}", guestagent::VSOCK_PORT),
            };
            system.out.serial(&format!("slic: guest agent {pid} on {on}\n"));
            system.procs.guest = Some(GuestProc { pid, _chan: chan });
        }
        Err(e) => system.out.serial(&format!("slic: guest agent: {e}\n")),
    }
}

/// After a child exited: the agent again if it was the one.
pub fn reaped(system: &mut System, pid: u32) {
    if system.procs.guest.as_ref().is_some_and(|p| p.pid == pid) {
        system.procs.guest = None;
        system.out.serial(&format!("slic: guest agent ({pid}) ended; starting it again\n"));
        start(system);
    }
}
