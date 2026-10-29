// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The character buffer under the lexer: line counting, line
//! continuations and alias text spliced in place.

/// The source characters and a cursor.
#[derive(Debug)]
pub(super) struct Input {
    pub(super) chars: Vec<char>,
    pub(super) pos: usize,
    pub(super) line: u32,
    /// Alias text spliced in: (end of the text, alias name). An alias
    /// is not expanded again while the cursor is inside its own text.
    pub(super) aliases: Vec<(usize, String)>,
}

impl Input {
    pub(super) fn new(src: &str, line: u32) -> Self {
        Self {
            chars: src.chars().collect(),
            pos: 0,
            line,
            aliases: Vec::new(),
        }
    }

    /// The raw character at the cursor.
    pub(super) fn raw(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// The raw character `n` past the cursor.
    pub(super) fn raw_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    /// Consume one raw character.
    pub(super) fn bump(&mut self) -> Option<char> {
        let c = self.raw()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    /// Skip any backslash-newline pairs at the cursor.
    pub(super) fn skip_cont(&mut self) {
        while self.raw() == Some('\\') && self.raw_at(1) == Some('\n') {
            self.pos += 2;
            self.line += 1;
        }
    }

    /// The character at the cursor after line continuations.
    pub(super) fn peek(&mut self) -> Option<char> {
        self.skip_cont();
        self.raw()
    }

    /// The character after the one at the cursor, continuations skipped
    /// before the first one only.
    pub(super) fn peek2(&mut self) -> Option<char> {
        self.skip_cont();
        let mut i = self.pos + 1;
        while self.chars.get(i) == Some(&'\\') && self.chars.get(i + 1) == Some(&'\n') {
            i += 2;
        }
        self.chars.get(i).copied()
    }

    /// Consume one character after line continuations.
    pub(super) fn next(&mut self) -> Option<char> {
        self.skip_cont();
        self.bump()
    }

    /// The text between two offsets.
    pub(super) fn text(&self, start: usize, end: usize) -> String {
        let end = end.min(self.chars.len());
        let start = start.min(end);
        self.chars[start..end].iter().collect()
    }

    /// Is the alias `name` being expanded at the cursor?
    pub(super) fn alias_active(&self, name: &str) -> bool {
        self.aliases
            .iter()
            .any(|(end, n)| self.pos < *end && n == name)
    }

    /// Replace `start..end` with `text`, the expansion of alias `name`,
    /// and put the cursor at its start.
    pub(super) fn splice(&mut self, start: usize, end: usize, text: &str, name: &str) {
        let new: Vec<char> = text.chars().collect();
        let added = new.len();
        let removed = end - start;
        // Lines consumed by the replaced token are counted again.
        let lines = self.chars[start..end]
            .iter()
            .filter(|&&c| c == '\n')
            .count();
        self.line -= u32::try_from(lines).unwrap_or(0);
        self.chars.splice(start..end, new);
        for (e, _) in &mut self.aliases {
            if *e >= end {
                *e = *e + added - removed;
            } else if *e > start {
                *e = start + added;
            }
        }
        self.aliases.retain(|(e, _)| *e > start);
        self.aliases.push((start + added, name.to_string()));
        self.pos = start;
    }
}
