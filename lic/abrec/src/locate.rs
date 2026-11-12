// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Finding the partition by GPT type GUID through sysfs, via `diskimg`.

/// `/dev/<partition>` of the first partition with the record's type.
#[cfg(target_os = "linux")]
pub fn locate(spec: &crate::Spec) -> Result<std::path::PathBuf, crate::AbError> {
    diskimg::locate::first_of_type(spec.type_guid).map_err(|_| crate::AbError::NotFound)
}
