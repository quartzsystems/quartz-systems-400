// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! slic: QS/400 PID 1. Phase 1: a real IPL with persistent state in the LIC
//! partition, SRC progression, a serial service console and power down.
//!
//! `unsafe` is denied everywhere except `platform::sys`, which holds the
//! signal plumbing. (`forbid` cannot be scoped, so `deny` is used.)

#![deny(unsafe_code)]

#[cfg(target_os = "linux")]
mod ipl;
#[cfg(target_os = "linux")]
mod platform;
#[cfg(target_os = "linux")]
mod src;
#[cfg(target_os = "linux")]
mod svc;

#[cfg(target_os = "linux")]
fn main() {
    ipl::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("slic runs only as PID 1 on Linux");
    std::process::exit(1);
}
