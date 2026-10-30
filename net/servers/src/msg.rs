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
];
