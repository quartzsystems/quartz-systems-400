// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! slic: QS/400 PID 1. Phase 0 scope: mount the base filesystems, announce
//! the IPL with an SRC, heartbeat on serial, reap children, never exit.
//!
//! `unsafe` is denied everywhere except `platform::sys`, which holds the
//! signal plumbing. (`forbid` cannot be scoped, so `deny` is used.)

#![deny(unsafe_code)]

#[cfg(target_os = "linux")]
mod codes;
#[cfg(target_os = "linux")]
mod ipl;
#[cfg(target_os = "linux")]
mod platform;

#[cfg(target_os = "linux")]
fn main() {
    ipl::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("slic runs only as PID 1 on Linux");
    std::process::exit(1);
}
