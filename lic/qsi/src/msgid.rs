// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The message identifiers of QSI's own refusals. A refusal carries one
//! and its text; a managed system's own refusals (a host's, a
//! system's) carry its identifiers, passed through as it gives them.
//! `QSI0001` to `QSI0099` are shared with the partition scope
//! (`qsipart::service::msgid`, `QSI0010` to `QSI0022` are its own);
//! `QSI01nn` are the session's, `QSI02nn` pairing's, `QSI03nn` to
//! `QSI09nn` the families'.

/// A request of a family or operation the receiver does not know.
pub const UNKNOWN: &str = "QSI0001";
/// A request whose body does not read, or is out of range.
pub const BAD: &str = "QSI0002";
/// A request of a family the two sides did not agree on.
pub const NOT_AGREED: &str = "QSI0003";

/// Before a pin, nothing but pairing is answered.
pub const NOT_PINNED: &str = "QSI0101";
/// More requests outstanding than the limit.
pub const TOO_MANY_REQUESTS: &str = "QSI0102";
/// More streams open than the limit.
pub const TOO_MANY_STREAMS: &str = "QSI0103";
/// A message longer than the limit.
pub const TOO_LONG: &str = "QSI0104";
/// Data on a stream beyond the credit given.
pub const NO_CREDIT: &str = "QSI0105";
/// A stream that is not open.
pub const NO_STREAM: &str = "QSI0106";
/// Nothing heard within the keep-alive timeout.
pub const TIMED_OUT: &str = "QSI0107";
/// A message out of order: before the hello, or a second hello.
pub const OUT_OF_ORDER: &str = "QSI0108";
/// A frame that does not read: the connection is closed.
pub const BAD_FRAME: &str = "QSI0109";
/// The peer is closing: no new requests.
pub const CLOSING: &str = "QSI0110";
/// The peer speaks no version this side does.
pub const NO_VERSION: &str = "QSI0111";
/// The peer is not what this side talks to: a console to a console, or
/// a managed system to a managed system.
pub const WRONG_PEER: &str = "QSI0112";

/// No pairing password is out, it has expired, or its attempts are
/// spent.
pub const NO_PAIRING: &str = "QSI0201";
/// The pairing exchange did not confirm: a wrong password, or someone
/// in the middle. Deliberately one identifier for both.
pub const PAIRING_FAILED: &str = "QSI0202";
/// A pinned peer asked to pair again.
pub const ALREADY_PAIRED: &str = "QSI0203";
/// A pairing step out of order (a confirmation without a start).
pub const PAIRING_ORDER: &str = "QSI0204";
