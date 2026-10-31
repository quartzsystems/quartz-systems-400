// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The hardware resource service, `/init qshwr`: the one process that
//! walks sysfs. It names the resources at start, follows hotplug over
//! `NETLINK_KOBJECT_UEVENT` and answers the `hwrsrc` protocol on its
//! socket for `slic`, DST, the installer, the storage service and,
//! through it, every job. `slic` starts it first thing in the LIC phase
//! (`procs/hwr.rs`).

mod serve;
mod uevent;

use std::path::Path;
use std::sync::{Arc, Mutex};

use hwrsrc::Table;
use ipc::chan::Chan;

/// What the service tells `slic` once it is ready: resources found,
/// whether hotplug events are followed.
pub type Ready = (u32, bool);

/// The sysfs root.
const SYS: &str = "/sys";

/// Run as the service. Never returns.
pub fn run() -> ! {
    let mut parent = match Chan::from_stdin() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("qshwr: no channel to slic: {e}");
            std::process::exit(1);
        }
    };
    let table = Arc::new(Mutex::new(Table::scan(Path::new(SYS))));
    let count = u32::try_from(table.lock().map_or(0, |t| t.resources.len())).unwrap_or(u32::MAX);
    let hotplug = uevent::spawn(Arc::clone(&table));
    let listener = match serve::bind() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("qshwr: {}: {e}", hwrsrc::client::SOCKET);
            std::process::exit(1);
        }
    };
    let _ = parent.send(&(count, hotplug));
    serve::accept(&listener, &table)
}
