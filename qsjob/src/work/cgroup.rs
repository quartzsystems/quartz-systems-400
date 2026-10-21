// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Storage pools as memory cgroups: `/sys/fs/cgroup/qs400/<POOL>` for a
//! shared pool (`*BASE` is `BASE`), `/sys/fs/cgroup/qs400/<SBS>/<n>`
//! for a subsystem's private pool. A pool's size is the cgroup's
//! `memory.high`. Failures are reported, not fatal: a system without
//! cgroups runs its jobs unpartitioned.

use std::fs;
use std::path::PathBuf;

/// The root of the pools.
const ROOT: &str = "/sys/fs/cgroup/qs400";

/// The directory of a pool: `*BASE`, `*SHRPOOL3`, `QBATCH/2`.
#[must_use]
pub fn path(pool: &str) -> PathBuf {
    let mut p = PathBuf::from(ROOT);
    for part in pool.trim_start_matches('*').split('/') {
        p.push(part);
    }
    p
}

/// Turn the memory controller on for children of `dir`.
fn enable(dir: &std::path::Path) {
    let _ = fs::write(dir.join("cgroup.subtree_control"), "+memory +pids");
}

/// Make a pool's cgroup and set its size; 0 kilobytes is no limit.
pub fn ensure(pool: &str, size_kb: u64) {
    let dir = path(pool);
    enable(std::path::Path::new("/sys/fs/cgroup"));
    let _ = fs::create_dir_all(ROOT);
    enable(std::path::Path::new(ROOT));
    if let Some(parent) = dir.parent()
        && parent != std::path::Path::new(ROOT)
    {
        let _ = fs::create_dir_all(parent);
        enable(parent);
    }
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("qsjob: pool {pool}: {e}");
        return;
    }
    let high = if size_kb == 0 {
        "max".to_string()
    } else {
        (size_kb * 1024).to_string()
    };
    if let Err(e) = fs::write(dir.join("memory.high"), high) {
        eprintln!("qsjob: pool {pool} size: {e}");
    }
}

/// Move a process into a pool.
pub fn place(pool: &str, pid: u32) {
    let file = path(pool).join("cgroup.procs");
    if let Err(e) = fs::write(&file, pid.to_string()) {
        eprintln!("qsjob: job {pid} into pool {pool}: {e}");
    }
}

/// Remove a private pool's cgroup once its jobs are gone.
pub fn remove(pool: &str) {
    if !pool.starts_with('*') {
        let _ = fs::remove_dir(path(pool));
    }
}
