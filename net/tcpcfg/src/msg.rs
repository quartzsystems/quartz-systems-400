// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The message identifiers TCP/IP configuration sends. Their
//! descriptions are in `QSYS/QTCPMSG` (`runtime/msgf/msgs/qtcpmsg.toml`);
//! `tests/messages.rs` checks that every identifier here has one.
//! Identifiers the oracle's `QTCP/QTCPMSGF` shows keep its numbers and
//! substitution layouts; the rest are ours in the same families.

/// TCP/IP started.
pub const STARTED: &str = "TCP1A00";
/// TCP/IP already active.
pub const ALREADY_ACTIVE: &str = "TCP1A01";
/// TCP/IP ended.
pub const ENDED: &str = "TCP1A02";
/// TCP/IP not active.
pub const NOT_ACTIVE: &str = "TCP1A03";
/// A server could not be started in this release.
pub const SERVER_NOT_STARTED: &str = "TCP1A04";
/// Interface started.
pub const IFC_STARTED: &str = "TCP1A0B";
/// Interface ended.
pub const IFC_ENDED: &str = "TCP1A0C";
/// Interface already active.
pub const IFC_ACTIVE: &str = "TCP1A0D";
/// Interface not active.
pub const IFC_INACTIVE: &str = "TCP1A0E";
/// Interface failed to start.
pub const IFC_FAILED: &str = "TCP1A0F";
/// A route could not be added to the kernel.
pub const RTE_FAILED: &str = "TCP1A11";
/// An attribute could not be applied to the kernel.
pub const ATTR_FAILED: &str = "TCP1A13";
/// TCP/IP ending.
pub const ENDING: &str = "TCP1A14";
/// An entry was added (`&1` names the table).
pub const ADDED: &str = "TCP1D03";
/// An entry was changed.
pub const CHANGED: &str = "TCP1D04";
/// An entry was removed.
pub const REMOVED: &str = "TCP1D05";
/// An entry already exists.
pub const EXISTS: &str = "TCP1D06";
/// An entry was not found.
pub const NOT_FOUND: &str = "TCP1D07";
/// A value is not valid for its parameter.
pub const BAD_VALUE: &str = "TCP1D08";
/// Attributes changed.
pub const ATTRS_CHANGED: &str = "TCP1D09";
/// Host table merged.
pub const MERGED: &str = "TCP1D0A";
/// Entry renamed.
pub const RENAMED: &str = "TCP1D0B";
/// An entry cannot change while it is active.
pub const IN_USE: &str = "TCP1D0C";
/// A table is full.
pub const FULL: &str = "TCP1D0D";
/// No interface for an address (oracle `QTCPMSGF`).
pub const NO_IFC: &str = "TCP2658";
/// TCP/IP services not available (oracle `QTCPMSGF`).
pub const NOT_AVAILABLE: &str = "TCP2640";
/// Start of a line failed (oracle `QTCPMSGF`).
pub const LINE_FAILED: &str = "TCP8007";
/// Line not found (oracle `QTCPMSGF`).
pub const LINE_NOT_FOUND: &str = "TCP9509";
/// Duplicate internet address or line name (oracle `QTCPMSGF`).
pub const DUPLICATE: &str = "TCP9515";
/// Duplicate port entry (oracle `QTCPMSGF`).
pub const DUP_PORT: &str = "TCP9517";
/// Line not valid for TCP/IP (oracle `QTCPMSGF`).
pub const LINE_NOT_VALID: &str = "TCP9518";
/// Host table has no entries (oracle `QTCPMSGF`).
pub const NO_HOSTS: &str = "TCP9640";
/// No route entries (oracle `QTCPMSGF`).
pub const NO_ROUTES: &str = "TCP9642";
/// No port entries (oracle `QTCPMSGF`).
pub const NO_PORTS: &str = "TCP9644";
/// Host and domain name together longer than 255 (oracle `QTCPMSGF`).
pub const NAME_TOO_LONG: &str = "TCP9650";
/// Local domain name not valid (oracle `QTCPMSGF`).
pub const BAD_DOMAIN: &str = "TCP9880";

/// Every identifier above.
pub const ALL: &[&str] = &[
    STARTED,
    ALREADY_ACTIVE,
    ENDED,
    NOT_ACTIVE,
    SERVER_NOT_STARTED,
    IFC_STARTED,
    IFC_ENDED,
    IFC_ACTIVE,
    IFC_INACTIVE,
    IFC_FAILED,
    RTE_FAILED,
    ATTR_FAILED,
    ENDING,
    ADDED,
    CHANGED,
    REMOVED,
    EXISTS,
    NOT_FOUND,
    BAD_VALUE,
    ATTRS_CHANGED,
    MERGED,
    RENAMED,
    IN_USE,
    FULL,
    NO_IFC,
    NOT_AVAILABLE,
    LINE_FAILED,
    LINE_NOT_FOUND,
    DUPLICATE,
    DUP_PORT,
    LINE_NOT_VALID,
    NO_HOSTS,
    NO_ROUTES,
    NO_PORTS,
    NAME_TOO_LONG,
    BAD_DOMAIN,
];

/// A message the engine reports: its identifier, our text and whether
/// it is an error (diagnostic) rather than information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// The identifier.
    pub msgid: String,
    /// The text.
    pub text: String,
    /// A diagnostic rather than information.
    pub error: bool,
}

impl Note {
    /// Information.
    pub fn info(msgid: &str, text: impl Into<String>) -> Self {
        Self {
            msgid: msgid.into(),
            text: text.into(),
            error: false,
        }
    }

    /// A diagnostic.
    pub fn diag(msgid: &str, text: impl Into<String>) -> Self {
        Self {
            msgid: msgid.into(),
            text: text.into(),
            error: true,
        }
    }

    /// `MSGID text`, one line.
    pub fn line(&self) -> String {
        format!("{} {}", self.msgid, self.text)
    }
}
