// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The signed-on sessions, kept by the storage service: a token names
//! a session, a session is attached to one connection job at a time,
//! and a detached session can be resumed until its idle timeout. A
//! session whose job ended without detaching counts as detached from
//! when that is first noticed. Tokens never leave this table except to
//! the client that signed on.

use std::collections::HashMap;

use qap_proto::Token;

/// One session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The profile.
    pub user: String,
    /// Seconds it may stay detached.
    pub idle_secs: u32,
    /// The connection job attached, by number.
    pub job: Option<u32>,
    /// When it was detached, Unix seconds.
    pub detached_at: u64,
}

/// Why a token does not resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gone {
    /// No such token (never issued, signed off, or ended with the
    /// server).
    Unknown,
    /// Detached longer than its idle timeout.
    Expired,
}

/// The table.
#[derive(Debug, Default)]
pub struct Sessions {
    map: HashMap<[u8; 32], Session>,
}

impl Sessions {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Detach sessions whose job is gone, drop the expired ones.
    fn sweep(&mut self, now: u64, alive: &dyn Fn(u32) -> bool) {
        for s in self.map.values_mut() {
            if s.job.is_some_and(|j| !alive(j)) {
                s.job = None;
                s.detached_at = now;
            }
        }
        self.map
            .retain(|_, s| s.job.is_some() || now < s.detached_at.saturating_add(u64::from(s.idle_secs)));
    }

    /// A new session for `user`, attached to `job`.
    pub fn create(&mut self, token: &Token, user: &str, idle_secs: u32, job: u32) {
        self.map.insert(
            token.0,
            Session {
                user: user.to_string(),
                idle_secs,
                job: Some(job),
                detached_at: 0,
            },
        );
    }

    /// Attach `job` to the session `token` names (moving it from
    /// another job); its idle timeout starts over.
    pub fn resume(&mut self, token: &Token, job: u32, now: u64, alive: &dyn Fn(u32) -> bool) -> Result<Session, Gone> {
        let was_known = self.map.contains_key(&token.0);
        self.sweep(now, alive);
        let s = self
            .map
            .get_mut(&token.0)
            .ok_or(if was_known { Gone::Expired } else { Gone::Unknown })?;
        s.job = Some(job);
        s.detached_at = 0;
        Ok(s.clone())
    }

    /// The connection of `job` closed: the session waits to be resumed.
    pub fn detach(&mut self, token: &Token, job: u32, now: u64) {
        if let Some(s) = self.map.get_mut(&token.0)
            && s.job == Some(job)
        {
            s.job = None;
            s.detached_at = now;
        }
    }

    /// End the session (`SignOff`, an operator); whether it was there.
    pub fn end(&mut self, token: &Token) -> bool {
        self.map.remove(&token.0).is_some()
    }

    /// End every session (the server ended).
    pub fn clear(&mut self) {
        self.map.clear();
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
        assert!(t.end(&a));
        assert!(!t.end(&a));
        assert_eq!(t.resume(&a, 3, 0, ALIVE), Err(Gone::Unknown));
        t.clear();
        assert_eq!(t.resume(&b, 3, 0, ALIVE), Err(Gone::Unknown));
    }
}
