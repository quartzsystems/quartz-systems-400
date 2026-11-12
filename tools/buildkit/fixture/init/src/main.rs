// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSDEMO's PID 1: mount `/proc`, say who we are on the console, power
//! off. The kernel has the console open as our standard output.

use std::ffi::CStr;
use std::io::Write as _;

use rustix::mount::{MountFlags, mount};
use rustix::system::{RebootCommand, reboot};

/// The release `build.rs` captured from the fixture's own `VERSION`.
const RELEASE: vrm::Release = vrm::release_from_env!("QSDEMO_");

fn main() {
    let _ = std::fs::create_dir_all("/proc");
    let _ = mount("proc", "/proc", "proc", MountFlags::empty(), None::<&CStr>);
    println!("QSDEMO {RELEASE} {}", env!("QSDEMO_BUILD_ID"));
    let _ = std::io::stdout().flush();
    rustix::fs::sync();
    let _ = reboot(RebootCommand::PowerOff);
    // PID 1 must not return.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
