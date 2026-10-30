// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The objects DDM and DRDA keep: the relational database directory
//! (`ADDRDBDIRE`, `CHGRDBDIRE`, `RMVRDBDIRE`, `WRKRDBDIRE`) with its
//! `*LOCAL` entry, DDM files (`CRTDDMF`, `CHGDDMF`) and the DDM server's
//! TCP/IP attributes (`CHGDDMTCPA`). Each is a payload that encodes to
//! `KEYWORD(value)` text; `panel` lays out the WRKRDBDIRE displays.

pub mod ddmf;
pub mod panel;
pub mod rdbdir;
pub mod tcpa;
mod text;

pub use ddmf::DdmFile;
pub use rdbdir::{RdbDirectory, RdbEntry};
pub use tcpa::DdmTcpAttrs;

/// What a directory or file change refused, with the IBM message ID the
/// command sends.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{id} {text}")]
pub struct Refused {
    /// The message ID.
    pub id: &'static str,
    /// Quartz text.
    pub text: String,
}

impl Refused {
    fn new(id: &'static str, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
        }
    }
}
