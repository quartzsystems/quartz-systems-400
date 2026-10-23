// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Data File Utility and the Report Layout Utility: field values
//! in and out of records ([`field`]), the data entry session's modes
//! and counts ([`session`]), DFU program definitions ([`program`]),
//! the printer file page image ([`rlu`]) and their displays
//! ([`screen`]). No store and no 5250 I/O: the job's drivers
//! (`work/qsjob/src/devtools/dfu.rs`, `rlu.rs`) do those.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod field;
pub mod program;
pub mod rlu;
pub mod screen;
pub mod session;

pub use field::Field;
pub use program::Program;
pub use session::{Mode, Session};

/// Why a value or a definition is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DfuError {
    /// A numeric field holds something else.
    #[error("Field {0} must be numeric.")]
    NotNumeric(String),
    /// The number does not fit.
    #[error("The value of field {0} is too large.")]
    TooLarge(String),
    /// A character the code page lacks.
    #[error("Field {0} holds a character that cannot be stored.")]
    BadData(String),
    /// The field does not fit the record.
    #[error("Field {0} lies outside the record.")]
    Layout(String),
    /// Not a DFU program's payload.
    #[error("The program is not a DFU program.")]
    NotDfu,
}
