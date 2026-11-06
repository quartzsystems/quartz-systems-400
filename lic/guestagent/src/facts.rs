// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the agent reports of the system, as plain data, and the
//! [`Guest`] it asks: the real one on Linux (`crate::linux`), one in
//! memory in tests.

use crate::status::Status;

/// An IP address of an interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    /// `ipv4` or `ipv6`.
    pub family: &'static str,
    /// The address as text.
    pub address: String,
    /// The prefix length.
    pub prefix: u8,
}

/// A network interface.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Interface {
    /// The kernel's name (`eth0`).
    pub name: String,
    /// The MAC address, `52:54:00:12:34:56`; empty when none.
    pub mac: String,
    /// Its addresses.
    pub addresses: Vec<Address>,
    /// Bytes, packets, errors and drops received, then sent.
    pub stats: [u64; 8],
}

/// A file system or storage pool and its use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filesystem {
    /// Its device or pool name (`ASP1`, `ublkb0`).
    pub name: String,
    /// Where it is mounted (`/`, `/QSYS.LIB`).
    pub mountpoint: String,
    /// Its type (`xfs`, `qs400-asp`).
    pub kind: String,
    /// Bytes in use.
    pub used: u64,
    /// Bytes in all.
    pub total: u64,
}

/// The kernel underneath.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kernel {
    /// `uname -r`.
    pub release: String,
    /// `uname -v`.
    pub version: String,
    /// `uname -m`.
    pub machine: String,
}

/// How the hypervisor asked the system to stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// `powerdown` or `halt`: `PWRDWNSYS OPTION(*CNTRLD) RESTART(*NO)`.
    PowerDown,
    /// `reboot`: `PWRDWNSYS OPTION(*CNTRLD) RESTART(*YES)`.
    Restart,
}

/// What the agent asks of the system.
pub trait Guest {
    /// The state `slic` last wrote.
    fn status(&mut self) -> Status;
    /// The kernel.
    fn kernel(&mut self) -> Kernel;
    /// The network interfaces, loopback included.
    fn interfaces(&mut self) -> Vec<Interface>;
    /// The storage pools and file systems.
    fn filesystems(&mut self) -> Vec<Filesystem>;
    /// Nanoseconds since 1970-01-01 UTC.
    fn time_ns(&mut self) -> i64;
    /// Processors online.
    fn cpus(&mut self) -> usize;
    /// Start a controlled power down; `Err` with why it could not.
    ///
    /// # Errors
    /// The request could not be made.
    fn stop(&mut self, how: Stop) -> Result<(), String>;
    /// Record what the uninterruptible power supply says.
    ///
    /// # Errors
    /// The state could not be kept.
    fn set_power(&mut self, power: crate::ups::Power) -> Result<(), String> {
        let _ = power;
        Err("The guest keeps no power supply state".into())
    }
}
