// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! ILE objects for QS/400: the payloads of `*MODULE`, `*PGM`, `*SRVPGM`
//! and `*BNDDIR`, the binder that turns modules into programs with
//! IBM's resolution order, the binder language and export signatures,
//! the display providers, and the store operations that create and
//! read the objects. `docs/ILE.md` is the specification.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bin;
mod error;

pub mod binder;
pub mod dsp;
pub mod object;
pub mod spec;
pub mod store;

pub use binder::lang::{BinderSource, ExportBlock, parse_binder_source};
pub use binder::signature::{Signature, signature_of};
pub use binder::sysproc::{SYSTEM_SRVPGM_PROCS, is_system_srvpgm_proc, system_srvpgm};
pub use binder::{BindInput, BindOptions, Diag, ExportSpec, Severity, bind};
pub use error::{IleError, Result};
pub use object::{
    ActGrpAttr, BndDir, BndDirEntry, BndEntryKind, BoundModule, CreateData, Export, ExportKind,
    ImportRes, ImportTarget, ModuleObject, PgmAttr, PgmKind, ProgramObject, SrvPgmRef, UserProfile,
};
