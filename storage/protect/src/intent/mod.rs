// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The write-intent bitmap of a mirrored set: which regions may hold
//! writes that have not reached every member, so that after an unclean
//! stop only those are copied again.
//!
//! - [`IntentMap`]: the map in memory and its versioned bytes.
//! - [`IntentStore`]: where the caller keeps them (read all, write a
//!   range durably); [`MemIntentStore`] for tests.
//! - [`copy_next`]: the resynchronization copy with a [`Cursor`], for
//!   sets too large to look at every chunk on every step.
//!
//! [`set::MirrorSet`](crate::set::MirrorSet) keeps the map when it is
//! opened with a [`set::Intent`](crate::set::Intent).

mod copy;
mod map;
mod store;

pub use copy::{Cursor, copy_next, done_count, undo};
pub use map::{DEFAULT_REGION_BYTES, HEADER_LAYOUT, HEADER_LEN, IntentMap, MapKind, VERSION};
pub use store::{IntentStore, MemIntentStore};
