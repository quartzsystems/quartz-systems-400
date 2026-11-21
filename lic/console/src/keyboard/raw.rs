// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ioctl `EVIOCGABS`: an absolute axis's range. With `fb::raw`, the
//! only `unsafe` in the console crate.

#![allow(unsafe_code)]

use std::fs::File;
use std::io;

use rustix::ioctl::{Getter, ioctl};

/// `struct input_absinfo`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AbsInfo {
    /// The current value.
    pub value: i32,
    /// Smallest value.
    pub minimum: i32,
    /// Largest value.
    pub maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

/// `EVIOCGABS(ABS_X)`: `_IOR('E', 0x40 + 0, struct input_absinfo)`.
const EVIOCGABS_X: u32 = 0x8018_4540;
/// `EVIOCGABS(ABS_Y)`.
const EVIOCGABS_Y: u32 = 0x8018_4541;

/// The `ABS_X` (`y` false) or `ABS_Y` axis of an event device.
pub fn abs_info(file: &File, y: bool) -> io::Result<AbsInfo> {
    let info = if y {
        // SAFETY: EVIOCGABS fills exactly one input_absinfo, six i32s,
        // which the struct above matches field for field.
        unsafe { ioctl(file, Getter::<EVIOCGABS_Y, AbsInfo>::new()) }?
    } else {
        // SAFETY: as above.
        unsafe { ioctl(file, Getter::<EVIOCGABS_X, AbsInfo>::new()) }?
    };
    Ok(info)
}
