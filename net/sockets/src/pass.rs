// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `givedescriptor` and `takedescriptor`: a socket handed from one job
//! to another (prestart servers hand accepted connections this way).
//! A job is named by its 16-byte internal job identifier, the value
//! `QUSRJOBI` returns. [`InProcess`] relays between jobs in one
//! process; between job processes the storage service's relay passes
//! the kernel descriptor with `SCM_RIGHTS` ([`crate::relay::Relay`]).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};

use crate::Errno;
use crate::sys::Fd;

/// An internal job identifier.
pub type JobId = [u8; 16];

/// Descriptor passing.
pub trait DescriptorPassing {
    /// Queue `fd` (already duplicated: the giver keeps its own) for
    /// `target`.
    fn give(&mut self, fd: Fd, target: &JobId) -> Result<(), Errno>;

    /// Wait for a descriptor given to this job, from `source` or, with
    /// `None`, from any job.
    fn take(&mut self, source: Option<&JobId>) -> Result<Fd, Errno>;
}

type Queues = HashMap<JobId, VecDeque<(JobId, Fd)>>;

/// Passing between jobs of one process (threads, or tests). Every job
/// holds a handle made by [`InProcess::join`] on one shared relay.
#[derive(Clone)]
pub struct InProcess {
    me: JobId,
    relay: Arc<(Mutex<Queues>, Condvar)>,
}

impl std::fmt::Debug for InProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InProcess").field("me", &self.me).finish_non_exhaustive()
    }
}

impl InProcess {
    /// A relay with one job on it.
    #[must_use]
    pub fn new(me: JobId) -> Self {
        Self {
            me,
            relay: Arc::new((Mutex::new(HashMap::new()), Condvar::new())),
        }
    }

    /// Another job on the same relay.
    #[must_use]
    pub fn join(&self, me: JobId) -> Self {
        Self {
            me,
            relay: self.relay.clone(),
        }
    }

    /// Descriptors waiting for this job.
    #[must_use]
    pub fn waiting(&self) -> usize {
        let q = self.relay.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        q.get(&self.me).map_or(0, VecDeque::len)
    }
}

impl DescriptorPassing for InProcess {
    fn give(&mut self, fd: Fd, target: &JobId) -> Result<(), Errno> {
        let (lock, cv) = &*self.relay;
        let mut q = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        q.entry(*target).or_default().push_back((self.me, fd));
        cv.notify_all();
        Ok(())
    }

    // Off Linux no descriptor exists, so nothing is ever queued.
    #[cfg_attr(not(target_os = "linux"), allow(unreachable_code))]
    fn take(&mut self, source: Option<&JobId>) -> Result<Fd, Errno> {
        let (lock, cv) = &*self.relay;
        let mut q = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(mine) = q.get_mut(&self.me) {
                let at = mine
                    .iter()
                    .position(|(from, _)| source.is_none_or(|s| s == from));
                if let Some(at) = at {
                    let (_, fd) = mine.remove(at).expect("found");
                    return Ok(fd);
                }
            }
            q = cv.wait(q).unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}
