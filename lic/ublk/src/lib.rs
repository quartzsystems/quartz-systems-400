// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A ublk server: a block device the kernel reads and writes through
//! this process. The storage service serves the IFS volume with it
//! (`docs/IFS.md` "The block device"). Only Linux has the driver; other
//! hosts build the [`Target`] side and a [`Device`] that cannot start.

pub mod abi;
pub mod target;

#[cfg(target_os = "linux")]
mod device;
#[cfg(target_os = "linux")]
mod sys;

#[cfg(target_os = "linux")]
pub use device::{DEPTH, Device, MAX_IO};
pub use target::{BLOCK, Target};

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
