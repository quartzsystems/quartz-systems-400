// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Interactive SQL (`STRSQL`): the session's attributes ([`attrs`]),
//! the statement history and the lines being typed ([`history`]),
//! statements built from the prompt displays ([`assemble`]), a query's
//! rows paged and shifted for Display Data or laid out for print
//! ([`result`]), the saved session ([`saved`]) and the displays
//! ([`screen`]). No store and no screen I/O: the job's driver
//! (`work/qsjob/src/devtools/isql.rs`) runs the statements through the
//! storage service.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod assemble;
pub mod attrs;
pub mod history;
pub mod result;
pub mod saved;
pub mod screen;

pub use attrs::Attrs;
pub use history::History;
pub use result::Data;

/// Why a value or a prompt cannot be taken.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IsqlError {
    /// A session attribute outside its set.
    #[error("Value {value} is not valid for {field}.")]
    BadValue {
        /// The attribute's label.
        field: &'static str,
        /// What was typed.
        value: String,
    },
    /// A prompt is missing a value it needs.
    #[error("A value is needed for {0}.")]
    Missing(&'static str),
    /// A saved session that cannot be read.
    #[error("The saved session is damaged: {0}.")]
    Damaged(String),
}
