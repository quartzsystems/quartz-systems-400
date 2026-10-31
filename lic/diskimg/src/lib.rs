// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 disk layout, shared by `build disk` (an image file) and
//! the installer in `slic` (a real disk): a protective MBR, a GPT with
//! the ESP, the LIC, IFS and ASP1 partitions, and the FAT
//! volumes that go on the ESP and the installation media.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod efi;
mod error;
pub mod fat;
pub mod iso9660;
pub mod migrate;
pub mod plan;
mod table;
mod window;

#[cfg(target_os = "linux")]
pub mod locate;

pub use error::Error;
pub use plan::{
    ASP1_NAME, ASP1_TYPE_GUID, DEFAULT_DISK_BYTES, ESP_NAME, Guids, IFS_INDEX, IFS_NAME,
    IFS_TYPE_GUID, LIC_NAME, LIC_TYPE_GUID, MAX_IFS_BYTES, MIN_DISK_BYTES, MIN_IFS_BYTES, Part,
    Plan, SECTOR, ifs_bytes,
};
pub use table::{IfsFormat, read_table, write_image, write_image_with};
pub use window::Window;
pub use xfsfmt;
