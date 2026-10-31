// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Licensed programs (`docs/LICPGM.md`): the Quartz product scheme and
//! its shipped catalog, the `*PRDDFN` and `*PRDLOD` payloads, the
//! product directory and CHKPRDOPT, the QSZ* product APIs, RSTLICPGM,
//! DLTLICPGM and SAVLICPGM planning, QSPKGPRD manifests, license keys
//! and use (ADDLICKEY, WRKLICINF, QLZAREQ, QLZARLS) and software
//! agreements.
//!
//! Storage is the integrator's: [`ProductStore`] holds the objects and
//! records, [`ObjectCheck`] answers for other objects, and install
//! goes through [`plan::Media`] and [`plan::Restorer`]. [`MemStore`]
//! is an in-memory store for tests.

#![deny(missing_docs)]
#![allow(
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::too_many_arguments,
    clippy::many_single_char_names,
    clippy::match_same_arms,
    clippy::single_match_else,
    clippy::items_after_statements,
    clippy::enum_glob_use,
    // Plan errors carry the load they are about; they are cold.
    clippy::result_large_err
)]

pub mod agree;
pub mod api;
pub mod catalog;
pub mod directory;
pub mod id;
pub mod license;
pub mod model;
pub mod payload;
pub mod pkg;
pub mod plan;
pub mod store;
pub mod system;

pub use id::{LicTerm, LoadId, LoadType, OptionNo, ProductId};
pub use model::definition::ProductDefinition;
pub use model::load::{LoadKey, ProductLoad};
pub use model::state::LoadState;
pub use model::{Date, QualName};
pub use store::{MemStore, ObjStatus, ObjectCheck, ProductStore, StoreError};
pub use system::SystemInfo;
