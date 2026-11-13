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
//! What lives here is what the IPL needs before the object store is
//! open: slots, keylock, history, the IPL subset of the system values,
//! the console mode, the side of the next IPL where no firmware keeps it,
//! and the key slots that seal secrets kept elsewhere. Everything else
//! is in the store.

#![forbid(unsafe_code)]

pub mod console;
mod error;
mod format;
pub mod ipla;
pub mod iplsrc;
mod locate;
mod part;

pub use console::{ConsoleMode, TAG_CONSOLE_MODE};
pub use error::LicError;
pub use format::header::{Header, IplMode, IplType, Slot};
pub use format::history::{IplHistory, Outcome};
pub use format::keyslot::{KEY_BYTES, KEYSLOT_VERSION, KeyPurpose, KeySlot};
pub use format::pal::{PAL_MAX, PalEntry, Severity};
pub use format::tlv::{TAG_USER_MIN, Tlv};
pub use format::{FORMAT_VERSION, HISTORY_MAX, Record, SLOT_BYTES, SPEC};
pub use iplsrc::{IPL_SOURCE_BYTES, IPL_SOURCE_VERSION, IplSource, TAG_IPL_SOURCE};
pub use locate::{LIC_TYPE_GUID, PARTITION_NAME};
pub use part::{Device, LicPart, RESUME_MAX, RESUME_OFFSET, SERVICE_TAGS};

#[cfg(target_os = "linux")]
pub use locate::locate;
