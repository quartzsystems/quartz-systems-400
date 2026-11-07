// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Compiler diagnostics: one message a compiler reports, where it points
//! and how severe it is. Every compiler fills a [`Sink`] while it runs;
//! the job keeps the last compile's list for QAP's `Develop.Compile`,
//! and the front-ends hand the same values to the language servers.
//! The shapes are QAP version 8's `Diagnostic` and `Where`
//! (`docs/QAP.md`). No dependency on anything else in the tree, so the
//! front-ends can build it anywhere.

#![forbid(unsafe_code)]

pub mod front;
pub mod msgtext;

/// A source member: a source physical file and one of its members.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MemberName {
    /// The file's library.
    pub library: String,
    /// The source physical file.
    pub file: String,
    /// The member.
    pub member: String,
}

/// Where a diagnostic points.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Where {
    /// A line of a source member (the one compiled, or a `/COPY` member).
    Member(MemberName),
    /// A line of a stream file (`SRCSTMF`, an `/INCLUDE` path).
    Path(String),
    /// Nowhere in the source: the command, the object, the binder.
    #[default]
    None,
}

/// One diagnostic, as the listing shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diagnostic {
    /// The source it points into.
    pub at: Where,
    /// The line, from 1; 0 when unknown.
    pub line: u32,
    /// The column, from 1; 0 when unknown.
    pub column: u16,
    /// The last line the diagnostic covers; 0 when it is `line`.
    pub end_line: u32,
    /// The last column it covers; 0 when unknown.
    pub end_column: u16,
    /// The message's severity, 0 to 99.
    pub severity: u8,
    /// The message identifier (`RNF7030`, `CPD0716`, `SQL0104`).
    pub id: String,
    /// The first-level text, its substitution variables filled.
    pub text: String,
    /// The second-level text.
    pub help: String,
}

impl Diagnostic {
    /// Whether this diagnostic alone stops the object being created
    /// at `limit` (a compiler's `GENLVL`, 10 for most commands).
    #[must_use]
    pub fn fails(&self, limit: u8) -> bool {
        self.severity > limit
    }

    /// The `(where, line, id)` a listing comparison keys on.
    #[must_use]
    pub fn key(&self) -> (&Where, u32, &str) {
        (&self.at, self.line, &self.id)
    }
}

/// The severity bands IBM's compilers use.
pub mod severity {
    /// Information (00).
    pub const INFO: u8 = 0;
    /// A warning the object is created with (10).
    pub const WARNING: u8 = 10;
    /// An error (20, 30).
    pub const ERROR: u8 = 20;
    /// A severe error (30).
    pub const SEVERE: u8 = 30;
    /// A terminal error: the compile stopped (40 and above).
    pub const TERMINAL: u8 = 40;

    /// The highest severity of `diags`, or 0.
    #[must_use]
    pub fn highest(diags: &[super::Diagnostic]) -> u8 {
        diags.iter().map(|d| d.severity).max().unwrap_or(0)
    }
}

/// Where a compiler sends its diagnostics as it finds them.
pub trait Sink {
    /// One diagnostic.
    fn report(&mut self, d: Diagnostic);
}

impl Sink for Vec<Diagnostic> {
    fn report(&mut self, d: Diagnostic) {
        self.push(d);
    }
}

/// The second-level text of message `id` with its values, as the
/// message file has it; the caller owns the message files.
pub type Help<'a> = &'a mut dyn FnMut(&str, &[String]) -> String;

/// No second-level text: for callers without message files.
#[must_use]
pub fn no_help(_: &str, _: &[String]) -> String {
    String::new()
}

/// A sink that drops everything, for callers that only want the object.
#[derive(Clone, Copy, Debug, Default)]
pub struct Discard;

impl Sink for Discard {
    fn report(&mut self, _: Diagnostic) {}
}

/// The compiler commands whose diagnostics the job keeps.
pub const COMMANDS: &[&str] = &[
    "CRTBNDRPG",
    "CRTRPGMOD",
    "CRTSQLRPGI",
    "CRTBNDCL",
    "CRTCLMOD",
    "CRTPF",
    "CRTLF",
    "CRTDSPF",
    "CRTPRTF",
    "CRTCMD",
    "RUNSQLSTM",
    "CRTPGM",
    "CRTSRVPGM",
];

/// Whether command `name` (upper case) is one of [`COMMANDS`].
#[must_use]
pub fn is_compiler(name: &str) -> bool {
    COMMANDS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vec_collects_and_the_highest_severity_counts() {
        let mut v: Vec<Diagnostic> = Vec::new();
        v.report(Diagnostic {
            severity: 10,
            id: "RNF7031".into(),
            ..Diagnostic::default()
        });
        v.report(Diagnostic {
            severity: 30,
            id: "RNF7030".into(),
            line: 42,
            ..Diagnostic::default()
        });
        assert_eq!(severity::highest(&v), 30);
        assert!(v[1].fails(10));
        assert!(!v[0].fails(10));
        assert_eq!(v[1].key(), (&Where::None, 42, "RNF7030"));
    }
}
