// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The update service protocol (`docs/UPDATE_PROTOCOL.md`): the signed
//! catalog, orders and their resolution, the HTTP client `SNDPTFORD`
//! and `QSCHKUPD` use, and a reference server over a directory.

#![deny(missing_docs)]

pub mod catalog;
pub mod client;
pub mod deliver;
mod error;
pub mod http;
pub mod order;
pub mod publish;
pub mod server;
pub mod url;

pub use catalog::{Catalog, CatalogGroup, CatalogPtf, GroupKind, PtfKind, Product};
pub use client::{Client, Ident};
pub use error::{Error, Result};
pub use order::{OrderMode, OrderRequest, OrderResult, Why};
pub use server::Server;
pub use url::Url;

/// Message identifiers the commands send for these errors.
pub mod msg {
    /// The service could not be reached or answered badly.
    pub const CONNECT: &str = "CPF8C24";
    /// What the service sent does not verify.
    pub const VERIFY: &str = "CPF8C32";
}
