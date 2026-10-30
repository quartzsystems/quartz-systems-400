// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The framework's message identifiers, described in `QTCPMSG`
//! (`runtime/msgf/msgs/qtcpmsg.toml`). They are ours in IBM's `TCP1Axx`
//! family, beside the TCP/IP configuration's.

/// `&1 cannot be started in this release.`
pub const NOT_AVAILABLE: &str = "TCP1A04";
/// `&1 could not be started.`
pub const NOT_STARTED: &str = "TCP1A05";
/// `Server instance &2 of &1 not found.`
pub const NO_INSTANCE: &str = "TCP1A06";
/// `&1 server starting.`
pub const STARTING: &str = "TCP1A07";
/// `&1 server already started.`
pub const ALREADY: &str = "TCP1A08";
/// `&1 server not active.`
pub const NOT_ACTIVE: &str = "TCP1A09";
/// `&1 server ending.`
pub const ENDING: &str = "TCP1A0A";
/// `&1 attributes changed.`
pub const ATTRS_CHANGED: &str = "TCP1A10";
/// `Value not valid: &1` (the configuration's).
pub const BAD_VALUE: &str = "TCP1D08";

/// `SNTP client adjusted the clock by &1 milliseconds.`
pub const NTP_ADJUSTED: &str = "TCP1A40";
/// `Clock not adjusted: &2 differs by &1 seconds, more than MAXADJ.`
pub const NTP_TOO_FAR: &str = "TCP1A41";
/// `Clock set at once: &2 differed by &1 seconds.`
pub const NTP_STEPPED: &str = "TCP1A42";
/// `No time server answered: &1.`
pub const NTP_NO_SERVER: &str = "TCP1A43";
/// `SNTP client not adjusting the clock: QTIMADJ names &1.`
pub const NTP_OTHER_ADJUSTER: &str = "TCP1A44";
/// `SNTP poll of &1: offset &2 ms, round trip &3 ms.`
pub const NTP_POLLED: &str = "TCP1A45";
/// `SNTP request from &1 &2.`
pub const NTP_SERVED: &str = "TCP1A46";
/// `LPD spooled file &1 from &2 into &3.`
pub const LPD_SPOOLED: &str = "TCP1A47";
/// `LPD job from &1 not spooled: &2.`
pub const LPD_REFUSED: &str = "TCP1A48";
/// `Spooled file &1 sent to queue &2 on &3.`
pub const SPLF_SENT: &str = "TCP1A49";
/// `Spooled file &1 not sent: &2.`
pub const SPLF_NOT_SENT: &str = "TCP1A4A";
/// `E-mail sent to &1 of &2 recipients.`
pub const MAIL_SENT: &str = "TCP1A4B";
/// `E-mail not sent: &1.`
pub const MAIL_NOT_SENT: &str = "TCP1A4C";

/// Every identifier above.
pub const ALL: &[&str] = &[
    NOT_AVAILABLE,
    NOT_STARTED,
    NO_INSTANCE,
    STARTING,
    ALREADY,
    NOT_ACTIVE,
    ENDING,
    ATTRS_CHANGED,
    BAD_VALUE,
    NTP_ADJUSTED,
    NTP_TOO_FAR,
    NTP_STEPPED,
    NTP_NO_SERVER,
    NTP_OTHER_ADJUSTER,
    NTP_POLLED,
    NTP_SERVED,
    LPD_SPOOLED,
    LPD_REFUSED,
    SPLF_SENT,
    SPLF_NOT_SENT,
    MAIL_SENT,
    MAIL_NOT_SENT,
];
