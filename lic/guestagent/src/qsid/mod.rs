// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `qsid`, the partition service channel's daemon (`docs/QSID.md`):
//! under QS/VS, `slic` starts it (`/init qsid`) before the first
//! reference code and it serves `qsipart`'s service port. Its facts are
//! the guest agent's: the state `slic` keeps ([`crate::status`]), which
//! `slic` also pushes to it with each code it posts, and the network's
//! addresses.
//!
//! - [`serve`]: the protocol over a [`serve::Host`], without I/O.
//! - `linux`: the port, the channel to `slic`, the host.
//!
//! Between `slic` and the daemon, on the daemon's standard input (an
//! `ipc` channel): [`Notice`] one way, [`Ask`] the other.

use ipc::wire_enum;

pub mod serve;

#[cfg(target_os = "linux")]
pub mod linux;

/// What `slic` tells the daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// A reference code was posted: the state file's text after it.
    Posted {
        /// [`crate::status::Status::render`].
        status: String,
    },
    /// The state changed without a code (the system halted or ended).
    Changed {
        /// As above.
        status: String,
    },
    /// The answer to [`Ask::Dst`]: a refusal's message and text, or
    /// none when accepted.
    Answer {
        /// The request's id.
        id: u32,
        /// Why not.
        refusal: Option<(String, String)>,
    },
    /// Say [`Ask::Flushed`] once everything before this is written to
    /// the port (or there is no peer to write it to).
    Flush {
        /// Repeated in the answer.
        token: u32,
    },
}
wire_enum!(Notice {
    0 => Posted { status },
    1 => Changed { status },
    2 => Answer { id, refusal },
    3 => Flush { token },
});

/// What the daemon asks of `slic`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Control panel function 21: service tools on the console.
    Dst {
        /// The request's id, for [`Notice::Answer`].
        id: u32,
    },
    /// Everything before the [`Notice::Flush`] with this token is out.
    Flushed {
        /// Its token.
        token: u32,
    },
}
wire_enum!(Ask {
    0 => Dst { id },
    1 => Flushed { token },
});

/// The texts of the refusals, in the system's words (`qsipart`'s
/// message identifiers say which).
pub mod text {
    /// `QSI0001`.
    #[must_use]
    pub fn unknown(kind: u16) -> String {
        format!("The system does not know message kind {kind:#06x}.")
    }
    /// `QSI0002`.
    #[must_use]
    pub fn bad(why: &str) -> String {
        format!("The request could not be read: {why}.")
    }
    /// `QSI0010`.
    pub const NOT_READY: &str = "The system cannot power down until its IPL is complete.";
    /// `QSI0011`.
    pub const ENDING: &str = "A power down is already under way.";
    /// `QSI0012`.
    #[must_use]
    pub fn power_down_failed(why: &str) -> String {
        format!("The power down could not be started: {why}.")
    }
    /// `QSI0020`.
    #[must_use]
    pub fn function(n: u8) -> String {
        format!("Function {n} is not one this system carries out for the hypervisor; 21 is.")
    }
    /// `QSI0021`.
    pub const NOT_MANUAL: &str = "Function 21 needs the keylock in manual mode.";
    /// `QSI0022`, before the IPL is complete.
    pub const DST_IPL: &str =
        "The IPL is not complete; at an attended IPL its menu offers the service tools.";
    /// `QSI0022`, with the service tools on the console already.
    pub const DST_ACTIVE: &str = "The service tools are on the console already.";
    /// `QSI0022`, when `slic` did not answer in time.
    pub const DST_NO_ANSWER: &str = "The system did not answer the request for service tools.";
}
