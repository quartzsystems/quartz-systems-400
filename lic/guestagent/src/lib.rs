// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 guest agent (`docs/QUALIFIED_HARDWARE.md`, "Guest agent").
//! It speaks the QEMU guest agent's wire protocol, JSON over the
//! `org.qemu.guest_agent.0` virtio-serial port (and over vsock where
//! the hypervisor offers it), so Proxmox, libvirt and QEMU itself see a
//! standard agent: the host name, the release, the IPL state and the
//! last SRC, the IP addresses, disk use; and graceful power down and
//! restart, which are `PWRDWNSYS *CNTRLD` with or without
//! `RESTART(*YES)`.
//!
//! - [`framer`]: requests out of a byte stream, the `0xFF` resync.
//! - [`agent`]: the commands and their answers, over a [`Guest`].
//! - [`facts`]: what the agent reports, as plain data.
//! - [`status`]: the state file `slic` keeps for it.
//! - [`port`]: finding the agent's virtio-serial port in sysfs.
//! - [`ups`]: the power supply's state the host reports.
//! - `linux`: the guest itself and the serve loop (`/init qsga`).

#![forbid(unsafe_code)]

pub mod agent;
pub mod facts;
pub mod framer;
pub mod port;
pub mod status;
pub mod ups;

#[cfg(target_os = "linux")]
pub mod linux;

pub use agent::Agent;
pub use facts::Guest;

/// The virtio-serial port name hypervisors give the agent's channel.
pub const PORT_NAME: &str = "org.qemu.guest_agent.0";

/// The vsock port the agent listens on where vsock is present.
pub const VSOCK_PORT: u32 = 9_400;

/// The version `guest-info` reports: the protocol's level this agent
/// implements, then QS/400's release.
#[must_use]
pub fn version() -> String {
    format!("9.0.0-qs400-{}", release::RELEASE)
}
