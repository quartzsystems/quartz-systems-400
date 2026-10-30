// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The lines being edited, each with a number of its own so a line
//! command can name it while other commands move it. A line an insert
//! made and nobody typed into yet is `fresh`: the next Enter drops it
//! if it is still empty, as IBM's editors drop unused insert lines.

/// One line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    /// Its number (never 0: 0 names the top of the file).
    pub id: u64,
    /// The text, without its line end.
    pub text: String,
    /// Made by an insert and not typed into.
    pub fresh: bool,
}

/// The lines and how many changes they carry since the last save.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Buffer {
    lines: Vec<Line>,
    changes: usize,
    next: u64,
}

impl Buffer {
    /// Over a file's lines.
    #[must_use]
    pub fn new(lines: Vec<String>) -> Self {
        let mut b = Self::default();
        for text in lines {
            let id = b.id();
            b.lines.push(Line {
                id,
                text,
                fresh: false,
            });
        }
        b
    }

    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// How many lines.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The lines.
    #[must_use]
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Line `i`'s text.
    #[must_use]
    pub fn text(&self, i: usize) -> &str {
        self.lines.get(i).map_or("", |l| l.text.as_str())
    }

    /// Where the line numbered `id` is now.
    #[must_use]
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.lines.iter().position(|l| l.id == id)
    }

    /// The texts, for a save.
    #[must_use]
    pub fn texts(&self) -> Vec<String> {
        self.lines.iter().map(|l| l.text.clone()).collect()
    }

    /// Changes made since the file was read or saved.
    #[must_use]
    pub fn changes(&self) -> usize {
        self.changes
    }

    /// The file was saved.
    pub fn saved(&mut self) {
        self.changes = 0;
    }

    /// Count a change made outside the lines (a CCSID, a line end).
    pub fn touch(&mut self) {
        self.changes += 1;
    }

    /// Replace line `i`'s text.
    pub fn set(&mut self, i: usize, text: String) {
        if let Some(l) = self.lines.get_mut(i) {
            if l.text != text {
                l.text = text;
                self.changes += 1;
            }
            l.fresh = false;
        }
    }

    /// Insert `n` fresh lines before index `at`; their numbers.
    pub fn insert_fresh(&mut self, at: usize, n: usize) -> Vec<u64> {
        let at = at.min(self.lines.len());
        let mut ids = Vec::new();
        for k in 0..n {
            let id = self.id();
            ids.push(id);
            self.lines.insert(
                at + k,
                Line {
                    id,
                    text: String::new(),
                    fresh: true,
                },
            );
        }
        ids
    }

    /// Insert lines with text before index `at`.
    pub fn insert(&mut self, at: usize, texts: Vec<String>) {
        let at = at.min(self.lines.len());
        self.changes += texts.len();
        for (k, text) in texts.into_iter().enumerate() {
            let id = self.id();
            self.lines.insert(
                at + k,
                Line {
                    id,
                    text,
                    fresh: false,
                },
            );
        }
    }

    /// Remove lines `from..to`, answering their texts.
    pub fn remove(&mut self, from: usize, to: usize) -> Vec<String> {
        let to = to.min(self.lines.len());
        if from >= to {
            return Vec::new();
        }
        let gone: Vec<Line> = self.lines.drain(from..to).collect();
        self.changes += gone.iter().filter(|l| !l.fresh).count();
        gone.into_iter().map(|l| l.text).collect()
    }

    /// Drop the fresh lines nobody typed into; how many went before
    /// index `before` (so a position can follow).
    pub fn drop_fresh(&mut self, before: usize) -> usize {
        let mut gone = 0;
        let mut i = 0;
        self.lines.retain(|l| {
            let keep = !l.fresh;
            if !keep && i < before {
                gone += 1;
            }
            i += 1;
            keep
        });
        gone
    }
}
