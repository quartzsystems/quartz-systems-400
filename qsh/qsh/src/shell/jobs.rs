// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job table: asynchronous lists and their children.

use crate::sys::{Child, WaitStatus};

/// A job's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    /// Still running.
    Running,
    /// Ended with this status.
    Done(WaitStatus),
    /// Stopped by a signal.
    Stopped(i32),
}

/// A job.
pub struct Job {
    /// The job number.
    pub id: u32,
    /// The process ID (`$!`).
    pub pid: i32,
    /// The child.
    pub child: Box<dyn Child>,
    /// The command text.
    pub text: String,
    /// The state at the last poll.
    pub state: JobState,
}

/// All jobs of a shell.
#[derive(Default)]
pub struct Jobs {
    list: Vec<Job>,
}

impl Jobs {
    /// Add a running job; its number is the lowest free one.
    pub fn add(&mut self, child: Box<dyn Child>, text: String) -> u32 {
        let mut id = 1;
        while self.list.iter().any(|j| j.id == id) {
            id += 1;
        }
        let pid = child.pid();
        self.list.push(Job {
            id,
            pid,
            child,
            text,
            state: JobState::Running,
        });
        id
    }

    /// Update states without waiting.
    pub fn poll(&mut self) {
        for j in &mut self.list {
            if j.state == JobState::Running
                && let Ok(Some(st)) = j.child.try_wait()
            {
                j.state = match st {
                    WaitStatus::Stopped(s) => JobState::Stopped(s),
                    other => JobState::Done(other),
                };
            }
        }
    }

    /// Jobs in number order.
    pub fn iter(&self) -> impl Iterator<Item = &Job> {
        self.list.iter()
    }

    /// The current job (`%+`): the most recently started.
    pub fn current(&self) -> Option<u32> {
        self.list.last().map(|j| j.id)
    }

    /// The previous job (`%-`).
    pub fn previous(&self) -> Option<u32> {
        let n = self.list.len();
        (n >= 2).then(|| self.list[n - 2].id)
    }

    /// A job by `%` spec or process ID.
    pub fn find(&self, spec: &str) -> Option<u32> {
        if let Some(s) = spec.strip_prefix('%') {
            return match s {
                "" | "%" | "+" => self.current(),
                "-" => self.previous(),
                _ => {
                    if let Ok(n) = s.parse::<u32>() {
                        return self.list.iter().find(|j| j.id == n).map(|j| j.id);
                    }
                    if let Some(sub) = s.strip_prefix('?') {
                        return self
                            .list
                            .iter()
                            .rev()
                            .find(|j| j.text.contains(sub))
                            .map(|j| j.id);
                    }
                    self.list
                        .iter()
                        .rev()
                        .find(|j| j.text.starts_with(s))
                        .map(|j| j.id)
                }
            };
        }
        let pid: i32 = spec.parse().ok()?;
        self.list.iter().find(|j| j.pid == pid).map(|j| j.id)
    }

    /// A job by number.
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Job> {
        self.list.iter_mut().find(|j| j.id == id)
    }

    /// Wait for a job and remove it; its status.
    pub fn wait(&mut self, id: u32) -> Option<WaitStatus> {
        let j = self.get_mut(id)?;
        let st = match j.state {
            JobState::Done(st) => st,
            _ => j.child.wait().unwrap_or(WaitStatus::Exited(127)),
        };
        self.list.retain(|j| j.id != id);
        Some(st)
    }

    /// Remove finished jobs, returning them for notification.
    pub fn reap(&mut self) -> Vec<Job> {
        let (done, keep): (Vec<Job>, Vec<Job>) = std::mem::take(&mut self.list)
            .into_iter()
            .partition(|j| matches!(j.state, JobState::Done(_)));
        self.list = keep;
        done
    }

    /// Job numbers in order.
    pub fn ids(&self) -> Vec<u32> {
        self.list.iter().map(|j| j.id).collect()
    }
}

/// The state word `jobs` prints.
pub fn state_text(s: JobState) -> String {
    match s {
        JobState::Running => "Running".to_string(),
        JobState::Done(WaitStatus::Exited(0)) => "Done".to_string(),
        JobState::Done(WaitStatus::Exited(n)) => format!("Done({n})"),
        JobState::Done(WaitStatus::Signaled(n) | WaitStatus::Stopped(n)) => {
            crate::signal::name(n).map_or_else(|| format!("Signal {n}"), |s| format!("SIG{s}"))
        }
        JobState::Stopped(n) => crate::signal::name(n)
            .map_or_else(|| "Stopped".to_string(), |s| format!("Stopped(SIG{s})")),
    }
}
