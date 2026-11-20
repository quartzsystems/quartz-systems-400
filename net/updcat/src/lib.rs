// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The update service's catalog (`docs/UPDATE_PROTOCOL.md` §3): the
//! signed text, verified against keys the caller trusts and parsed only
//! once verified; products, PTFs and groups; and, for a release, its
//! newest cumulative package with the date its name carries. The
//! publisher's side writes the canonical text and signs it.

#![deny(missing_docs)]

pub mod catalog;
pub mod cumulative;
pub mod signed;

pub use catalog::{Catalog, CatalogGroup, CatalogPtf, FORMAT, GroupKind, Latest, Product, PtfKind};
pub use cumulative::{Cumulative, Date};
pub use ed25519_dalek::SigningKey;

/// Why a catalog was refused.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No trusted key signed it.
    #[error("{0}")]
    Verify(String),
    /// The signed text is not a catalog this code reads.
    #[error("{0}")]
    Parse(String),
}

/// A result.
pub type Result<T> = std::result::Result<T, Error>;
