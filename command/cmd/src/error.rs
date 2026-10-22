// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Diagnostics and the error of a command: `*DIAG` messages followed
//! by one `*ESCAPE`, the way the analyzer reports.

use std::fmt;

/// One diagnostic message: its identifier and substitution data laid
/// out as the description's variables expect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diag {
    /// `CPDnnnn`.
    pub msgid: String,
    /// The message data, CP037 fixed fields.
    pub data: Vec<u8>,
    /// The parameter keyword in error, for the prompter's cursor.
    pub parm: Option<String>,
    /// A detail the text may carry, in words (ours, not IBM's).
    pub detail: String,
}

impl Diag {
    /// A diagnostic with no data.
    #[must_use]
    pub fn new(msgid: &str) -> Self {
        Self {
            msgid: msgid.into(),
            ..Self::default()
        }
    }

    /// With fixed CP037 fields of the given widths as data.
    #[must_use]
    pub fn data(mut self, fields: &[(&str, usize)]) -> Self {
        self.data = data(fields);
        self
    }

    /// With the keyword in error.
    #[must_use]
    pub fn parm(mut self, kwd: &str) -> Self {
        self.parm = Some(kwd.to_string());
        self
    }

    /// With a detail.
    #[must_use]
    pub fn detail(mut self, s: impl Into<String>) -> Self {
        self.detail = s.into();
        self
    }
}

/// Fixed CP037 fields, blank padded or truncated to each width.
#[must_use]
pub fn data(fields: &[(&str, usize)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (text, width) in fields {
        let mut b = ccsid::to_ebcdic(text).unwrap_or_else(|_| vec![0x40; *width]);
        b.resize(*width, 0x40);
        out.extend_from_slice(&b);
    }
    out
}

/// What a command analysis or run produced instead of a result: the
/// diagnostics, then the escape message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmdError {
    /// `*DIAG` messages, in order.
    pub diags: Vec<Diag>,
    /// The `*ESCAPE`: `CPF0001` for analysis errors, `CPF0006` for
    /// errors in processing, or the message the processor raised.
    pub escape: Diag,
}

impl CmdError {
    /// Diagnostics under `CPF0001` for command `name`.
    #[must_use]
    pub fn analysis(name: &str, diags: Vec<Diag>) -> Self {
        Self {
            diags,
            escape: Diag::new("CPF0001").data(&[(name, 10)]),
        }
    }

    /// One escape with no diagnostics.
    #[must_use]
    pub fn escape(msgid: &str) -> Self {
        Self {
            diags: Vec::new(),
            escape: Diag::new(msgid),
        }
    }

    /// An escape with a detail.
    #[must_use]
    pub fn detail(msgid: &str, detail: impl Into<String>) -> Self {
        Self {
            diags: Vec::new(),
            escape: Diag::new(msgid).detail(detail),
        }
    }

    /// An escape with data.
    #[must_use]
    pub fn with_data(msgid: &str, fields: &[(&str, usize)]) -> Self {
        Self {
            diags: Vec::new(),
            escape: Diag::new(msgid).data(fields),
        }
    }

    /// The most telling identifier: the first diagnostic's, else the
    /// escape's.
    #[must_use]
    pub fn msgid(&self) -> &str {
        self.diags.first().map_or(&self.escape.msgid, |d| &d.msgid)
    }

    /// Every identifier, diagnostics then escape.
    #[must_use]
    pub fn msgids(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.diags.iter().map(|d| d.msgid.as_str()).collect();
        v.push(&self.escape.msgid);
        v
    }
}

impl From<Diag> for CmdError {
    fn from(d: Diag) -> Self {
        let name = d.parm.clone().unwrap_or_default();
        Self {
            diags: vec![d],
            escape: Diag::new("CPF0001").data(&[(&name, 10)]),
        }
    }
}

impl fmt::Display for CmdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for d in &self.diags {
            write!(f, "{}", d.msgid)?;
            if !d.detail.is_empty() {
                write!(f, " {}", d.detail)?;
            }
            writeln!(f)?;
        }
        write!(f, "{}", self.escape.msgid)?;
        if !self.escape.detail.is_empty() {
            write!(f, " {}", self.escape.detail)?;
        }
        Ok(())
    }
}

impl std::error::Error for CmdError {}

impl From<store::StoreError> for CmdError {
    fn from(e: store::StoreError) -> Self {
        let id = e.msgid().unwrap_or("CPF9999");
        Self::detail(id, e.to_string())
    }
}

impl From<ipc::WireError> for CmdError {
    fn from(e: ipc::WireError) -> Self {
        Self::detail("CPF0006", format!("command definition damaged: {e}"))
    }
}

/// A result of this crate.
pub type Result<T> = std::result::Result<T, CmdError>;
