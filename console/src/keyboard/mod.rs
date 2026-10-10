// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Keyboard input: Linux evdev readers and the 5250 key map.

pub mod map;

#[cfg(target_os = "linux")]
mod evdev;

#[cfg(target_os = "linux")]
pub use evdev::spawn_readers;

/// One key press or release, as a Linux key code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// `KEY_*` code from `linux/input-event-codes.h`.
    pub code: u16,
    /// Press (or repeat) rather than release.
    pub pressed: bool,
}
