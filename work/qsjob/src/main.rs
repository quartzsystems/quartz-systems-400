// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! qsjob: one job. Its parent passes the control channel as standard
//! input and says who the job is; the job connects to the storage
//! service, which holds everything it reads and writes, and runs its
//! programs here, in its own address space. A job that crashes takes
//! nothing with it: the service releases what it held.

#![deny(unsafe_code)]

#[cfg(target_os = "linux")]
mod command;
#[cfg(target_os = "linux")]
mod console;
#[cfg(target_os = "linux")]
mod devtools;
#[cfg(target_os = "linux")]
mod display;
#[cfg(target_os = "linux")]
mod panels;
#[cfg(target_os = "linux")]
mod platform;
#[cfg(target_os = "linux")]
mod start;
#[cfg(target_os = "linux")]
mod work;

#[cfg(target_os = "linux")]
fn main() {
    start::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("qsjob runs only under slic on Linux");
    std::process::exit(1);
}
