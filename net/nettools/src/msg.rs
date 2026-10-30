// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The messages the tools send: IBM's identifiers (`QSYS/QTCPMSG` for
//! PING and TRACEROUTE, `QCPFMSG` `CPF39xx` for the communications
//! trace, as the oracle lists them) with our texts. The descriptions
//! are in `runtime/msgf/msgs/`; `tests/messages.rs` checks each one.

/// How a message is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Informational.
    Info,
    /// Diagnostic.
    Diag,
    /// Completion.
    Comp,
    /// Escape: the command failed.
    Escape,
}

/// One message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msg {
    /// The identifier.
    pub id: &'static str,
    /// The text, substitution done.
    pub text: String,
    /// How it is sent.
    pub kind: Kind,
}

impl Msg {
    /// A message.
    pub fn new(id: &'static str, kind: Kind, text: impl Into<String>) -> Self {
        Self { id, text: text.into(), kind }
    }

    /// `ID text`, as the console shows it.
    pub fn line(&self) -> String {
        format!("{} {}", self.id, self.text)
    }
}

/// Remote system name too long.
pub const NAME_TOO_LONG: &str = "TCP3200";
/// Unknown host.
pub const UNKNOWN_HOST: &str = "TCP3202";
/// Verifying a named host at an address.
pub const VERIFY_AT: &str = "TCP3203";
/// Verifying an address.
pub const VERIFY: &str = "TCP3204";
/// No reply within the wait time.
pub const NO_REPLY: &str = "TCP3206";
/// Local address not valid.
pub const BAD_LOCAL: &str = "TCP3207";
/// Statistics.
pub const STATS: &str = "TCP3210";
/// Round-trip times.
pub const ROUND_TRIP: &str = "TCP3211";
/// An IPv4 reply.
pub const REPLY: &str = "TCP3215";
/// An IPv4 duplicate reply.
pub const REPLY_DUP: &str = "TCP3216";
/// An IPv6 reply.
pub const REPLY6: &str = "TCP3217";
/// An IPv6 duplicate reply.
pub const REPLY6_DUP: &str = "TCP3218";
/// Address not of the version asked.
pub const BAD_VERSION: &str = "TCP3219";
/// Starting TTL greater than the limit.
pub const BAD_RANGE: &str = "TCP3252";
/// Probing routes.
pub const PROBING: &str = "TCP3260";
/// One hop.
pub const HOP: &str = "TCP3261";
/// A socket operation failed.
pub const SOCKET_FAILED: &str = "TCP3263";

/// `STRCMNTRC` done.
pub const TRC_STARTED: &str = "CPF39A0";
/// `ENDCMNTRC` done.
pub const TRC_ENDED: &str = "CPF39A1";
/// `DLTCMNTRC` done.
pub const TRC_DELETED: &str = "CPF39A2";
/// `PRTCMNTRC` done.
pub const TRC_PRINTED: &str = "CPF39A3";
/// `CHKCMNTRC` done.
pub const TRC_CHECKED: &str = "CPF39A4";
/// A trace's status.
pub const TRC_STATUS: &str = "CPF39A5";
/// Not authorized to the trace tool.
pub const TRC_NOT_AUTH: &str = "CPF39A8";
/// The trace function failed.
pub const TRC_FAILED: &str = "CPF39A9";
/// A trace already exists.
pub const TRC_EXISTS: &str = "CPF39AA";
/// The object cannot be traced.
pub const TRC_CANNOT: &str = "CPF39AD";
/// The trace already ended.
pub const TRC_ALREADY_ENDED: &str = "CPF39AE";
/// No traces at all.
pub const TRC_NONE: &str = "CPF39B0";
/// The trace does not exist.
pub const TRC_NOT_FOUND: &str = "CPF39B1";
/// The trace's data cannot be deleted (it is active).
pub const TRC_CANNOT_DELETE: &str = "CPF39B2";
/// The trace holds no data.
pub const TRC_NO_DATA: &str = "CPF39B3";
/// No traces of a type.
pub const TRC_NONE_OF_TYPE: &str = "CPF39BE";
/// No records selected for printing.
pub const TRC_NO_RECORDS: &str = "CPF39B9";
/// The trace cannot end (still active elsewhere).
pub const TRC_CANNOT_END: &str = "CPF39C3";

/// Every identifier here, for the message file check.
pub const ALL: &[&str] = &[
    NAME_TOO_LONG,
    UNKNOWN_HOST,
    VERIFY_AT,
    VERIFY,
    NO_REPLY,
    BAD_LOCAL,
    STATS,
    ROUND_TRIP,
    REPLY,
    REPLY_DUP,
    REPLY6,
    REPLY6_DUP,
    BAD_VERSION,
    BAD_RANGE,
    PROBING,
    HOP,
    SOCKET_FAILED,
    TRC_STARTED,
    TRC_ENDED,
    TRC_DELETED,
    TRC_PRINTED,
    TRC_CHECKED,
    TRC_STATUS,
    TRC_NOT_AUTH,
    TRC_FAILED,
    TRC_EXISTS,
    TRC_CANNOT,
    TRC_ALREADY_ENDED,
    TRC_NONE,
    TRC_NOT_FOUND,
    TRC_CANNOT_DELETE,
    TRC_NO_DATA,
    TRC_NONE_OF_TYPE,
    TRC_NO_RECORDS,
    TRC_CANNOT_END,
];
