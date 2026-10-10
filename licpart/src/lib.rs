// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The LIC partition: a small raw GPT partition that persists IPL state
//! before any object store exists.
//!
//! Two copies (A and B) of one record live at fixed offsets. A write goes
//! to the older copy, so a torn write can only damage the copy that was
//! already stale; a read takes the newest copy whose CRC verifies.
//!
//! Everything stored here is `MIGRATE(phase2)`: it moves into the object
//! store once that exists.

#![forbid(unsafe_code)]

mod error;
mod format;
mod locate;
mod part;

pub use error::LicError;
pub use format::header::{Header, IplMode, IplType, Slot};
pub use format::history::{IplHistory, Outcome};
pub use format::tlv::{TAG_USER_MIN, Tlv};
pub use format::{FORMAT_VERSION, HISTORY_MAX, Record, SLOT_BYTES};
pub use locate::{LIC_TYPE_GUID, PARTITION_NAME};
pub use part::{Device, LicPart};

#[cfg(target_os = "linux")]
pub use locate::locate;
