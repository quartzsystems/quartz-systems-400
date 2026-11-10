// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The session table: each `QZLSFILE` job registers its client's
//! sessions, their tree connections and open files, and the counters
//! `ZLSL0400` reports. The storage service holds the one table; the
//! list APIs read it and `QZLSENSS` marks sessions to end.

use std::collections::BTreeMap;

/// A tree connection: one share in use by a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// Unique within the table.
    pub id: u32,
    /// The share, upper case (`IPC$` for the pipe share).
    pub share: String,
    /// `ZLSL0600`'s connection type: 0 disk, 1 print, 3 IPC.
    pub kind: i32,
    /// Files open through it.
    pub opens: u32,
    /// When it was made (seconds since the epoch).
    pub since: u64,
}

/// A session: one user signed on from one client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// Unique within the table, never reused while the system is up.
    pub id: u64,
    /// The client's name as NetBIOS gives it (up to 15 characters), or
    /// its address when it gave none.
    pub workstation: String,
    /// The client's full name or address (up to 124 characters).
    pub expanded: String,
    /// The profile signed on.
    pub user: String,
    /// The `QZLSFILE` job serving it, `number/user/name`.
    pub job: String,
    /// Signed on as the guest profile.
    pub guest: bool,
    /// When it started (seconds since the epoch).
    pub since: u64,
    /// Its last request.
    pub last_active: u64,
    /// Its tree connections.
    pub connections: Vec<Connection>,
    /// `QZLSENSS` asked it to end; the job ends it and unregisters.
    pub ending: bool,
}

impl Session {
    /// Files open across its connections.
    #[must_use]
    pub fn opens(&self) -> u32 {
        self.connections.iter().map(|c| c.opens).sum()
    }
}

/// The counters `ZLSL0400` reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Files opened.
    pub opens: u32,
    /// Print jobs spooled.
    pub print_jobs: u32,
    /// Sessions started.
    pub session_starts: u32,
    /// Sessions ended by the idle time-out.
    pub auto_disconnects: u32,
    /// Sessions ended normally.
    pub normal_disconnects: u32,
    /// Sessions that replaced another of the same client.
    pub duplicate_sessions: u32,
    /// Sessions ended by an error.
    pub session_errors: u32,
    /// Sign-ons refused for a password.
    pub password_violations: u32,
    /// Sign-ons as the guest profile for an unknown user.
    pub unknown_users: u32,
    /// Bytes sent.
    pub bytes_sent: u64,
    /// Bytes received.
    pub bytes_received: u64,
    /// Requests answered, for the average response time.
    pub requests: u64,
    /// Their total time in milliseconds.
    pub response_ms: u64,
    /// When the server started (seconds since the epoch); 0 not started.
    pub started: u64,
    /// When the counters were last reset.
    pub reset: u64,
}

impl Stats {
    /// Average response time in milliseconds.
    #[must_use]
    pub fn average_response(&self) -> i32 {
        self.response_ms
            .checked_div(self.requests)
            .map_or(0, |v| i32::try_from(v).unwrap_or(i32::MAX))
    }
}

/// A counter a job adds to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counter {
    /// [`Stats::opens`].
    Opens,
    /// [`Stats::print_jobs`].
    PrintJobs,
    /// [`Stats::auto_disconnects`].
    AutoDisconnects,
    /// [`Stats::duplicate_sessions`].
    DuplicateSessions,
    /// [`Stats::session_errors`].
    SessionErrors,
    /// [`Stats::password_violations`].
    PasswordViolations,
    /// [`Stats::unknown_users`].
    UnknownUsers,
    /// [`Stats::bytes_sent`].
    BytesSent,
    /// [`Stats::bytes_received`].
    BytesReceived,
    /// One request taking `n` milliseconds.
    Response,
}

/// The table.
#[derive(Clone, Debug, Default)]
pub struct SessionTable {
    sessions: BTreeMap<u64, Session>,
    next_session: u64,
    next_connection: u32,
    /// The counters.
    pub stats: Stats,
}

/// Why a table request failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TableError {
    /// No session has the identifier.
    #[error("session {0} does not exist")]
    NoSession(u64),
    /// No connection has the identifier.
    #[error("connection {0} does not exist")]
    NoConnection(u32),
    /// The share has its maximum users connected.
    #[error("share {0} has its maximum users")]
    MaxUsers(String),
}

impl SessionTable {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The server started at `now`: counters from zero.
    pub fn started(&mut self, now: u64) {
        self.sessions.clear();
        self.stats = Stats {
            started: now,
            reset: now,
            ..Stats::default()
        };
    }

    /// Register a session; its identifier.
    pub fn register(&mut self, mut s: Session) -> u64 {
        self.next_session += 1;
        s.id = self.next_session;
        s.connections.clear();
        s.ending = false;
        self.stats.session_starts += 1;
        let id = s.id;
        self.sessions.insert(id, s);
        id
    }

    /// The session ended; `idle` when the time-out ended it.
    pub fn unregister(&mut self, id: u64, idle: bool) -> Result<Session, TableError> {
        let s = self.sessions.remove(&id).ok_or(TableError::NoSession(id))?;
        if idle {
            self.stats.auto_disconnects += 1;
        } else {
            self.stats.normal_disconnects += 1;
        }
        Ok(s)
    }

    /// A tree connection of session `id` to `share`; its identifier.
    pub fn connect(
        &mut self,
        id: u64,
        share: &str,
        kind: i32,
        now: u64,
    ) -> Result<u32, TableError> {
        let s = self
            .sessions
            .get_mut(&id)
            .ok_or(TableError::NoSession(id))?;
        self.next_connection = self.next_connection.wrapping_add(1).max(1);
        let c = self.next_connection;
        s.connections.push(Connection {
            id: c,
            share: share.to_ascii_uppercase(),
            kind,
            opens: 0,
            since: now,
        });
        s.last_active = now;
        Ok(c)
    }

    /// [`SessionTable::connect`] unless `share` already has `max` users
    /// (sessions with a connection to it) other than session `id` (`max`
    /// below 1: no maximum), counted across every job.
    ///
    /// # Errors
    ///
    /// No such session, or [`TableError::MaxUsers`].
    pub fn connect_within(
        &mut self,
        id: u64,
        share: &str,
        kind: i32,
        max: i32,
        now: u64,
    ) -> Result<u32, TableError> {
        // Users are sessions: a session's second tree to the share is no
        // new user.
        let has = |s: &Session| {
            s.connections
                .iter()
                .any(|c| c.share.eq_ignore_ascii_case(share))
        };
        let mine = self.sessions.get(&id).is_some_and(has);
        if let Ok(max) = usize::try_from(max)
            && max > 0
            && !mine
        {
            let users = self.sessions.values().filter(|s| has(s)).count();
            if users >= max {
                return Err(TableError::MaxUsers(share.to_ascii_uppercase()));
            }
        }
        self.connect(id, share, kind, now)
    }

    /// The tree connection `conn` ended.
    pub fn disconnect(&mut self, id: u64, conn: u32) -> Result<(), TableError> {
        let s = self
            .sessions
            .get_mut(&id)
            .ok_or(TableError::NoSession(id))?;
        let before = s.connections.len();
        s.connections.retain(|c| c.id != conn);
        if s.connections.len() == before {
            return Err(TableError::NoConnection(conn));
        }
        Ok(())
    }

    /// Files open through connection `conn` are now `opens`; an
    /// increase counts as opens in the statistics.
    pub fn set_opens(
        &mut self,
        id: u64,
        conn: u32,
        opens: u32,
        now: u64,
    ) -> Result<(), TableError> {
        let s = self
            .sessions
            .get_mut(&id)
            .ok_or(TableError::NoSession(id))?;
        s.last_active = now;
        let c = s
            .connections
            .iter_mut()
            .find(|c| c.id == conn)
            .ok_or(TableError::NoConnection(conn))?;
        if opens > c.opens {
            self.stats.opens = self.stats.opens.saturating_add(opens - c.opens);
        }
        c.opens = opens;
        Ok(())
    }

    /// Session `id` made a request at `now`.
    pub fn touch(&mut self, id: u64, now: u64) -> Result<(), TableError> {
        let s = self
            .sessions
            .get_mut(&id)
            .ok_or(TableError::NoSession(id))?;
        s.last_active = now;
        Ok(())
    }

    /// Add `n` to a counter.
    pub fn count(&mut self, c: Counter, n: u64) {
        let n32 = u32::try_from(n).unwrap_or(u32::MAX);
        let st = &mut self.stats;
        match c {
            Counter::Opens => st.opens = st.opens.saturating_add(n32),
            Counter::PrintJobs => st.print_jobs = st.print_jobs.saturating_add(n32),
            Counter::AutoDisconnects => {
                st.auto_disconnects = st.auto_disconnects.saturating_add(n32);
            }
            Counter::DuplicateSessions => {
                st.duplicate_sessions = st.duplicate_sessions.saturating_add(n32);
            }
            Counter::SessionErrors => st.session_errors = st.session_errors.saturating_add(n32),
            Counter::PasswordViolations => {
                st.password_violations = st.password_violations.saturating_add(n32);
            }
            Counter::UnknownUsers => st.unknown_users = st.unknown_users.saturating_add(n32),
            Counter::BytesSent => st.bytes_sent = st.bytes_sent.saturating_add(n),
            Counter::BytesReceived => st.bytes_received = st.bytes_received.saturating_add(n),
            Counter::Response => {
                st.requests += 1;
                st.response_ms = st.response_ms.saturating_add(n);
            }
        }
    }

    /// Every session, by identifier.
    pub fn sessions(&self) -> impl Iterator<Item = &Session> {
        self.sessions.values()
    }

    /// One session.
    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Session> {
        self.sessions.get(&id)
    }

    /// Mark session `id` to end; the job serving it.
    pub fn end(&mut self, id: u64) -> Result<String, TableError> {
        let s = self
            .sessions
            .get_mut(&id)
            .ok_or(TableError::NoSession(id))?;
        s.ending = true;
        Ok(s.job.clone())
    }

    /// The sessions of a workstation (its name or expanded name, case
    /// not significant).
    #[must_use]
    pub fn of_workstation(&self, ws: &str) -> Vec<u64> {
        self.sessions
            .values()
            .filter(|s| {
                s.workstation.eq_ignore_ascii_case(ws) || s.expanded.eq_ignore_ascii_case(ws)
            })
            .map(|s| s.id)
            .collect()
    }

    /// The sessions a job registered.
    #[must_use]
    pub fn of_job(&self, job: &str) -> Vec<u64> {
        self.sessions
            .values()
            .filter(|s| s.job == job)
            .map(|s| s.id)
            .collect()
    }

    /// Whether session `id` was asked to end.
    #[must_use]
    pub fn is_ending(&self, id: u64) -> bool {
        self.sessions.get(&id).is_some_and(|s| s.ending)
    }

    /// The job ended: forget its sessions (an error end for each).
    pub fn job_ended(&mut self, job: &str) -> usize {
        let ids = self.of_job(job);
        for id in &ids {
            self.sessions.remove(id);
            self.stats.session_errors += 1;
        }
        ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(ws: &str, user: &str) -> Session {
        Session {
            id: 0,
            workstation: ws.into(),
            expanded: format!("{ws}.example"),
            user: user.into(),
            job: "000123/QUSER/QZLSFILE".into(),
            guest: false,
            since: 100,
            last_active: 100,
            connections: Vec::new(),
            ending: false,
        }
    }

    #[test]
    fn sessions_register_connect_and_end() {
        let mut t = SessionTable::new();
        t.started(50);
        let a = t.register(session("PC1", "BOB"));
        let b = t.register(session("PC2", "ANN"));
        assert_ne!(a, b);
        let c = t.connect(a, "qibm", 0, 110).expect("connects");
        t.set_opens(a, c, 3, 120).expect("opens");
        t.set_opens(a, c, 1, 121).expect("closes");
        assert_eq!(t.get(a).map(Session::opens), Some(1));
        assert_eq!(t.stats.opens, 3);
        assert_eq!(
            t.get(a).map(|s| s.connections[0].share.clone()),
            Some("QIBM".into())
        );
        assert_eq!(t.of_workstation("pc1"), vec![a]);
        assert_eq!(t.of_workstation("PC2.EXAMPLE"), vec![b]);
        assert_eq!(t.end(a).as_deref(), Ok("000123/QUSER/QZLSFILE"));
        assert!(t.is_ending(a));
        t.unregister(a, false).expect("ends");
        t.unregister(b, true).expect("idles out");
        assert_eq!(t.unregister(b, true), Err(TableError::NoSession(b)));
        assert_eq!(t.stats.session_starts, 2);
        assert_eq!(t.stats.normal_disconnects, 1);
        assert_eq!(t.stats.auto_disconnects, 1);
        assert_eq!(t.disconnect(9, 1), Err(TableError::NoSession(9)));
    }

    #[test]
    fn counters_and_a_job_that_ends() {
        let mut t = SessionTable::new();
        t.count(Counter::Response, 10);
        t.count(Counter::Response, 30);
        t.count(Counter::BytesSent, 5_000_000_000);
        assert_eq!(t.stats.average_response(), 20);
        assert_eq!(t.stats.bytes_sent, 5_000_000_000);
        t.register(session("PC1", "BOB"));
        t.register(session("PC1", "BOB"));
        assert_eq!(t.job_ended("000123/QUSER/QZLSFILE"), 2);
        assert_eq!(t.sessions().count(), 0);
        assert_eq!(t.stats.session_errors, 2);
    }

    #[test]
    fn maximum_users_count_across_jobs() {
        let mut t = SessionTable::new();
        let a = t.register(session("PC1", "BOB"));
        let mut other = session("PC2", "ANN");
        other.job = "000124/QUSER/QZLSFILE".into();
        let b = t.register(other);
        assert!(t.connect_within(a, "data", 0, 1, 5).is_ok());
        assert_eq!(
            t.connect_within(b, "DATA", 0, 1, 5),
            Err(TableError::MaxUsers("DATA".into()))
        );
        assert!(t.connect_within(b, "DATA", 0, -1, 5).is_ok());
        assert!(t.connect_within(b, "other", 0, 1, 5).is_ok());
        // A second tree of a session that has the share is no new user.
        assert!(t.connect_within(a, "DATA", 0, 1, 5).is_ok());
    }
}
