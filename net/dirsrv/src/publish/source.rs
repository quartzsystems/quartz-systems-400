// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where the publisher's objects and changes come from
//! ([`PublishSource`]): the service's lists of profiles and printers and
//! the log of their changes ([`Tracker`], the service's half), and a
//! source in memory for tests ([`MemSource`]).

use std::collections::{BTreeMap, VecDeque};

use super::config::{Kind, PublishConfig};
use super::map::Object;
use super::publisher::Status;

/// A place in the change log: the service's epoch and a change number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    /// Taken when the service started.
    pub epoch: u64,
    /// The last change seen.
    pub seq: u64,
}

/// One change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A new object.
    Created(Object),
    /// An object whose published values changed.
    Changed(Object),
    /// An object gone: its kind and name.
    Deleted(Kind, String),
}

/// What [`PublishSource::changes`] answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// The cursor to ask from next.
    pub next: Cursor,
    /// The log no longer reaches the cursor: list everything again.
    pub resync: bool,
    /// The changes after the cursor, oldest first.
    pub changes: Vec<Change>,
}

/// What [`PublishSource::list`] answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    /// The changes the list includes.
    pub cursor: Cursor,
    /// Every object of the kind.
    pub objects: Vec<Object>,
}

/// The configuration as a source gives it to a publishing job.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceConfig {
    /// The file.
    pub config: PublishConfig,
    /// Each agent's password in the clear, when it has one.
    pub passwords: Vec<(Kind, String)>,
    /// The system's name (the default publisher value).
    pub system: String,
}

impl SourceConfig {
    /// `kind`'s password.
    #[must_use]
    pub fn password(&self, kind: Kind) -> Option<&str> {
        self.passwords
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, p)| p.as_str())
    }
}

/// What a publishing agent reads: the service implements it over the
/// link in the job, tests with [`MemSource`].
pub trait PublishSource {
    /// The configuration, with the passwords opened.
    ///
    /// # Errors
    ///
    /// Why it cannot be had (a bad file, the service).
    fn config(&mut self) -> Result<SourceConfig, String>;

    /// Every object of `kind`.
    ///
    /// # Errors
    ///
    /// The service's.
    fn list(&mut self, kind: Kind) -> Result<Listing, String>;

    /// The changes of `kind` after `since`; `status` is the agent's,
    /// reported with the ask.
    ///
    /// # Errors
    ///
    /// The service's.
    fn changes(&mut self, kind: Kind, since: Cursor, status: &Status) -> Result<Changes, String>;
}

/// The most changes a kind's log keeps.
pub const LOG_LIMIT: usize = 1024;

#[derive(Debug, Default)]
struct KindLog {
    current: BTreeMap<String, Object>,
    listed: bool,
    log: VecDeque<(u64, Change)>,
    /// The highest change number dropped from the log.
    dropped: u64,
}

/// The service's half: the objects last listed per kind and the changes
/// between listings (docs/DIRSRV.md §10.1, change events).
#[derive(Debug)]
pub struct Tracker {
    epoch: u64,
    seq: u64,
    limit: usize,
    kinds: [KindLog; 2],
}

impl Tracker {
    /// A tracker of epoch `epoch` keeping [`LOG_LIMIT`] changes a kind.
    #[must_use]
    pub fn new(epoch: u64) -> Self {
        Self::with_limit(epoch, LOG_LIMIT)
    }

    /// A tracker keeping `limit` changes a kind.
    #[must_use]
    pub fn with_limit(epoch: u64, limit: usize) -> Self {
        Self {
            epoch,
            seq: 0,
            limit: limit.max(1),
            kinds: [KindLog::default(), KindLog::default()],
        }
    }

    /// The cursor of now.
    #[must_use]
    pub const fn cursor(&self) -> Cursor {
        Cursor {
            epoch: self.epoch,
            seq: self.seq,
        }
    }

    /// Whether `kind` was listed yet.
    #[must_use]
    pub fn listed(&self, kind: Kind) -> bool {
        self.kinds[kind as usize].listed
    }

    /// Take a fresh list of `kind`: what differs from the last one is
    /// logged (nothing at the first).
    pub fn update(&mut self, kind: Kind, objects: Vec<Object>) {
        let mut now: BTreeMap<String, Object> = BTreeMap::new();
        for o in objects.into_iter().filter(|o| o.kind() == kind) {
            now.insert(o.key().to_string(), o);
        }
        let k = &mut self.kinds[kind as usize];
        if !k.listed {
            k.current = now;
            k.listed = true;
            return;
        }
        let mut changes = Vec::new();
        for (name, o) in &now {
            match k.current.get(name) {
                None => changes.push(Change::Created(o.clone())),
                Some(old) if old != o => changes.push(Change::Changed(o.clone())),
                Some(_) => {}
            }
        }
        for name in k.current.keys() {
            if !now.contains_key(name) {
                changes.push(Change::Deleted(kind, name.clone()));
            }
        }
        k.current = now;
        for c in changes {
            self.seq += 1;
            k.log.push_back((self.seq, c));
            while k.log.len() > self.limit {
                if let Some((n, _)) = k.log.pop_front() {
                    k.dropped = n;
                }
            }
        }
    }

    /// Every object of `kind` as last listed.
    #[must_use]
    pub fn listing(&self, kind: Kind) -> Listing {
        Listing {
            cursor: self.cursor(),
            objects: self.kinds[kind as usize]
                .current
                .values()
                .cloned()
                .collect(),
        }
    }

    /// The changes of `kind` after `since`, or a resync when the log no
    /// longer reaches it.
    #[must_use]
    pub fn since(&self, kind: Kind, since: Cursor) -> Changes {
        let k = &self.kinds[kind as usize];
        let next = self.cursor();
        if since.epoch != self.epoch || since.seq > self.seq || since.seq < k.dropped {
            return Changes {
                next,
                resync: true,
                changes: Vec::new(),
            };
        }
        Changes {
            next,
            resync: false,
            changes: k
                .log
                .iter()
                .filter(|(n, _)| *n > since.seq)
                .map(|(_, c)| c.clone())
                .collect(),
        }
    }
}

/// A source in memory: a configuration and a tracker the test changes.
#[derive(Debug)]
pub struct MemSource {
    /// What [`PublishSource::config`] answers.
    pub config: Result<SourceConfig, String>,
    /// The objects and their changes.
    pub tracker: Tracker,
    /// The objects now, by kind; taken into the tracker at each ask.
    pub objects: Vec<Object>,
    /// The statuses reported, last last.
    pub reported: Vec<(Kind, Status)>,
    /// When set, every list and changes call fails with it.
    pub fail: Option<String>,
}

impl MemSource {
    /// A source of `config` and no objects.
    #[must_use]
    pub fn new(config: SourceConfig) -> Self {
        Self {
            config: Ok(config),
            tracker: Tracker::new(1),
            objects: Vec::new(),
            reported: Vec::new(),
            fail: None,
        }
    }

    fn take(&mut self, kind: Kind) -> Result<(), String> {
        if let Some(e) = &self.fail {
            return Err(e.clone());
        }
        self.tracker.update(kind, self.objects.clone());
        Ok(())
    }
}

impl PublishSource for MemSource {
    fn config(&mut self) -> Result<SourceConfig, String> {
        self.config.clone()
    }

    fn list(&mut self, kind: Kind) -> Result<Listing, String> {
        self.take(kind)?;
        Ok(self.tracker.listing(kind))
    }

    fn changes(&mut self, kind: Kind, since: Cursor, status: &Status) -> Result<Changes, String> {
        self.reported.push((kind, status.clone()));
        self.take(kind)?;
        Ok(self.tracker.since(kind, since))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish::map::UserInfo;

    fn u(name: &str, text: &str) -> Object {
        Object::User(UserInfo {
            name: name.into(),
            text: text.into(),
            mail: None,
        })
    }

    #[test]
    fn logs_what_changed_between_listings() {
        let mut t = Tracker::new(7);
        t.update(Kind::Users, vec![u("A", "a"), u("B", "b")]);
        let start = t.cursor();
        assert_eq!(start, Cursor { epoch: 7, seq: 0 });
        assert_eq!(t.listing(Kind::Users).objects.len(), 2);
        t.update(Kind::Users, vec![u("A", "a2"), u("C", "c")]);
        let c = t.since(Kind::Users, start);
        assert!(!c.resync);
        assert_eq!(
            c.changes,
            vec![
                Change::Changed(u("A", "a2")),
                Change::Created(u("C", "c")),
                Change::Deleted(Kind::Users, "B".into()),
            ]
        );
        assert_eq!(c.next.seq, 3);
        // Nothing new after the next cursor; the same list logs nothing.
        t.update(Kind::Users, vec![u("A", "a2"), u("C", "c")]);
        assert!(t.since(Kind::Users, c.next).changes.is_empty());
        // Printers are another log.
        assert!(t.since(Kind::Printers, start).changes.is_empty());
    }

    #[test]
    fn a_cursor_out_of_reach_is_a_resync() {
        let mut t = Tracker::with_limit(3, 2);
        t.update(Kind::Users, vec![]);
        let start = t.cursor();
        t.update(Kind::Users, vec![u("A", ""), u("B", ""), u("C", "")]);
        // Three changes, two kept: the start is out of reach.
        assert!(t.since(Kind::Users, start).resync);
        let mid = Cursor { epoch: 3, seq: 1 };
        let c = t.since(Kind::Users, mid);
        assert!(!c.resync);
        assert_eq!(c.changes.len(), 2);
        // Another epoch (the service restarted), or a cursor from the future.
        assert!(t.since(Kind::Users, Cursor { epoch: 2, seq: 3 }).resync);
        assert!(t.since(Kind::Users, Cursor { epoch: 3, seq: 9 }).resync);
    }

    #[test]
    fn objects_of_another_kind_are_not_taken() {
        let mut t = Tracker::new(1);
        t.update(Kind::Printers, vec![u("A", "")]);
        assert!(t.listing(Kind::Printers).objects.is_empty());
        assert!(t.listed(Kind::Printers));
        assert!(!t.listed(Kind::Users));
    }
}
