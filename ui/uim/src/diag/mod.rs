// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Compiler diagnostics: `UIMnnnn` identifiers with Quartz text and
//! IBM-style severities (10 warning, 20 error, 30 severe, 40 terminal).
//! A severity of 20 or more stops the panel group from being created.

pub mod ids;

use std::fmt;

/// How bad a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// The group is still created.
    Warning,
    /// The group is not created.
    Error,
    /// A whole element was dropped.
    Severe,
    /// Compilation could not continue.
    Terminal,
}

impl Severity {
    /// The two-digit code listings show.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Warning => 10,
            Self::Error => 20,
            Self::Severe => 30,
            Self::Terminal => 40,
        }
    }
}

/// One message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// `UIMnnnn`.
    pub id: &'static str,
    /// Severity.
    pub severity: Severity,
    /// Source line index (0-based), when the message is about a line.
    pub line: Option<usize>,
    /// The text.
    pub text: String,
}

impl Diagnostic {
    /// A new message.
    pub fn new(
        id: &'static str,
        severity: Severity,
        line: Option<usize>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id,
            severity,
            line,
            text: text.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:02} {}", self.id, self.severity.code(), self.text)
    }
}

/// The messages of one compilation.
#[derive(Clone, Debug, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    /// No messages yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one.
    pub fn push(&mut self, d: Diagnostic) {
        self.items.push(d);
    }

    /// Add every message of another set.
    pub fn extend(&mut self, other: Diagnostics) {
        self.items.extend(other.items);
    }

    /// The messages, in the order sent.
    #[must_use]
    pub fn items(&self) -> &[Diagnostic] {
        &self.items
    }

    /// The highest severity, if any message was sent.
    #[must_use]
    pub fn max_severity(&self) -> Option<Severity> {
        self.items.iter().map(|d| d.severity).max()
    }

    /// No message of severity 20 or more.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.max_severity().is_none_or(|s| s < Severity::Error)
    }

    /// The messages, consumed.
    #[must_use]
    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }

    /// The messages sorted by line, the line-less ones first.
    pub fn sort(&mut self) {
        self.items.sort_by_key(|d| d.line.map_or(0, |l| l + 1));
    }
}
