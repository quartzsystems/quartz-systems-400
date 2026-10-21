// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages between QS/400 processes: `slic`, the storage service and
//! the job processes.
//!
//! [`Wire`] is a compact little-endian codec: integers fixed width,
//! lengths as `u32`, enums as a one-byte tag. [`wire_struct!`] and
//! [`wire_enum!`] implement it for plain types. [`frame`] puts one
//! encoded message in a length-prefixed frame on any byte stream, and
//! [`chan`] is a blocking channel of frames over a Unix socket.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(unix)]
pub mod chan;
mod error;
pub mod frame;
pub mod rpc;
mod wire;

pub use error::WireError;
pub use wire::{Dec, Wire, decode, encode};
