// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a publishing job does at each turn (docs/DIRSRV.md §10.1): read
//! the configuration now and then, take the source's changes into the
//! publisher, let it reach the target when due, and report its status.

use super::config::{AgentConfig, Kind};
use super::publisher::{Connect, Publisher, Status};
use super::source::{Cursor, PublishSource};

/// How often the configuration is read again, seconds.
pub const CONFIG_EVERY: u64 = 60;

/// The settings a publisher was made from.
type Loaded = (AgentConfig, Option<String>, String);

/// One agent's job.
#[derive(Debug)]
pub struct Runner {
    kind: Kind,
    publisher: Option<Publisher>,
    loaded: Option<Loaded>,
    cursor: Option<Cursor>,
    next_config: u64,
    error: Option<(u64, String)>,
}

impl Runner {
    /// The job of `kind`'s agent.
    #[must_use]
    pub const fn new(kind: Kind) -> Self {
        Self {
            kind,
            publisher: None,
            loaded: None,
            cursor: None,
            next_config: 0,
            error: None,
        }
    }

    /// The agent.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    /// The publisher, while the agent is enabled.
    #[must_use]
    pub const fn publisher(&self) -> Option<&Publisher> {
        self.publisher.as_ref()
    }

    /// The status to report.
    #[must_use]
    pub fn status(&self) -> Status {
        let mut s = self
            .publisher
            .as_ref()
            .map(Publisher::status)
            .unwrap_or_default();
        if let Some((t, e)) = &self.error
            && s.last_error.as_ref().is_none_or(|(u, _)| t >= u)
        {
            s.last_error = Some((*t, e.clone()));
        }
        s
    }

    /// The configuration read again at the next turn.
    pub fn reload(&mut self) {
        self.next_config = 0;
    }

    /// Drop the target's connection (the job ends).
    pub fn close(&mut self) {
        if let Some(p) = &mut self.publisher {
            p.close();
        }
    }

    fn fail(&mut self, now: u64, e: String) {
        tracing::warn!(agent = self.kind.name(), "publishing: {e}");
        self.error = Some((now, e));
    }

    fn configure(&mut self, source: &mut dyn PublishSource, now: u64) {
        self.next_config = now + CONFIG_EVERY;
        let sc = match source.config() {
            Ok(c) => c,
            Err(e) => return self.fail(now, format!("configuration: {e}")),
        };
        let a = sc.config.agent(self.kind).clone();
        let loaded = (
            a.clone(),
            sc.password(self.kind).map(str::to_string),
            sc.system.clone(),
        );
        if self.loaded.as_ref() == Some(&loaded) {
            return;
        }
        self.close();
        self.publisher = None;
        self.cursor = None;
        self.loaded = Some(loaded.clone());
        if a.enabled {
            match Publisher::new(a, loaded.1, &loaded.2) {
                Ok(p) => self.publisher = Some(p),
                Err(e) => self.fail(now, e),
            }
        }
    }

    fn list(&mut self, source: &mut dyn PublishSource, now: u64) {
        match source.list(self.kind) {
            Ok(l) => {
                if let Some(p) = &mut self.publisher {
                    p.resync(&l.objects);
                }
                self.cursor = Some(l.cursor);
            }
            Err(e) => self.fail(now, format!("listing: {e}")),
        }
    }

    /// One turn at `now` (seconds since the epoch).
    pub fn tick(&mut self, source: &mut dyn PublishSource, connect: &mut dyn Connect, now: u64) {
        if now >= self.next_config {
            self.configure(source, now);
        }
        let status = self.status();
        let Some(p) = &self.publisher else {
            // Disabled: only the status is reported.
            if let Err(e) = source.changes(self.kind, Cursor::default(), &status) {
                self.fail(now, format!("status: {e}"));
            }
            return;
        };
        match self.cursor {
            Some(_) if p.resync_due(now) => self.list(source, now),
            None => self.list(source, now),
            Some(c) => match source.changes(self.kind, c, &status) {
                Ok(ch) if ch.resync => self.list(source, now),
                Ok(ch) => {
                    if let Some(p) = &mut self.publisher {
                        for c in &ch.changes {
                            p.apply(c);
                        }
                    }
                    self.cursor = Some(ch.next);
                }
                Err(e) => self.fail(now, format!("changes: {e}")),
            },
        }
        if let Some(p) = &mut self.publisher {
            p.run(connect, now);
        }
    }
}
