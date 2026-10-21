// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message files for QS/400: message descriptions with first- and
//! second-level text, severity, substitution variables, replies and
//! dump lists; substitution of message data into the text; the `*MSGF`
//! payload and its commands; and the system files `QCPFMSG`, `QCEEMSG`
//! and `QSQLMSG`, authored in `msgs/*.toml` and installed into `QSYS`
//! on first IPL. Every text is ours; the identifiers are IBM's.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;

pub mod desc;
pub mod payload;
pub mod source;
pub mod spec;
pub mod store;

pub use desc::{MsgDesc, VarType};
pub use error::{MsgfError, Result};
pub use source::{MsgFile, builtin, parse};

/// Where message descriptions are looked up at run time.
pub trait MsgLookup {
    /// The description of `msgid` in message file `msgf` of `lib`
    /// (`*LIBL` for the list), or none.
    fn describe(&mut self, msgid: &str, msgf: &str, lib: &str) -> Option<MsgDesc>;
}

/// The built-in message files, parsed from their sources; what the
/// runtime uses until a store supplies the objects.
pub struct BuiltinMsgs {
    files: Vec<MsgFile>,
}

impl BuiltinMsgs {
    /// Parse the built-in sources.
    #[must_use]
    pub fn new() -> Self {
        Self { files: builtin() }
    }

    /// Add a file, for tests.
    pub fn add(&mut self, f: MsgFile) {
        self.files.push(f);
    }
}

impl Default for BuiltinMsgs {
    fn default() -> Self {
        Self::new()
    }
}

impl MsgLookup for BuiltinMsgs {
    fn describe(&mut self, msgid: &str, msgf: &str, _lib: &str) -> Option<MsgDesc> {
        self.files
            .iter()
            .find(|f| f.name == msgf)
            .and_then(|f| f.find(msgid))
            .cloned()
    }
}
