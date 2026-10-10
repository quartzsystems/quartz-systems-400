// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 object store.
//!
//! Layers, bottom up: checksummed pages ([`page`]), a copy-on-write
//! B+tree ([`btree`]) over a [`Pager`], and the object layer ([`obj`]).
//! The on-disk format is specified in `docs/STORE_FORMAT.md`, and
//! `tests/spec.rs` checks the constants here against it.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod btree;
mod bytes;
mod error;
pub mod obj;
pub mod page;
pub mod spec;

pub use error::{Result, StoreError};
pub use page::pager::{MemPager, PageNo, Pager};
pub use page::{BODY_SIZE, PAGE_SIZE, Page, PageType};
