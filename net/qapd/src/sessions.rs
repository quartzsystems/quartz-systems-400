// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The signed-on sessions, kept by the storage service: a token names
//! a session; a session has one resumable connection job (the one that
//! signed on or last resumed) and others attached (version 4), at most
//! [`MAX_CONNECTIONS`] in all. A session with no connection can be
//! resumed until its idle timeout. A job that ended without detaching
//! counts as detached from when that is first noticed. When a session
//! ends, its other jobs are told ([`Sessions::take_ended`]). Tokens
//! never leave this table except to the client that signed on.

use std::collections::HashMap;

use qap_proto::Token;

/// Connections a session may have, the resumable one included.
pub const MAX_CONNECTIONS: usize = 16;

/// One session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The profile.
    pub user: String,
    /// Seconds it may stay without a connection.
    pub idle_secs: u32,
    /// The resumable connection job, by number.
    pub job: Option<u32>,
    /// The attached connection jobs.
    pub attached: Vec<u32>,
    /// When its last connection left, Unix seconds.
    pub detached_at: u64,
}

impl Session {
    /// Its connections now.
    #[must_use]
    pub fn connections(&self) -> usize {
        usize::from(self.job.is_some()) + self.attached.len()
    }

    /// Whether `job` is one of its connections.
    fn has(&self, job: u32) -> bool {
        self.job == Some(job) || self.attached.contains(&job)
    }
}

/// Why a token does not resume or attach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gone {
    /// No such token (never issued, signed off, or ended with the
    /// server).
    Unknown,
    /// Without a connection longer than its idle timeout.
    Expired,
    /// It has [`MAX_CONNECTIONS`] already.
    Full,
}

/// How a session ended, as its other connections are told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndedBy {
    /// `SignOff` on one of its connections.
    SignOff,
    /// An operator ended one of its connection jobs.
    Operator,
}

/// The table.
#[derive(Debug, Default)]
pub struct Sessions {
    map: HashMap<[u8; 32], Session>,
    /// Jobs whose session another job ended, until they ask.
    ended: HashMap<u32, EndedBy>,
}

impl Sessions {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop jobs that are gone, then the expired sessions.
    fn sweep(&mut self, now: u64, alive: &dyn Fn(u32) -> bool) {
        for s in self.map.values_mut() {
            let before = s.connections();
            if s.job.is_some_and(|j| !alive(j)) {
                s.job = None;
            }
            s.attached.retain(|j| alive(*j));
            if before > 0 && s.connections() == 0 {
                s.detached_at = now;
            }
        }
        self.map.retain(|_, s| {
            s.connections() > 0 || now < s.detached_at.saturating_add(u64::from(s.idle_secs))
        });
        self.ended.retain(|j, _| alive(*j));
    }

    /// A new session for `user`, `job` its resumable connection.
    pub fn create(&mut self, token: &Token, user: &str, idle_secs: u32, job: u32) {
        self.map.insert(
            token.0,
            Session {
                user: user.to_string(),
                idle_secs,
                job: Some(job),
                attached: Vec::new(),
                detached_at: 0,
            },
        );
    }

    /// The session `token` names, after a sweep.
    fn live(
        &mut self,
        token: &Token,
        now: u64,
        alive: &dyn Fn(u32) -> bool,
    ) -> Result<&mut Session, Gone> {
        let was_known = self.map.contains_key(&token.0);
        self.sweep(now, alive);
        self.map.get_mut(&token.0).ok_or(if was_known {
            Gone::Expired
        } else {
            Gone::Unknown
        })
    }

    /// Make `job` the session's resumable connection, moving it from
    /// another job; attached jobs stay. `Full` when that would make more
    /// than [`MAX_CONNECTIONS`].
    pub fn resume(
        &mut self,
        token: &Token,
        job: u32,
        now: u64,
        alive: &dyn Fn(u32) -> bool,
    ) -> Result<Session, Gone> {
        let s = self.live(token, now, alive)?;
        if s.job.is_none() && s.connections() >= MAX_CONNECTIONS {
            return Err(Gone::Full);
        }
        s.job = Some(job);
        Ok(s.clone())
    }

    /// Attach `job` to the session beside its other connections; `Full`
    /// when it has [`MAX_CONNECTIONS`].
    pub fn attach(
        &mut self,
        token: &Token,
        job: u32,
        now: u64,
        alive: &dyn Fn(u32) -> bool,
    ) -> Result<Session, Gone> {
        let s = self.live(token, now, alive)?;
        if s.has(job) {
            return Ok(s.clone());
        }
        if s.connections() >= MAX_CONNECTIONS {
            return Err(Gone::Full);
        }
        s.attached.push(job);
        Ok(s.clone())
    }

    /// The connection of `job` closed. The last to go starts the idle
    /// timeout.
    pub fn detach(&mut self, token: &Token, job: u32, now: u64) {
        let Some(s) = self.map.get_mut(&token.0) else {
            return;
        };
        if !s.has(job) {
            return;
        }
        if s.job == Some(job) {
            s.job = None;
        }
        s.attached.retain(|j| *j != job);
        if s.connections() == 0 {
            s.detached_at = now;
        }
    }

    /// End the session from job `by` (`SignOff`, an operator); its other
    /// jobs are told why. Whether it was there.
    pub fn end(&mut self, token: &Token, by: u32, why: EndedBy) -> bool {
        let Some(s) = self.map.remove(&token.0) else {
            return false;
        };
        for j in s.job.into_iter().chain(s.attached).filter(|j| *j != by) {
            self.ended.insert(j, why);
        }
        true
    }

    /// Whether another job ended `job`'s session, and how; told once.
    pub fn take_ended(&mut self, job: u32) -> Option<EndedBy> {
        self.ended.remove(&job)
    }

    /// End every session (the server ended; its jobs hear it so).
    pub fn clear(&mut self) {
        self.map.clear();
        self.ended.clear();
    }

    /// The session `token` names, as it is now.
    #[must_use]
    pub fn get(&self, token: &Token) -> Option<&Session> {
        self.map.get(&token.0)
    }

    /// Sessions now, for a display.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALIVE: &dyn Fn(u32) -> bool = &|_| true;

    #[test]
    fn resume_within_and_after_the_timeout() {
        let mut t = Sessions::new();
        let tok = Token([1; 32]);
        t.create(&tok, "BOB", 60, 7);
        // Attached: never expires.
        assert_eq!(t.resume(&tok, 8, 10_000, ALIVE).unwrap().user, "BOB");
        t.detach(&tok, 7, 100); // not the job attached now: ignored
        t.detach(&tok, 8, 100);
        assert_eq!(t.resume(&tok, 9, 159, ALIVE).unwrap().job, Some(9));
        t.detach(&tok, 9, 200);
        assert_eq!(t.resume(&tok, 10, 260, ALIVE), Err(Gone::Expired));
        assert_eq!(t.resume(&tok, 10, 261, ALIVE), Err(Gone::Unknown));
        assert!(t.is_empty());
    }

    #[test]
    fn a_dead_job_detaches() {
        let mut t = Sessions::new();
        let tok = Token([2; 32]);
        t.create(&tok, "BOB", 60, 7);
        let dead: &dyn Fn(u32) -> bool = &|j| j != 7;
        // Noticed at 500: resumable until 560.
        assert!(t.resume(&tok, 8, 500, dead).is_ok());
        t.detach(&tok, 8, 500);
        assert_eq!(t.resume(&tok, 9, 561, ALIVE), Err(Gone::Expired));
    }

    #[test]
    fn sign_off_and_server_end() {
        let mut t = Sessions::new();
        let (a, b) = (Token([3; 32]), Token([4; 32]));
        t.create(&a, "A", 60, 1);
        t.create(&b, "B", 60, 2);
        assert!(t.end(&a, 1, EndedBy::SignOff));
        assert!(!t.end(&a, 1, EndedBy::SignOff));
        assert_eq!(t.take_ended(1), None, "the job that signed off is not told");
        assert_eq!(t.resume(&a, 3, 0, ALIVE), Err(Gone::Unknown));
        t.clear();
        assert_eq!(t.resume(&b, 3, 0, ALIVE), Err(Gone::Unknown));
    }

    #[test]
    fn attach_up_to_sixteen() {
        let mut t = Sessions::new();
        let tok = Token([5; 32]);
        t.create(&tok, "BOB", 60, 1);
        for j in 2..=16u32 {
            assert_eq!(
                t.attach(&tok, j, 0, ALIVE).unwrap().connections(),
                j as usize
            );
        }
        assert_eq!(t.attach(&tok, 17, 0, ALIVE), Err(Gone::Full));
        // One leaves, one more fits.
        t.detach(&tok, 9, 0);
        assert!(t.attach(&tok, 17, 0, ALIVE).is_ok());
        // The resumable one left: one more attaches, and a resume would
        // then make 17.
        t.detach(&tok, 1, 0);
        assert!(t.attach(&tok, 18, 0, ALIVE).is_ok());
        assert_eq!(t.resume(&tok, 19, 0, ALIVE), Err(Gone::Full));
        assert_eq!(t.attach(&tok, 99, 0, ALIVE), Err(Gone::Full));
        assert_eq!(t.attach(&Token([6; 32]), 20, 0, ALIVE), Err(Gone::Unknown));
    }

    #[test]
    fn the_timer_runs_only_without_connections() {
        let mut t = Sessions::new();
        let tok = Token([7; 32]);
        t.create(&tok, "BOB", 60, 1);
        t.attach(&tok, 2, 0, ALIVE).unwrap();
        // The resumable connection left; the attached one keeps it.
        t.detach(&tok, 1, 100);
        assert!(t.attach(&tok, 3, 10_000, ALIVE).is_ok());
        t.detach(&tok, 2, 10_000);
        t.detach(&tok, 3, 20_000);
        // Resumable until 20 060.
        assert_eq!(t.resume(&tok, 4, 20_059, ALIVE).unwrap().job, Some(4));
        t.detach(&tok, 4, 30_000);
        assert_eq!(t.attach(&tok, 5, 30_060, ALIVE), Err(Gone::Expired));
    }

    #[test]
    fn a_dead_attached_job_leaves() {
        let mut t = Sessions::new();
        let tok = Token([8; 32]);
        t.create(&tok, "BOB", 60, 1);
        t.attach(&tok, 2, 0, ALIVE).unwrap();
        t.detach(&tok, 1, 0);
        let dead: &dyn Fn(u32) -> bool = &|j| j != 2;
        // Noticed at 500: no connection left, resumable until 560.
        let s = t.resume(&tok, 3, 500, dead).unwrap();
        assert!(s.attached.is_empty());
        t.detach(&tok, 3, 500);
        assert_eq!(t.resume(&tok, 4, 560, ALIVE), Err(Gone::Expired));
    }

    #[test]
    fn resume_leaves_attached_jobs() {
        let mut t = Sessions::new();
        let tok = Token([9; 32]);
        t.create(&tok, "BOB", 60, 1);
        t.attach(&tok, 2, 0, ALIVE).unwrap();
        let s = t.resume(&tok, 3, 0, ALIVE).unwrap();
        assert_eq!((s.job, s.attached), (Some(3), vec![2]));
    }

    #[test]
    fn ending_tells_the_other_jobs() {
        let mut t = Sessions::new();
        let tok = Token([10; 32]);
        t.create(&tok, "BOB", 60, 1);
        t.attach(&tok, 2, 0, ALIVE).unwrap();
        t.attach(&tok, 3, 0, ALIVE).unwrap();
        assert!(t.end(&tok, 2, EndedBy::Operator));
        assert_eq!(t.take_ended(2), None);
        assert_eq!(t.take_ended(1), Some(EndedBy::Operator));
        assert_eq!(t.take_ended(3), Some(EndedBy::Operator));
        assert_eq!(t.take_ended(3), None, "told once");
        // A job that ended before it asked is forgotten at the next sweep.
        let tok = Token([11; 32]);
        t.create(&tok, "BOB", 60, 4);
        t.attach(&tok, 5, 0, ALIVE).unwrap();
        t.end(&tok, 5, EndedBy::SignOff);
        let dead: &dyn Fn(u32) -> bool = &|j| j != 4;
        let _ = t.resume(&Token([12; 32]), 6, 0, dead);
        assert_eq!(t.take_ended(4), None);
    }
}
