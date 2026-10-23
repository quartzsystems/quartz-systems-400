// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The display channel between a job and the TN5250E session that
//! holds its device: a Unix socket at `/run/qs400/dev/<DEVICE>` the
//! session listens on and the job connects to. Records cross it as
//! 5250 data, framed by `ipc`; the session only moves them to and from
//! the telnet connection.

use ipc::wire_enum;

/// Where a device's socket lives.
pub const DIR: &str = "/run/qs400/dev";

/// The socket path of a device.
#[must_use]
pub fn socket(device: &str) -> String {
    format!("{DIR}/{device}")
}

/// From the job to the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToDevice {
    /// A record for the display: its operation code and data stream.
    Write {
        /// The record's operation code.
        opcode: u8,
        /// The 5250 data.
        data: Vec<u8>,
    },
    /// The job is done with the display.
    Release,
}

wire_enum!(ToDevice {
    0 => Write { opcode, data },
    1 => Release,
});

/// From the session to the job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromDevice {
    /// A record from the display: flags, operation code and data.
    Record {
        /// The header flags (System Request, Attention, ...).
        flags: u16,
        /// The operation code.
        opcode: u8,
        /// The data: cursor, AID and fields, or a saved screen.
        data: Vec<u8>,
    },
    /// The display came back after a disconnection, or is a new one:
    /// the job's screen must be sent whole again.
    Reset {
        /// The new device's name, when it changed.
        device: String,
    },
    /// The connection is gone; the session decides what becomes of
    /// the job (`QDEVRCYACN`).
    Gone,
}

wire_enum!(FromDevice {
    0 => Record { flags, opcode, data },
    1 => Reset { device },
    2 => Gone,
});

/// The key numbers a job's display answers with: `0` for Enter, `1`
/// to `24` for the function keys, then the other attention keys.
pub mod key {
    /// Enter.
    pub const ENTER: u8 = 0;
    /// Page Up (Roll Down).
    pub const PAGE_UP: u8 = 25;
    /// Page Down (Roll Up).
    pub const PAGE_DOWN: u8 = 26;
    /// Help.
    pub const HELP: u8 = 27;
    /// Print.
    pub const PRINT: u8 = 28;
    /// Clear.
    pub const CLEAR: u8 = 29;
    /// Record Backspace.
    pub const RECORD_BACKSPACE: u8 = 30;
    /// System Request.
    pub const SYSREQ: u8 = 31;
    /// Attention.
    pub const ATTN: u8 = 32;
    /// Test Request.
    pub const TESTREQ: u8 = 33;
    /// The display went away: the job's turn to decide.
    pub const GONE: u8 = 255;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let m = FromDevice::Record {
            flags: 0x0400,
            opcode: 0,
            data: vec![1, 2, 3],
        };
        let b = ipc::encode(&m);
        let back: FromDevice = ipc::decode(&b).unwrap();
        assert_eq!(back, m);
        assert_eq!(socket("QPADEV0001"), "/run/qs400/dev/QPADEV0001");
    }
}
