// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signal plumbing. The only module in slic that uses `unsafe` and `libc`.
//! Signals of interest are blocked in the main thread before any other
//! thread exists (so all inherit the mask) and collected synchronously
//! with `sigwaitinfo` on a dedicated thread.

#![allow(unsafe_code)]

use std::mem;
use std::ptr;

use rustix::process::{WaitOptions, getpid, waitpid};

/// Signals slic handles.
pub const HANDLED: [i32; 4] = [libc::SIGCHLD, libc::SIGUSR1, libc::SIGTERM, libc::SIGINT];

/// Are we PID 1?
pub fn is_init() -> bool {
    getpid().is_init()
}

fn handled_set() -> libc::sigset_t {
    // SAFETY: an all-zero sigset_t is a valid value to pass to sigemptyset.
    let mut set: libc::sigset_t = unsafe { mem::zeroed() };
    // SAFETY: `set` is a valid, writable sigset_t.
    unsafe {
        libc::sigemptyset(&raw mut set);
        for s in HANDLED {
            libc::sigaddset(&raw mut set, s);
        }
    }
    set
}

/// Block the handled signals in the calling thread. Call before spawning.
pub fn block_signals() {
    let set = handled_set();
    // SAFETY: `set` is valid; a null old-set pointer is permitted.
    unsafe {
        libc::pthread_sigmask(libc::SIG_BLOCK, &raw const set, ptr::null_mut());
    }
}

/// Wait for one of the handled signals. Returns its number.
pub fn wait_signal() -> i32 {
    let set = handled_set();
    loop {
        // SAFETY: `set` is valid; siginfo may be null.
        let signo = unsafe { libc::sigwaitinfo(&raw const set, ptr::null_mut()) };
        if signo > 0 {
            return signo;
        }
    }
}

/// Reap every exited child. Returns their process IDs.
pub fn reap() -> Vec<u32> {
    let mut pids = Vec::new();
    while let Ok(Some((pid, _))) = waitpid(None, WaitOptions::NOHANG) {
        pids.push(pid.as_raw_nonzero().get().unsigned_abs());
    }
    pids
}
