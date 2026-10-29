// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a writer job is told to do next.

use ipc::wire_enum;

use crate::obj::splf::SplfAttrs;

/// The writer's next step.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // one at a time, over the wire
pub enum WtrStep {
    /// End: `ENDWTR`, or `AUTOEND(*YES)` with the queue empty.
    End,
    /// Nothing to print now (held, or the queue is empty or held).
    Wait,
    /// Print this file.
    File {
        /// Its attributes.
        attrs: SplfAttrs,
        /// Its SCS.
        data: Vec<u8>,
    },
}

wire_enum!(WtrStep {
    0 => End,
    1 => Wait,
    2 => File { attrs, data },
});

/// Where a writer prints, as the service tells the writer job when it
/// starts (`docs/PRINT.md`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WtrDest {
    /// The capture device, or a TN5250E printer session's device.
    Capture,
    /// A LAN printer device (`CRTDEVPRT DEVCLS(*LAN) LANATTACH(*IP)`).
    Lan {
        /// `RMTLOCNAME`.
        host: String,
        /// `PORT`; 0 for the driver's.
        port: u16,
        /// `SYSDRVPGM`.
        driver: String,
        /// `SECURECNN(*YES)`.
        secure: bool,
        /// `TRANSFORM(*YES)`.
        transform: bool,
        /// `MFRTYPMDL`.
        mfrtypmdl: String,
        /// The `*WSCST` payload for `MFRTYPMDL(*WSCST)`; empty for none.
        wscst: Vec<u8>,
        /// `CHRID`: `gcgid cpgid` or `*SYSVAL`.
        chrid: String,
    },
    /// A remote output queue (`STRRMTWTR`).
    Remote {
        /// The host: `RMTSYS`, or `INTNETADR`.
        host: String,
        /// `RMTPRTQ`; blank for `*USER`.
        queue: String,
        /// `DESTTYPE` code (`wm::obj::outqrmt::DESTTYPE`).
        desttype: u8,
        /// `DESTOPT`.
        destopt: String,
        /// `SEPPAGE(*YES)`.
        seppage: bool,
        /// `TRANSFORM(*YES)`.
        transform: bool,
        /// `MFRTYPMDL`.
        mfrtypmdl: String,
        /// The `*WSCST` payload for `MFRTYPMDL(*WSCST)`; empty for none.
        wscst: Vec<u8>,
        /// This system's name, for the LPR host lines.
        local_host: String,
    },
}

wire_enum!(WtrDest {
    0 => Capture,
    1 => Lan { host, port, driver, secure, transform, mfrtypmdl, wscst, chrid },
    2 => Remote { host, queue, desttype, destopt, seppage, transform, mfrtypmdl, wscst, local_host },
});
