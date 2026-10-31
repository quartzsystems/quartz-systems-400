// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `slic`'s side of the hardware resource service: start it before
//! anything asks about hardware, start it again should it end.

use std::time::Duration;

use ipc::chan::Chan;

use super::spawn::{SELF, spawn};
use crate::hwsvc::Ready;
use crate::ipl::system::System;

/// The running service.
pub struct HwrProc {
    /// Its process ID.
    pub pid: u32,
    /// Kept open for the life of the service.
    _chan: Chan,
}

/// Start the service and wait until its table is built and its socket
/// listens.
pub fn start(system: &mut System) -> Result<(), String> {
    let (pid, mut chan) = spawn(SELF, &["qshwr"])?;
    chan.set_timeout(Some(Duration::from_secs(60)))
        .map_err(|e| e.to_string())?;
    let (count, hotplug): Ready = chan
        .recv()
        .map_err(|e| format!("hardware resource service: {e}"))?;
    let _ = chan.set_timeout(None);
    system.out.serial(&format!(
        "slic: hardware resource service {pid}: {count} resources{}\n",
        if hotplug { "" } else { "; uevent socket unavailable, no hotplug" }
    ));
    system.procs.hwr = Some(HwrProc { pid, _chan: chan });
    Ok(())
}

/// After a child exited: the service again if it was the one.
pub fn reaped(system: &mut System, pid: u32) {
    if system.procs.hwr.as_ref().is_some_and(|p| p.pid == pid) {
        system.procs.hwr = None;
        system
            .out
            .serial(&format!("slic: hardware resource service ({pid}) ended; starting it again\n"));
        if let Err(e) = start(system) {
            system.out.serial(&format!("slic: {e}\n"));
        }
    }
}

