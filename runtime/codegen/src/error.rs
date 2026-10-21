// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What can go wrong while generating or loading code.

/// Result with a [`CodegenError`].
pub type Result<T> = std::result::Result<T, CodegenError>;

/// A code generation error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodegenError {
    /// The module does not verify; codegen only takes verified modules.
    #[error("module does not verify: {0}")]
    Verify(String),

    /// An instruction this release cannot lower.
    #[error("proc {proc}: {what} is not supported yet")]
    Unsupported {
        /// Procedure.
        proc: String,
        /// The instruction, as text.
        what: String,
    },

    /// Cranelift rejected the function.
    #[error("proc {proc}: {msg}")]
    Backend {
        /// Procedure.
        proc: String,
        /// The backend's message.
        msg: String,
    },

    /// The target is not available.
    #[error("target: {0}")]
    Target(String),

    /// A relocation could not be resolved while loading.
    #[error("load: {0}")]
    Load(String),
}
