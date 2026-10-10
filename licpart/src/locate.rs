// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Finding the partition by GPT type GUID through sysfs, via `diskimg`.

/// GPT partition name.
pub use diskimg::LIC_NAME as PARTITION_NAME;
/// GPT partition type GUID of the QS/400 LIC partition.
pub use diskimg::LIC_TYPE_GUID;

/// `/dev/<partition>` of the first LIC partition found.
#[cfg(target_os = "linux")]
pub fn locate() -> Result<std::path::PathBuf, crate::error::LicError> {
    diskimg::locate::first_of_type(LIC_TYPE_GUID).map_err(|_| crate::error::LicError::NotFound)
}
