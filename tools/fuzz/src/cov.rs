// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Coverage from LLVM's `SanitizerCoverage` inline 8-bit counters. A build
//! with `-C passes=sancov-module -C llvm-args=-sanitizer-coverage-level=3
//! -C llvm-args=-sanitizer-coverage-inline-8bit-counters` (what `build
//! fuzz` passes) gives every edge a byte counter and calls
//! [`__sanitizer_cov_8bit_counters_init`] once per module with its array.
//! Without those flags no array registers and the fuzzer runs blind.
//!
//! This is the only file in the crate with `unsafe`: the counter arrays
//! belong to the instrumented code, which bumps them as it runs.

use std::sync::Mutex;

/// The registered counter arrays, as (start, length).
static REGIONS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// Called by the instrumented code before `main`, once per module.
///
/// # Safety
///
/// `start..stop` must be a counter array that lives for the whole program
/// (LLVM's are static).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __sanitizer_cov_8bit_counters_init(start: *mut u8, stop: *mut u8) {
    let len = (stop as usize).saturating_sub(start as usize);
    if len > 0
        && let Ok(mut r) = REGIONS.lock()
    {
        r.push((start as usize, len));
    }
}

/// The PC table callback some LLVM versions emit with the counters; unused.
///
/// # Safety
///
/// Never dereferences its arguments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __sanitizer_cov_pcs_init(_start: *const usize, _stop: *const usize) {}

/// Whether the build is instrumented.
pub fn instrumented() -> bool {
    REGIONS.lock().is_ok_and(|r| !r.is_empty())
}

/// The number of edges the build counts.
pub fn edges() -> usize {
    REGIONS.lock().map_or(0, |r| r.iter().map(|&(_, l)| l).sum())
}

/// A hit count folded into one of eight buckets, as one bit.
fn bucket(n: u8) -> u8 {
    match n {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 4,
        4..=7 => 8,
        8..=15 => 16,
        16..=31 => 32,
        32..=127 => 64,
        128.. => 128,
    }
}

/// Every (edge, bucket) seen so far.
#[derive(Default)]
pub struct Map {
    seen: Vec<u8>,
}

impl Map {
    /// Folds the counters of the run just finished into the map and
    /// zeroes them. Returns how many new (edge, bucket) pairs it saw.
    pub fn absorb(&mut self) -> usize {
        let Ok(regions) = REGIONS.lock() else {
            return 0;
        };
        let total: usize = regions.iter().map(|&(_, l)| l).sum();
        if self.seen.len() < total {
            self.seen.resize(total, 0);
        }
        let mut new = 0;
        let mut at = 0;
        for &(start, len) in regions.iter() {
            // SAFETY: `start..start+len` is a static counter array LLVM
            // registered (see the init callback); bytes are always valid,
            // and the fuzzer runs one input at a time on this thread, so
            // nothing else writes them while they are read and cleared.
            let counters = unsafe { std::slice::from_raw_parts_mut(start as *mut u8, len) };
            // Most counters stay zero: skip them eight at a time.
            let mut i = 0;
            while i < counters.len() {
                if i + 8 <= counters.len() && counters[i..i + 8].iter().all(|&c| c == 0) {
                    i += 8;
                    continue;
                }
                let c = &mut counters[i];
                if *c != 0 {
                    let b = bucket(*c);
                    let s = &mut self.seen[at + i];
                    if *s & b == 0 {
                        *s |= b;
                        new += 1;
                    }
                    *c = 0;
                }
                i += 1;
            }
            at += len;
        }
        new
    }

    /// Edges seen at least once.
    pub fn covered(&self) -> usize {
        self.seen.iter().filter(|&&s| s != 0).count()
    }
}
