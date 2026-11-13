// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The daemon registry: which NFS daemon jobs run, which are ready and
//! what they registered, which were asked to end, and the jobs
//! submitted that have not said hello yet.

use stg::nfs::{Daemon, NfsDaemonJob, NfsPort};

/// One job of the registry, with its number.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Job {
    number: u32,
    info: NfsDaemonJob,
}

/// The registry.
#[derive(Debug, Default)]
pub struct Registry {
    jobs: Vec<Job>,
    /// Submitted, not registered yet: the daemon and the job number.
    pending: Vec<(Daemon, u32)>,
    /// Asked to end before they said hello: they end as they start.
    cancelled: Vec<u32>,
    /// The binder is to read `/etc/RPCBTAB` back when it starts.
    restore: bool,
}

impl Registry {
    /// Forget the jobs `live` says are gone (ended, crashed, never ran).
    pub fn prune(&mut self, live: &dyn Fn(u32) -> bool) {
        self.jobs.retain(|j| live(j.number));
        self.pending.retain(|(_, n)| live(*n));
        self.cancelled.retain(|n| live(*n));
    }

    /// Jobs of `daemon` running or on their way.
    #[must_use]
    pub fn count(&self, daemon: Daemon) -> usize {
        self.jobs.iter().filter(|j| j.info.daemon == daemon).count()
            + self.pending.iter().filter(|(d, _)| *d == daemon).count()
    }

    /// Jobs of `daemon` that reported ready and were not asked to end.
    #[must_use]
    pub fn ready(&self, daemon: Daemon) -> usize {
        self.jobs
            .iter()
            .filter(|j| j.info.daemon == daemon && j.info.ready && !j.info.ending)
            .count()
    }

    /// A job was submitted.
    pub fn submitted(&mut self, daemon: Daemon, number: u32, restore: bool) {
        self.pending.push((daemon, number));
        if daemon == Daemon::Rpc {
            self.restore = restore;
        }
    }

    /// A job says hello: its number among its type's (the lowest free,
    /// from 1), and for the binder whether to read its registrations
    /// back.
    pub fn hello(&mut self, daemon: Daemon, number: u32, job: &str, now: u64) -> (u16, bool) {
        self.pending.retain(|(_, n)| *n != number);
        self.jobs.retain(|j| j.number != number);
        let ending = self.cancelled.contains(&number);
        self.cancelled.retain(|n| *n != number);
        let mut index = 1u16;
        while self
            .jobs
            .iter()
            .any(|j| j.info.daemon == daemon && j.info.index == index)
        {
            index += 1;
        }
        self.jobs.push(Job {
            number,
            info: NfsDaemonJob {
                daemon,
                index,
                job: job.to_string(),
                ready: false,
                ending,
                ports: Vec::new(),
                since: now,
            },
        });
        let restore = daemon == Daemon::Rpc && std::mem::take(&mut self.restore);
        (index, restore)
    }

    /// The job is ready.
    pub fn set_ready(&mut self, number: u32, ports: Vec<NfsPort>) -> bool {
        match self.jobs.iter_mut().find(|j| j.number == number) {
            Some(j) => {
                j.info.ready = true;
                j.info.ports = ports;
                true
            }
            None => false,
        }
    }

    /// The job ends.
    pub fn gone(&mut self, number: u32) {
        self.jobs.retain(|j| j.number != number);
        self.pending.retain(|(_, n)| *n != number);
    }

    /// Whether the job was asked to end (or is not registered).
    #[must_use]
    pub fn should_end(&self, number: u32) -> bool {
        self.jobs
            .iter()
            .find(|j| j.number == number)
            .is_none_or(|j| j.info.ending)
    }

    /// Ask every job of `daemon` to end; how many there were, and the
    /// numbers of those still on the queue (they end when they start).
    pub fn end(&mut self, daemon: Daemon) -> (usize, Vec<u32>) {
        let mut n = 0;
        for j in self.jobs.iter_mut().filter(|j| j.info.daemon == daemon) {
            j.info.ending = true;
            n += 1;
        }
        let queued: Vec<u32> = self
            .pending
            .iter()
            .filter(|(d, _)| *d == daemon)
            .map(|(_, n)| *n)
            .collect();
        self.pending.retain(|(d, _)| *d != daemon);
        self.cancelled.extend(&queued);
        (n + queued.len(), queued)
    }

    /// Every registered job.
    #[must_use]
    pub fn list(&self) -> Vec<NfsDaemonJob> {
        let mut v: Vec<NfsDaemonJob> = self.jobs.iter().map(|j| j.info.clone()).collect();
        v.sort_by_key(|j| (j.daemon, j.index));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_daemons_life() {
        let mut r = Registry::default();
        r.submitted(Daemon::Svr, 10, false);
        r.submitted(Daemon::Svr, 11, false);
        assert_eq!(r.count(Daemon::Svr), 2);
        assert_eq!(r.hello(Daemon::Svr, 11, "j11", 5), (1, false));
        assert_eq!(r.hello(Daemon::Svr, 10, "j10", 5), (2, false));
        assert_eq!(r.ready(Daemon::Svr), 0);
        assert!(r.set_ready(10, Vec::new()));
        assert_eq!(r.ready(Daemon::Svr), 1);
        assert!(!r.should_end(10));
        assert_eq!(r.end(Daemon::Svr), (2, Vec::new()));
        assert!(r.should_end(10));
        assert_eq!(r.ready(Daemon::Svr), 0);
        r.gone(10);
        r.prune(&|n| n != 11);
        assert_eq!(r.count(Daemon::Svr), 0);
        // A freed number is taken again.
        assert_eq!(r.hello(Daemon::Svr, 12, "j12", 6).0, 1);
    }

    #[test]
    fn the_binder_restores_once() {
        let mut r = Registry::default();
        r.submitted(Daemon::Rpc, 1, true);
        assert_eq!(r.hello(Daemon::Rpc, 1, "b", 0), (1, true));
        r.gone(1);
        assert_eq!(r.hello(Daemon::Rpc, 2, "b", 0), (1, false));
        r.submitted(Daemon::Nlm, 3, false);
        assert_eq!(r.end(Daemon::Nlm), (1, vec![3]));
        // It starts late and is told to end at once.
        r.hello(Daemon::Nlm, 3, "n", 0);
        assert!(r.should_end(3));
    }
}
