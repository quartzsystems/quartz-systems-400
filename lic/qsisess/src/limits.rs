// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The session's limits: everything a hostile peer could make it hold
//! is bounded by one of these.

/// A session's limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Requests outstanding at once.
    pub max_requests: usize,
    /// Streams open at once.
    pub max_streams: usize,
    /// The longest message (request, reply, event), its envelope
    /// included, across all its parts.
    pub max_message: usize,
    /// The longest user name on a request, bytes.
    pub max_user: usize,
    /// The credit each side gives a stream when it opens, bytes.
    pub window: u32,
    /// Events the log keeps.
    pub log_events: usize,
    /// Bytes of event bodies the log keeps.
    pub log_bytes: usize,
    /// Idle this long, nothing heard, and a ping goes out (ms).
    pub keepalive_ms: u64,
    /// Nothing heard this long and the session closes (ms).
    pub timeout_ms: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_requests: 64,
            max_streams: 16,
            max_message: 1024 * 1024,
            max_user: 128,
            window: 256 * 1024,
            log_events: 4096,
            log_bytes: 4 * 1024 * 1024,
            keepalive_ms: 15_000,
            timeout_ms: 45_000,
        }
    }
}

impl Limits {
    /// The longest message, never more than a part's length field holds.
    pub(crate) fn message(&self) -> usize {
        self.max_message.min(u32::MAX as usize)
    }
}
