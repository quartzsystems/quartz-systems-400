// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signal plumbing. The only module in slic that uses `unsafe` and `libc`.
//! `SIGCHLD` is blocked and collected synchronously with `sigtimedwait`, so
//! there is no asynchronous handler to reason about.

#![allow(unsafe_code)]

use std::mem;
use std::ptr;
use std::time::Duration;

use rustix::process::{WaitOptions, getpid, waitpid};

/// Why `wait` returned.
pub enum Wake {
    /// A child changed state.
    Child,
    /// The timeout elapsed (or an unrelated interruption).
    Timeout,
}

/// Are we PID 1?
pub fn is_init() -> bool {
    getpid().is_init()
}

fn sigchld_set() -> libc::sigset_t {
    // SAFETY: an all-zero sigset_t is a valid value to pass to sigemptyset.
    let mut set: libc::sigset_t = unsafe { mem::zeroed() };
    // SAFETY: `set` is a valid, writable sigset_t.
    unsafe {
        libc::sigemptyset(&raw mut set);
        libc::sigaddset(&raw mut set, libc::SIGCHLD);
    }
    set
}

/// Block `SIGCHLD` so `wait` can collect it.
pub fn block_sigchld() {
    let set = sigchld_set();
    // SAFETY: `set` is valid; a null old-set pointer is permitted.
    unsafe {
        libc::sigprocmask(libc::SIG_BLOCK, &raw const set, ptr::null_mut());
    }
}

/// Wait up to `timeout` for a blocked `SIGCHLD`.
pub fn wait(timeout: Duration) -> Wake {
    let set = sigchld_set();
    // SAFETY: an all-zero timespec is valid; fields are set below.
    let mut ts: libc::timespec = unsafe { mem::zeroed() };
    // tv_sec and tv_nsec are both i64 on x86_64 musl.
    ts.tv_sec = i64::try_from(timeout.as_secs()).unwrap_or(i64::MAX);
    ts.tv_nsec = i64::from(timeout.subsec_nanos());
    // SAFETY: `set` and `ts` are valid for the call; siginfo may be null.
    let signo = unsafe { libc::sigtimedwait(&raw const set, ptr::null_mut(), &raw const ts) };
    if signo == libc::SIGCHLD {
        Wake::Child
    } else {
        Wake::Timeout
    }
}

/// Reap every exited child. Returns how many.
pub fn reap() -> usize {
    let mut n = 0;
    while let Ok(Some(_)) = waitpid(None, WaitOptions::NOHANG) {
        n += 1;
    }
    n
}
