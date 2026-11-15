// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The partition service channel: what a system running in a partition
//! and its hypervisor say to each other (`docs/QSID.md`). The
//! hypervisor gives the partition a virtio-console device with two
//! named ports, present from the kernel's first instruction:
//!
//! - [`SERVICE_PORT`]: the system's reference codes as they are posted,
//!   its state, a controlled power down, the control panel functions,
//!   pings and the hypervisor's clock ([`service`]);
//! - [`CONSOLE_PORT`]: the console as a grid of cells with its cursor
//!   and indicators, and the keys back ([`console`]).
//!
//! Both ports carry [`frame`]s: a checked eight-byte header, then a
//! body that is a [`ipc::Wire`] value. Either side may be the older:
//! each says its [`VERSION`] and capabilities first, a receiver skips
//! a message kind it does not know, and a body may carry fields after
//! the ones a receiver knows, which it passes over. Each side treats
//! the other's bytes as hostile: frames are bounded by [`frame::MAX_BODY`]
//! and every decoder is checked ([`fuzzing`] is the fuzz target).

#![forbid(unsafe_code)]

pub mod console;
mod error;
pub mod frame;
pub mod fuzzing;
pub mod service;
mod wire;

pub use error::Error;
pub use frame::{Frame, Reader};

/// The virtio-console port of the service channel.
pub const SERVICE_PORT: &str = "systems.quartz.qsvs.service.0";

/// The virtio-console port of the console.
pub const CONSOLE_PORT: &str = "systems.quartz.qsvs.console.0";

/// The protocol's version: the highest this crate speaks. The two sides
/// use the lower of the two they say.
pub const VERSION: u16 = 1;

/// Capability bits, as each side's hello says them. A side does only
/// what both say they have.
pub mod caps {
    /// Reference codes pushed as they are posted.
    pub const REFCODES: u64 = 1 << 0;
    /// The system's state pushed on change.
    pub const STATE: u64 = 1 << 1;
    /// A power down asked by the hypervisor.
    pub const POWER_DOWN: u64 = 1 << 2;
    /// Control panel functions.
    pub const PANEL: u64 = 1 << 3;
    /// The hypervisor's clock.
    pub const TIME: u64 = 1 << 4;
    /// The console port: whole frames and changes.
    pub const CONSOLE: u64 = 1 << 5;
    /// The console port: keys back.
    pub const KEYS: u64 = 1 << 6;

    /// Everything version 1 of the service port has.
    pub const SERVICE_V1: u64 = REFCODES | STATE | POWER_DOWN | PANEL | TIME;
    /// Everything version 1 of the console port has.
    pub const CONSOLE_V1: u64 = CONSOLE | KEYS;
}

/// The version two sides speak: the lower of theirs.
#[must_use]
pub fn agreed(mine: u16, theirs: u16) -> u16 {
    mine.min(theirs)
}
