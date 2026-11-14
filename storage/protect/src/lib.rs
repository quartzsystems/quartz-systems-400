// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Disk protection over [`blockdev::BlockDev`] units, with no object
//! store under or over it.
//!
//! - [`UnitState`]: what a member of a mirrored set is doing.
//! - [`mirror`]: I/O over the halves of a mirrored set (read from one,
//!   write to all, a failing half taken out of service, a damaged block
//!   rewritten from the other half) and the resynchronization copy with
//!   its progress map.
//! - [`parity`]: RAID 5 and RAID 6 sets with their partial-parity log,
//!   the repair at open, synchronization and rebuild.
//! - [`intent`]: the write-intent bitmap of a mirrored set, so that an
//!   unclean stop copies only the regions being written.
//! - [`spare`]: which hot spare takes a failed member's place.
//! - [`set`]: a protected set as one block device, for a caller with no
//!   pool of its own: [`set::MirrorSet`] and [`set::ParitySet`].
//!
//! The crate writes no label of its own. What identifies a member and
//! what records its state between runs is the caller's: the object
//! store keeps both in its unit headers and superblock, and passes the
//! on-disk constants of its format in ([`parity::Format`], the resume
//! map's magic). `docs/BUILDKIT.md` says what another caller supplies.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod bytes;
pub mod intent;
pub mod mirror;
pub mod parity;
pub mod set;
pub mod spare;
pub mod spec;
mod state;

pub use state::UnitState;

/// Bytes per block, and per page of a set: the block device's.
pub const PAGE_SIZE: usize = blockdev::BLOCK_SIZE as usize;
