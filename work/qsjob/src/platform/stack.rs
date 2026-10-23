// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The native stack's guard page. A `SIGSEGV` there is a runaway
//! recursion: the handler, on its own signal stack, lets the stack grow
//! by one more MiB (`RLIMIT_STACK`) so the faulting access completes,
//! and tells the runtime, whose next call-stack entry raises `MCH4429`
//! at that instruction boundary. The only module in qsjob that uses
//! `unsafe` and `libc`.

#![allow(unsafe_code)]

use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

/// What each forgiven fault adds to the soft stack limit.
const GROWTH: libc::rlim_t = 1 << 20;
/// Faults forgiven before the job dies instead.
const MAX_FAULTS: usize = 8;
/// The alternate signal stack's size.
const ALT_STACK: usize = 64 << 10;

/// Where the main thread's stack was when the guard was installed.
static STACK_TOP: AtomicUsize = AtomicUsize::new(0);
/// Faults forgiven so far.
static FAULTS: AtomicUsize = AtomicUsize::new(0);

/// Install the guard: the alternate stack and the `SIGSEGV` handler.
/// Once, on the main thread, before any program runs.
pub fn install() {
    let here = 0u8;
    STACK_TOP.store(ptr::from_ref(&here) as usize, Ordering::Relaxed);
    let stack: &'static mut [u8] = Box::leak(vec![0u8; ALT_STACK].into_boxed_slice());
    let ss = libc::stack_t {
        ss_sp: stack.as_mut_ptr().cast(),
        ss_flags: 0,
        ss_size: ALT_STACK,
    };
    // SAFETY: `ss` describes memory this process keeps for ever; a null
    // old-stack pointer is permitted.
    unsafe {
        libc::sigaltstack(&raw const ss, ptr::null_mut());
    }
    // SAFETY: an all-zero sigaction is a valid starting value; every
    // field set below is one the handler needs; a null old-action
    // pointer is permitted.
    unsafe {
        let mut sa: libc::sigaction = mem::zeroed();
        sa.sa_sigaction = on_segv as *const () as usize;
        sa.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        libc::sigemptyset(&raw mut sa.sa_mask);
        libc::sigaction(libc::SIGSEGV, &raw const sa, ptr::null_mut());
    }
}

/// The handler: forgive a fault on the stack's guard, die on any other.
extern "C" fn on_segv(_signo: libc::c_int, info: *mut libc::siginfo_t, _ctx: *mut libc::c_void) {
    // SAFETY: the kernel passes a valid `siginfo_t` for `SA_SIGINFO`.
    let addr = unsafe { (*info).si_addr() } as usize;
    // SAFETY: an all-zero rlimit is a valid value for getrlimit to fill.
    let mut lim: libc::rlimit = unsafe { mem::zeroed() };
    // SAFETY: `lim` is a valid, writable rlimit.
    unsafe {
        libc::getrlimit(libc::RLIMIT_STACK, &raw mut lim);
    }
    let top = STACK_TOP.load(Ordering::Relaxed);
    let size = usize::try_from(lim.rlim_cur).unwrap_or(usize::MAX);
    // On the guard: just below the limit, within the gap the kernel
    // keeps under the stack.
    let on_guard = top != 0
        && addr < top
        && top - addr >= size.saturating_sub(64 << 10)
        && top - addr <= size.saturating_add(2 << 20);
    let n = FAULTS.fetch_add(1, Ordering::Relaxed) + 1;
    if on_guard && n <= MAX_FAULTS && lim.rlim_cur.saturating_add(GROWTH) <= lim.rlim_max {
        lim.rlim_cur += GROWTH;
        // SAFETY: `lim` is a valid rlimit.
        unsafe {
            libc::setrlimit(libc::RLIMIT_STACK, &raw const lim);
        }
        mi::rtlib::note_stack_fault();
        return;
    }
    let msg: &[u8] = if on_guard {
        b"qsjob: stack exhausted, job ended\n"
    } else {
        b"qsjob: addressing fault, job ended\n"
    };
    // SAFETY: `write` and `_exit` are async-signal-safe; the buffer is
    // a static string.
    unsafe {
        libc::write(2, msg.as_ptr().cast(), msg.len());
        libc::_exit(3);
    }
}
