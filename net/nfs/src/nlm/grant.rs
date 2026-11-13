// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Blocked locks granted: the job's poll ([`NewsSource`]) names each
//! token the IFS granted; its request is called back
//! ([`super::call::send_granted`]), and a lock the client does not take
//! or answer for is given back through the thread's [`Vfs`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::call::{Grant, send_granted};
use super::state::NlmState;
use crate::server::Vfs;
use crate::server::news::NewsSource;

/// Sends the callbacks of every request granted: how many the clients took.
pub fn grant_ready(st: &NlmState, vfs: &dyn Vfs) -> usize {
    let mut n = 0;
    for b in st.take_ready() {
        match send_granted(st, &b) {
            Grant::Taken => n += 1,
            Grant::Refused | Grant::NoAnswer => {
                let _ = vfs.unlock(&b.req, &b.handle, b.sub, b.lock.offset, b.lock.len);
            }
        }
    }
    n
}

/// Polls `news` (waiting up to `poll_ms` each time) and sends grants
/// until `stop` is set.
pub fn grant_loop(st: &NlmState, vfs: &dyn Vfs, news: &mut dyn NewsSource, stop: &AtomicBool, poll_ms: u32) {
    while !stop.load(Ordering::Relaxed) {
        match news.poll(poll_ms) {
            Ok(n) => {
                for t in n.locks {
                    st.granted_token(t);
                }
            }
            Err(e) => {
                tracing::warn!("lock grant poll failed: {e}");
                std::thread::sleep(Duration::from_millis(u64::from(poll_ms)));
            }
        }
        grant_ready(st, vfs);
    }
}
