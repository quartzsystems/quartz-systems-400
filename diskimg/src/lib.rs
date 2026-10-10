// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 disk layout, shared by `build disk` (an image file) and
//! the installer in `slic` (a real disk): a protective MBR, a GPT with
//! the ESP, the LIC partition and the ASP1 partition, and the FAT
//! volumes that go on the ESP and the installation media.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
pub mod fat;
pub mod plan;
mod table;

#[cfg(target_os = "linux")]
pub mod locate;

pub use error::Error;
pub use plan::{
    ASP1_NAME, ASP1_TYPE_GUID, DEFAULT_DISK_BYTES, ESP_NAME, Guids, LIC_NAME, LIC_TYPE_GUID,
    MIN_DISK_BYTES, Part, Plan, SECTOR,
};
pub use table::{read_table, write_image};
