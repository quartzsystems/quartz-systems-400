// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A two-copy A/B record on a small raw partition, for state that must
//! survive before any file system or store is open.
//!
//! Two copies (A and B) of one record live at fixed offsets. A write goes
//! to the older copy, so a torn write can only damage the copy that was
//! already stale; a read takes the newest copy whose CRC verifies.
//!
//! The product is the caller's: its [`Spec`] gives the record's magic and
//! the GPT type of the partition, and the payload is opaque bytes.

#![forbid(unsafe_code)]

mod error;
mod format;
mod locate;
mod part;
mod spec;

pub use error::AbError;
pub use format::slot::{Slot, newest, target};
pub use format::{CRC32C, Frame, HEAD_LEN, decode, encode};
pub use part::{AbPart, Device, Stored};
pub use spec::{SLOT_BYTES, Spec};

#[cfg(target_os = "linux")]
pub use locate::locate;
