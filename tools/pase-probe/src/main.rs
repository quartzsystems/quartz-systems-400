// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! pase-probe: what a program run through the PASE mechanism sees. It
//! prints its user and group IDs, its process ID, how many processes
//! its `/proc` shows, the names in `/` and `/QOpenSys/tmp`, whether
//! `/home` is there, and what a forbidden system call (`pidfd_open`)
//! and an allowed one (`getrandom` through `std`) answered. The test
//! harness builds it for the musl target and places it in the IFS; it
//! is never part of the image.

#[cfg(target_os = "linux")]
fn names(dir: &str) -> String {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.join(" ")
}

#[cfg(target_os = "linux")]
fn main() {
    use rustix::process::{getgid, getpid, getuid, pidfd_open, PidfdFlags};

    println!("uid={} gid={}", getuid().as_raw(), getgid().as_raw());
    println!("pid={}", getpid().as_raw_nonzero());
    let procs = std::fs::read_dir("/proc").map_or(0, |d| {
        d.filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
            .count()
    });
    println!("procs={procs}");
    println!("root: {}", names("/"));
    println!("tmp: {}", names("/QOpenSys/tmp"));
    println!("home={}", std::path::Path::new("/home").is_dir());
    match pidfd_open(getpid(), PidfdFlags::empty()) {
        Ok(_) => println!("pidfd_open: allowed"),
        Err(e) => println!("pidfd_open: refused errno {}", e.raw_os_error()),
    }
    // Hashing seeds itself from getrandom: an allowed call.
    let _seeded = std::collections::hash_map::RandomState::new();
    println!("ok");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        println!("args: {}", args.join(" "));
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("pase-probe runs only on Linux");
    std::process::exit(1);
}
