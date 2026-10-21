// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `slic`'s side of the storage service: hand the store over at the end
//! of the IPL, ask the service to close it at power down.

use ipc::WireError;
use ipc::chan::Chan;
use rustix::process::{Pid, WaitOptions, waitpid};
use stg::{Client, Request, Start};

use super::spawn::{SELF, spawn};
use crate::ipl::system::System;
use crate::src;

/// The running service.
pub struct StgProc {
    /// Its process ID.
    pub pid: u32,
    chan: Chan,
    /// Set while power down closes it, so its exit is expected.
    pub closing: bool,
}

impl Client for StgProc {
    fn call(&mut self, req: &Request) -> Result<Vec<u8>, WireError> {
        self.chan.send(req)?;
        self.chan.recv_raw()
    }
}

/// Close the store here and start the service on the same units. The
/// service reopens them; the checkpoint just taken leaves it nothing
/// to replay.
pub fn start(system: &mut System) -> Result<(), String> {
    let storage = system.storage.take().ok_or("the object store is not open")?;
    let devices: Vec<String> = storage
        .devices
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    storage
        .store
        .close()
        .map_err(|e| format!("closing the store for the service: {e}"))?;
    system.temp = None;
    let (pid, mut chan) = spawn(SELF, &["qsstg"])?;
    chan.send(&Start {
        devices,
        sysvals: system.sysvals.as_ref().map(sysval::SysVals::to_blob),
        system: system
            .sysvals
            .as_ref()
            .map(sysval::SysVals::system_blob)
            .unwrap_or_default(),
    })
    .map_err(|e| format!("starting the storage service: {e}"))?;
    let opened: Result<String, String> = chan
        .recv()
        .map_err(|e| format!("starting the storage service: {e}"))?;
    let text = opened?;
    system.out.serial(&format!("slic: storage service {pid}: {text}\n"));
    system.procs.stg = Some(StgProc {
        pid,
        chan,
        closing: false,
    });
    system.emit(&src::STG_STARTED);
    Ok(())
}

/// D1001010: the service checkpoints, closes the store and exits.
pub fn close(system: &mut System) {
    let Some(mut p) = system.procs.stg.take() else {
        return;
    };
    p.closing = true;
    match p.close() {
        Ok(Ok(text)) => {
            system.out.serial(&format!("slic: {text}\n"));
            system.emit(&src::STORE_CLOSED);
        }
        Ok(Err(e)) => system.out.serial(&format!("slic: close store: {e}\n")),
        Err(e) => system.out.serial(&format!("slic: close store: {e}\n")),
    }
    // The answer comes before the exit; wait for it, so nothing writes
    // the units after power down goes on.
    if let Some(pid) = Pid::from_raw(i32::try_from(p.pid).unwrap_or(0)) {
        let _ = waitpid(Some(pid), WaitOptions::empty());
    }
}
