// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Compiler diagnostics: `DDSnnnn` IDs with Quartz text and IBM-style
//! severities (00 information, 10 warning, 20 error, 30 severe). A
//! severity of 20 or more stops the definition from being used.

pub mod ids;

use std::fmt;

/// How bad a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Information only.
    Info,
    /// The definition is still usable.
    Warning,
    /// The definition is not usable.
    Error,
    /// Compilation could not continue.
    Severe,
}

impl Severity {
    /// The two-digit code listings show.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Warning => 10,
            Self::Error => 20,
            Self::Severe => 30,
        }
    }
}

/// One message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// `DDSnnnn`.
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
    /// Add one.
    pub fn push(&mut self, d: Diagnostic) {
        self.items.push(d);
    }

    /// Add several.
    pub fn extend(&mut self, items: impl IntoIterator<Item = Diagnostic>) {
        self.items.extend(items);
    }

    /// The worst severity so far.
    #[must_use]
    pub fn max_severity(&self) -> Option<Severity> {
        self.items.iter().map(|d| d.severity).max()
    }

    /// Whether anything at severity 20 or more was reported.
    #[must_use]
    pub fn blocks_create(&self) -> bool {
        self.max_severity() >= Some(Severity::Error)
    }

    /// The messages.
    #[must_use]
    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.items
    }

    /// Take the messages.
    #[must_use]
    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }
}
