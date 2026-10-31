// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! slic: QS/400 PID 1. It runs the IPL, then starts itself again as the
//! storage service (`/init qsstg`), which owns the object store for the
//! rest of the IPL, as the hardware resource service (`/init qshwr`),
//! which owns the hardware resource table, and the job processes.
//!
//! `unsafe` is denied everywhere except `platform::sys`, which holds the
//! signal plumbing. (`forbid` cannot be scoped, so `deny` is used.)

#![deny(unsafe_code)]

#[cfg(target_os = "linux")]
mod cpp;
#[cfg(target_os = "linux")]
mod hwsvc;
#[cfg(target_os = "linux")]
mod ipl;
#[cfg(target_os = "linux")]
mod net;
#[cfg(target_os = "linux")]
mod platform;
#[cfg(target_os = "linux")]
mod procs;
#[cfg(target_os = "linux")]
mod src;
#[cfg(target_os = "linux")]
mod stgsvc;
#[cfg(target_os = "linux")]
mod svc;

#[cfg(target_os = "linux")]
fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("qsstg") => stgsvc::run(),
        Some("qshwr") => hwsvc::run(),
        _ => {}
    }
    ipl::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("slic runs only as PID 1 on Linux");
    std::process::exit(1);
}
