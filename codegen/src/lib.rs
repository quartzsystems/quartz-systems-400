// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Code generation for QS/400: IR modules to `x86_64` native code through
//! Cranelift. The output of a module is its code, the relocations the
//! loader resolves against the runtime call table and the program's
//! static area, the procedure table and the statement map. `docs/ABI.md`
//! is the calling convention.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod lower;

pub mod abi;
pub mod load;
pub mod output;
pub mod rt;

pub use error::{CodegenError, Result};
pub use load::{Loaded, Placed, Resolve};
pub use lower::{compile, target};
pub use output::{CODEGEN_VERSION, ModuleCode, ProcCode, Reloc, RelocKind, RelocTarget, StmtRange};
pub use rt::{Cond, RT_FNS, RtFn};
