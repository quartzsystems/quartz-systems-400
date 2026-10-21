// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! In-memory log of every SRC emitted this IPL.

use std::fmt::Write as _;
use std::time::Instant;

use crate::src::Src;

/// One emitted code.
pub struct Entry {
    /// Seconds since slic started.
    pub at: f64,
    /// The code.
    pub src: &'static Src,
}

/// The log.
pub struct IplLog {
    started: Instant,
    entries: Vec<Entry>,
}

impl IplLog {
    /// An empty log starting now.
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            entries: Vec::new(),
        }
    }

    /// Record a code.
    pub fn push(&mut self, src: &'static Src) {
        self.entries.push(Entry {
            at: self.started.elapsed().as_secs_f64(),
            src,
        });
    }

    /// Every entry, oldest first.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// Seconds since slic started.
    pub fn uptime(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// One line per entry.
    pub fn dump(&self) -> String {
        let mut s = String::new();
        for e in &self.entries {
            let _ = writeln!(s, "{:9.3}  {}  {}", e.at, e.src.code, e.src.text);
        }
        s
    }
}
