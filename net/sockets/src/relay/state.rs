// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the relay holds: descriptors queued per target, and how many
//! takers wait on each. Generic over the descriptor so it tests
//! without a kernel.

use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::pass::JobId;

/// A queued descriptor: from whom, the note, the descriptor.
type Entry<F> = (JobId, Vec<u8>, F);

#[derive(Debug)]
struct Inner<F> {
    queues: HashMap<JobId, VecDeque<Entry<F>>>,
    waiting: HashMap<JobId, u32>,
}

/// The relay's queues, shared by its connection threads.
#[derive(Debug)]
pub struct State<F> {
    inner: Mutex<Inner<F>>,
    cv: Condvar,
}

impl<F> Default for State<F> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                queues: HashMap::new(),
                waiting: HashMap::new(),
            }),
            cv: Condvar::new(),
        }
    }
}

impl<F> State<F> {
    /// Queue `fd` for `target`.
    pub fn give(&self, target: JobId, source: JobId, note: Vec<u8>, fd: F) {
        let mut g = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        g.queues.entry(target).or_default().push_back((source, note, fd));
        self.cv.notify_all();
    }

    /// Wait up to `wait` (forever with `None`) for a descriptor for
    /// `me`, from `source` when given.
    pub fn take(&self, me: JobId, source: Option<JobId>, wait: Option<Duration>) -> Option<Entry<F>> {
        let until = wait.map(|w| Instant::now() + w);
        let mut g = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        *g.waiting.entry(me).or_default() += 1;
        let got = loop {
            if let Some(q) = g.queues.get_mut(&me)
                && let Some(at) = q.iter().position(|(from, _, _)| source.is_none_or(|s| s == *from))
            {
                break q.remove(at);
            }
            match until {
                Some(t) => {
                    let now = Instant::now();
                    if now >= t {
                        break None;
                    }
                    g = self.cv.wait_timeout(g, t - now).unwrap_or_else(PoisonError::into_inner).0;
                }
                None => g = self.cv.wait(g).unwrap_or_else(PoisonError::into_inner),
            }
        };
        if let Some(n) = g.waiting.get_mut(&me) {
            *n = n.saturating_sub(1);
        }
        got
    }

    /// How many takers wait as `me` now, less what is already queued
    /// for them.
    pub fn idle(&self, me: &JobId) -> u32 {
        let g = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        let waiting = g.waiting.get(me).copied().unwrap_or(0);
        let queued = g.queues.get(me).map_or(0, |q| u32::try_from(q.len()).unwrap_or(u32::MAX));
        waiting.saturating_sub(queued)
    }

    /// Descriptors queued for `me`.
    pub fn queued(&self, me: &JobId) -> usize {
        let g = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        g.queues.get(me).map_or(0, VecDeque::len)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn hands_over_in_order() {
        let s: Arc<State<u32>> = Arc::default();
        let pool = crate::relay::pool("QZRCSRVS");
        let t = {
            let s = s.clone();
            std::thread::spawn(move || s.take(pool, None, Some(Duration::from_secs(5))))
        };
        while s.idle(&pool) == 0 {
            std::thread::yield_now();
        }
        s.give(pool, [1; 16], b"T".to_vec(), 7);
        assert_eq!(t.join().unwrap(), Some(([1; 16], b"T".to_vec(), 7)));
        assert_eq!(s.take(pool, None, Some(Duration::from_millis(10))), None);
        s.give(pool, [2; 16], Vec::new(), 8);
        s.give(pool, [3; 16], Vec::new(), 9);
        assert_eq!(s.take(pool, Some([3; 16]), None).map(|e| e.2), Some(9));
        assert_eq!(s.queued(&pool), 1);
    }
}
