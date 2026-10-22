// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Compiler diagnostics: `CPD07xx` messages with the statement they
//! belong to, kept for the listing's message summary.

use std::fmt;

/// Severity of a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Information (0).
    Info,
    /// Warning (10): the program is still created.
    Warning,
    /// Error (30): the program is not created.
    Error,
    /// Terminal (40).
    Terminal,
}

impl Severity {
    /// The numeric severity.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Warning => 10,
            Self::Error => 30,
            Self::Terminal => 40,
        }
    }
}

/// One diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diag {
    /// Source line (0 for the whole program).
    pub line: usize,
    /// `CPDnnnn`.
    pub msgid: String,
    /// Severity.
    pub severity: Severity,
    /// Text (ours).
    pub text: String,
}

impl Diag {
    /// An error at a line.
    #[must_use]
    pub fn at(line: usize, msgid: &str, text: impl Into<String>) -> Self {
        Self {
            line,
            msgid: msgid.into(),
            severity: Severity::Error,
            text: text.into(),
        }
    }

    /// A warning at a line.
    #[must_use]
    pub fn warn(line: usize, msgid: &str, text: impl Into<String>) -> Self {
        Self {
            line,
            msgid: msgid.into(),
            severity: Severity::Warning,
            text: text.into(),
        }
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {:02} {}",
            self.msgid,
            self.severity.number(),
            self.text
        )
    }
}

/// The compiler's identifiers (`CPD07xx` is IBM's range for CL
/// compiler diagnostics; the numbers here are ours until the machine
/// settles them, the texts are ours).
pub mod ids {
    /// A statement the compiler does not know.
    pub const UNKNOWN_STATEMENT: &str = "CPD0701";
    /// A variable used but not declared.
    pub const UNDECLARED: &str = "CPD0727";
    /// A variable declared twice.
    pub const REDECLARED: &str = "CPD0728";
    /// Types do not match.
    pub const TYPE_MISMATCH: &str = "CPD0731";
    /// A label used but not defined.
    pub const NO_LABEL: &str = "CPD0735";
    /// Structure: `ENDDO` without `DO` and the like.
    pub const STRUCTURE: &str = "CPD0740";
    /// A keyword or value the statement does not take.
    pub const BAD_VALUE: &str = "CPD0744";
    /// A required keyword is missing.
    pub const MISSING: &str = "CPD0745";
    /// An expression is not well formed.
    pub const EXPRESSION: &str = "CPD0750";
    /// A built-in function with the wrong arguments.
    pub const BUILTIN: &str = "CPD0751";
    /// A declaration after the first executable statement.
    pub const DECL_ORDER: &str = "CPD0756";
    /// `PGM` is not first, or missing.
    pub const PGM_FIRST: &str = "CPD0760";
    /// A command the analyzer rejected.
    pub const COMMAND: &str = "CPD0770";
    /// Something this release does not compile.
    pub const UNSUPPORTED: &str = "CPD0790";
    /// A subroutine used but not defined, or defined twice.
    pub const SUBR: &str = "CPD0795";
}
