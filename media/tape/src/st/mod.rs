// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Physical tape: drives through the Linux `st` driver and media
//! changers through `sg`. The transports are written against the ioctl
//! traits in [`ioctl`]; `raw` (Linux only) makes the real calls.

pub mod ioctl;
#[cfg(target_os = "linux")]
mod raw;
mod sg;
mod tape;

#[cfg(target_os = "linux")]
pub use raw::{LinuxSg, LinuxSt};
pub use sg::{
    MOVE_MEDIUM, READ_ELEMENT_STATUS, STATUS_ALLOCATION, SgChanger, move_medium_cdb,
    parse_element_status, read_element_status_cdb, sense_codes,
};
pub use tape::{DENSITIES, StTransport, density_name};
