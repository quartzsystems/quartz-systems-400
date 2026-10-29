// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The integrated file system's view of the object store, for the
//! storage service: `/QSYS.LIB` ([`view::StoreQsys`]), the profiles,
//! UID/GID numbers and authorization lists the IFS authority search
//! asks for ([`env::StoreEnv`]), and the sink that turns IFS audit
//! events into `QAUDJRN` entries ([`audit::JournalAudit`]). The store
//! is reached through [`access::StoreAccess`], so the same code runs
//! in the service (over the lent store) and in tests (over a store in
//! memory).

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod access;
pub mod audit;
pub mod env;
pub mod savf;
pub mod view;

pub use access::{MemAccess, StoreAccess};
pub use audit::JournalAudit;
pub use env::StoreEnv;
pub use view::StoreQsys;
