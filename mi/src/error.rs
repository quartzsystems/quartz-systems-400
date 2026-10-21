// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What can go wrong outside generated code: compiling, loading and
//! preparing a program to run.

/// Result with an [`MiError`].
pub type Result<T> = std::result::Result<T, MiError>;

/// A runtime setup error.
#[derive(Debug, thiserror::Error)]
pub enum MiError {
    /// Code generation failed.
    #[error(transparent)]
    Codegen(#[from] codegen::CodegenError),
    /// The IR did not parse.
    #[error(transparent)]
    Ir(#[from] qir::QirError),
    /// A procedure the caller named does not exist.
    #[error("procedure {0} does not exist")]
    NoProc(String),
    /// The module has no entry procedure.
    #[error("module {0} has no entry procedure")]
    NoEntry(String),
    /// Binding failed.
    #[error(transparent)]
    Bind(#[from] ile::IleError),
    /// A condition was raised outside generated code.
    #[error("{} was raised", .0.msgid())]
    Cond(codegen::Cond),
}
