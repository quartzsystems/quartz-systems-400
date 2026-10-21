// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What can go wrong with ILE objects and the binder.

/// Result with an [`IleError`].
pub type Result<T> = std::result::Result<T, IleError>;

/// An ILE error.
#[derive(Debug, thiserror::Error)]
pub enum IleError {
    /// A payload that cannot be read.
    #[error("{what}: {msg}")]
    Payload {
        /// Which object type.
        what: &'static str,
        /// What was wrong.
        msg: String,
    },

    /// The binder found imports nothing exports (CPD5D02).
    #[error("CPD5D02: unresolved imports: {0}")]
    Unresolved(String),

    /// A symbol the binder source exports does not exist (CPD5D03).
    #[error("CPD5D03: export {0} is not defined in any module")]
    NoSuchExport(String),

    /// No module has a program entry procedure (CPD5D06).
    #[error("CPD5D06: no module has a program entry procedure")]
    NoEntry,

    /// Binder source that does not parse.
    #[error("binder source line {line}: {msg}")]
    BinderSource {
        /// Line, from 1.
        line: u32,
        /// What was wrong.
        msg: String,
    },

    /// A signature value that is not 16 bytes or 32 hex digits.
    #[error("signature {0:?} is not valid")]
    Signature(String),

    /// The IR did not read back.
    #[error(transparent)]
    Ir(#[from] qir::QirError),

    /// Code generation failed.
    #[error(transparent)]
    Codegen(#[from] codegen::CodegenError),

    /// The store refused.
    #[error(transparent)]
    Store(#[from] store::StoreError),

    /// A name that is not an object name.
    #[error(transparent)]
    Name(#[from] qsname::NameError),

    /// An object of the wrong type was named.
    #[error("{name} is a {found}, not a {wanted}")]
    WrongType {
        /// The name.
        name: String,
        /// What it is.
        found: String,
        /// What was needed.
        wanted: String,
    },
}

impl IleError {
    pub(crate) fn payload(what: &'static str, msg: impl Into<String>) -> Self {
        Self::Payload {
            what,
            msg: msg.into(),
        }
    }
}
