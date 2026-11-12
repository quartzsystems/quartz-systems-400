// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A ublk server: a block device the kernel reads and writes through
//! this process, over whatever [`Target`] the caller gives it; any
//! `blockdev::BlockDev` is one through [`BlockTarget`]. A device added
//! with [`Options::recovery`] outlives its server: the kernel holds its
//! requests until a new process takes it over ([`Device::recover`]),
//! so a file system mounted on it stays mounted. Only Linux has the
//! driver; other hosts build the [`Target`] side and a [`Device`] that
//! cannot start.

pub mod abi;
pub mod target;

#[cfg(target_os = "linux")]
mod device;
#[cfg(target_os = "linux")]
mod sys;

#[cfg(target_os = "linux")]
pub use device::{DEPTH, Device, MAX_IO};
pub use target::{BLOCK, BlockTarget, Target};

/// How a device is added.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// Keep the device when its server ends without stopping it. The
    /// kernel holds the requests that come meanwhile, and issues again
    /// the ones the old server had taken and not answered, once a new
    /// process has taken the device over with `Device::recover`. The
    /// target must take a write twice, as a block device does.
    pub recovery: bool,
}

/// Why the device could not be served.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The kernel refused or failed a step.
    #[error("{0}")]
    Kernel(String),
    /// Not on this host.
    #[error("ublk needs Linux")]
    Unsupported,
}

/// Grow the xfs a file of `file` is on to `blocks` 4096-byte blocks.
#[cfg(target_os = "linux")]
pub fn xfs_growfs(file: &std::fs::File, blocks: u64) -> Result<(), Error> {
    // The inode space limit xfsfmt formats with.
    sys::xfs_growfs(file, blocks, 25).map_err(|e| Error::Kernel(format!("grow xfs: {e}")))
}

/// A device that cannot start: ublk needs Linux.
#[cfg(not(target_os = "linux"))]
pub struct Device;

#[cfg(not(target_os = "linux"))]
impl Device {
    /// Always [`Error::Unsupported`].
    pub fn start(_target: std::sync::Arc<dyn Target>) -> Result<Self, Error> {
        Err(Error::Unsupported)
    }

    /// Always [`Error::Unsupported`].
    pub fn start_with(
        _target: std::sync::Arc<dyn Target>,
        _options: Options,
    ) -> Result<Self, Error> {
        Err(Error::Unsupported)
    }

    /// Always [`Error::Unsupported`].
    pub fn recover(_id: u32, _target: std::sync::Arc<dyn Target>) -> Result<Self, Error> {
        Err(Error::Unsupported)
    }

    /// The device number.
    #[must_use]
    pub fn id(&self) -> u32 {
        0
    }

    /// The block device.
    #[must_use]
    pub fn block_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::new()
    }

    /// Never reached.
    pub fn resize(&self, _blocks: u64) -> Result<(), Error> {
        Err(Error::Unsupported)
    }

    /// Never reached.
    pub fn stop(self) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
}

/// Never reached off Linux.
#[cfg(not(target_os = "linux"))]
pub fn xfs_growfs(_file: &std::fs::File, _blocks: u64) -> Result<(), Error> {
    Err(Error::Unsupported)
}
