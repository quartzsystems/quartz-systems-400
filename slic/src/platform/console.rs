// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing to `/dev/console` and the serial port. Errors are ignored: if
//! a device is missing there is nowhere better to report it.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;

/// The system console.
pub const CONSOLE: &str = "/dev/console";
/// The serial port the kernel command line names.
pub const SERIAL: &str = "/dev/ttyS0";

/// `O_NOCTTY`: do not become the controlling terminal.
const O_NOCTTY: i32 = 0o400;

/// Open handles to both outputs; either may be absent.
pub struct Outputs {
    console: Option<File>,
    serial: Option<File>,
}

impl Outputs {
    /// Open whatever exists.
    pub fn open() -> Self {
        Self {
            console: open(CONSOLE),
            serial: open(SERIAL),
        }
    }

    /// Write to console and serial.
    pub fn both(&mut self, text: &str) {
        write(self.console.as_mut(), text);
        write(self.serial.as_mut(), text);
    }

    /// Write to serial only.
    pub fn serial(&mut self, text: &str) {
        write(self.serial.as_mut(), text);
    }
}

/// Fresh handles for the panic path, which owns nothing.
pub fn emergency(text: &str) {
    write(open(SERIAL).as_mut(), text);
    write(open(CONSOLE).as_mut(), text);
}

fn open(path: &str) -> Option<File> {
    OpenOptions::new()
        .write(true)
        .custom_flags(O_NOCTTY)
        .open(path)
        .ok()
}

fn write(file: Option<&mut File>, text: &str) {
    if let Some(f) = file {
        let _ = f.write_all(text.as_bytes());
    }
}
