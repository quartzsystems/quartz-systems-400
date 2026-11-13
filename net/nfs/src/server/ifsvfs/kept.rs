// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Kept descriptors that yield at once. Each descriptor a daemon thread
//! keeps holds an IFS lease (read and handle caching), as `QZLSFILE`'s
//! opens do, under a tag of its own ([`tag_of`]: the top bit set, so
//! never an `NFSv4` delegation's). A local job's (or `NetServer`'s) open
//! that the kept descriptor's share mode refuses breaks the handle
//! caching; the job's news poller ([`JobNews`], whatever thread polls)
//! takes the descriptor from the thread that kept it ([`super::jobfds`],
//! after the call that thread is in, at most [`BUSY_BOUND`]), gives the
//! lease back and closes it, and the open goes on. Other break orders
//! (delegations) are passed on to the poller's owner.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ifs::client::IfsClient;
use ifs::lease::state::H;
use ifs::lease::{LeaseKey, News};

use super::jobfds;
use crate::server::error::{VResult, VfsError};
use crate::server::instance::Instance;
use crate::server::news::NewsSource;

/// The tag bit of a kept descriptor's lease.
pub const KEPT: u64 = 1 << 63;

/// How long a break waits for the thread holding the descriptor to end
/// its call.
pub const BUSY_BOUND: Duration = Duration::from_secs(2);

/// The lease tag of kept descriptor `fd`.
#[must_use]
pub fn tag_of(fd: i32) -> u64 {
    KEPT | u64::from(fd.cast_unsigned())
}

/// Whether a lease tag is a kept descriptor's.
#[must_use]
pub fn is_kept(tag: u64) -> bool {
    tag & KEPT != 0
}

/// The lease key of kept descriptor `fd`.
#[must_use]
pub fn key_of(fd: i32) -> LeaseKey {
    let mut k = [0u8; 32];
    k[..8].copy_from_slice(b"QSNFSFD\0");
    k[8..12].copy_from_slice(&fd.to_be_bytes());
    k
}

/// The descriptor of a kept tag.
fn fd_of(tag: u64) -> i32 {
    i32::try_from(tag & u64::from(u32::MAX)).unwrap_or(-1)
}

/// Gives back kept descriptor `fd`'s lease and closes it, once the
/// thread holding it is not in a call (at most [`BUSY_BOUND`]); whether
/// it was closed.
pub fn release(c: &dyn IfsClient, instance: &Arc<Instance>, fd: i32) -> bool {
    let end = Instant::now() + BUSY_BOUND;
    loop {
        match jobfds::take_kept(instance, fd) {
            jobfds::Taken::Yes => {
                let _ = c.lease_report(tag_of(fd), &[]);
                let _ = c.close(fd);
                return true;
            }
            jobfds::Taken::NotKept => {
                // Already closed (its lease went with it).
                let _ = c.lease_report(tag_of(fd), &[]);
                return false;
            }
            jobfds::Taken::Busy if Instant::now() >= end => return false,
            jobfds::Taken::Busy => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

/// The IFS's news for a daemon job: break orders of the job's kept
/// descriptors are carried out here, the rest passed on.
#[derive(Debug)]
pub struct JobNews<C: IfsClient> {
    client: C,
    instance: Arc<Instance>,
}

impl<C: IfsClient> JobNews<C> {
    /// Over the poller thread's client, for the threads of `instance`.
    pub fn new(client: C, instance: Arc<Instance>) -> Self {
        Self { client, instance }
    }

    /// Carry out the kept descriptors' orders of `news`; what is left.
    pub fn sift(&self, mut news: News) -> News {
        let (kept, rest): (Vec<_>, Vec<_>) =
            news.breaks.into_iter().partition(|(t, _)| is_kept(*t));
        for (tag, keep) in kept {
            // Only losing the handle caching (an open refused by the
            // descriptor's share mode) needs the descriptor gone.
            if keep & H == 0 {
                release(&self.client, &self.instance, fd_of(tag));
            }
        }
        news.breaks = rest;
        news
    }
}

impl<C: IfsClient> NewsSource for JobNews<C> {
    fn poll(&mut self, wait_ms: u32) -> VResult<News> {
        match self.client.lease_orders(wait_ms) {
            Ok(n) => Ok(self.sift(n)),
            Err(ifs::Errno::EAGAIN | ifs::Errno::ETIME) => Ok(News::default()),
            Err(e) => Err(VfsError::from(e)),
        }
    }
}
