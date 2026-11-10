// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The APIs' errors: a `QCPFMSG` identifier and its replacement data,
//! laid out as the primary's (`tools/conformance/oracle/netserver/`),
//! and the reason codes each message carries.

use crate::fmt::text::fixed;

/// A failed API call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    /// `CPFB68A`, ...
    pub id: &'static str,
    /// Replacement data, EBCDIC and big-endian.
    pub data: Vec<u8>,
}

impl ApiError {
    /// A failure with no data.
    #[must_use]
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            data: Vec::new(),
        }
    }

    /// A failure whose data starts with the API's name (10 bytes).
    #[must_use]
    pub fn of(id: &'static str, api: &str) -> Self {
        Self::new(id).text(api, 10)
    }

    /// Append a text field of `n` bytes.
    #[must_use]
    pub fn text(mut self, s: &str, n: usize) -> Self {
        self.data.extend(fixed(s, n));
        self
    }

    /// Append raw bytes.
    #[must_use]
    pub fn raw(mut self, b: &[u8]) -> Self {
        self.data.extend_from_slice(b);
        self
    }

    /// Append a 4-byte integer.
    #[must_use]
    pub fn bin4(mut self, v: i32) -> Self {
        self.data.extend(v.to_be_bytes());
        self
    }

    /// Append an 8-byte integer.
    #[must_use]
    pub fn bin8(mut self, v: i64) -> Self {
        self.data.extend(v.to_be_bytes());
        self
    }

    /// The reason code at `off` of the data, for tests and logs.
    #[must_use]
    pub fn reason_at(&self, off: usize) -> i32 {
        crate::fmt::text::bin4(&self.data, off)
    }
}

/// The messages.
pub mod msg {
    /// The `QZLSSERVER` job has ended (not active).
    pub const JOB_ENDED: &str = "CPFB680";
    /// Internal error.
    pub const INTERNAL: &str = "CPFB681";
    /// API failed with a reason code (`QZLSADPS`, `QZLSCHPS`
    /// publishing).
    pub const FAILED: &str = "CPFB682";
    /// Data conversion failed.
    pub const CONVERSION: &str = "CPFB683";
    /// Not authorized to the API.
    pub const AUTHORITY: &str = "CPFB684";
    /// Unable to start.
    pub const START: &str = "CPFB685";
    /// Error configuring (`QZLSCHSI` values).
    pub const CONFIGURE: &str = "CPFB686";
    /// The guest profile cannot be found.
    pub const GUEST_NOT_FOUND: &str = "CPFB687";
    /// Error opening the information (`QZLSOLST`).
    pub const OPEN_LIST: &str = "CPFB688";
    /// Unable to end.
    pub const END: &str = "CPFB689";
    /// Error with a shared resource.
    pub const SHARE: &str = "CPFB68A";
    /// A character not valid for a value.
    pub const CHARACTER: &str = "CPFB68B";
    /// The guest profile has a special authority.
    pub const GUEST_SPECIAL: &str = "CPFB68C";
    /// A length not valid.
    pub const LENGTH: &str = "CPFB68D";
    /// Error ending a session.
    pub const END_SESSION: &str = "CPFB68E";
    /// Error listing the information (`QZLSLSTI`).
    pub const LIST: &str = "CPFB68F";
    /// Error changing the information (`QZLSCHSI` format).
    pub const CHANGE: &str = "CPFB690";
    /// Users were not enabled.
    pub const NOT_ENABLED: &str = "CPFB694";
    /// Not authorized to enable a user (diagnostic).
    pub const ENABLE_AUTHORITY: &str = "CPDB680";
    /// An authorization list does not exist.
    pub const NO_AUTL: &str = "CPF2283";
}

/// `CPFB684`'s reasons.
pub mod authority {
    /// No `*IOSYSCFG`.
    pub const IOSYSCFG: i32 = 1;
    /// No `*IOSYSCFG` and `*SECADM`.
    pub const IOSYSCFG_SECADM: i32 = 2;
    /// The directory or output queue does not exist, and no `*IOSYSCFG`.
    pub const MISSING: i32 = 3;
    /// It exists, the caller does not own it and has no `*IOSYSCFG`.
    pub const NOT_OWNER: i32 = 4;
}

/// `CPFB68A`'s reasons.
pub mod share {
    /// The share exists already.
    pub const DUPLICATE: i32 = 1;
    /// The share does not exist.
    pub const MISSING: i32 = 2;
    /// Permission not valid.
    pub const PERMISSION: i32 = 3;
    /// Maximum users not valid.
    pub const MAX_USERS: i32 = 4;
    /// Spooled file type not valid.
    pub const SPOOL_TYPE: i32 = 5;
    /// The path does not begin with `/`.
    pub const PATH: i32 = 7;
    /// A file share named for a print API, or the other way.
    pub const WRONG_TYPE: i32 = 8;
    /// Text conversion value not valid.
    pub const TEXT_CONVERSION: i32 = 10;
    /// Publish value not valid.
    pub const PUBLISH: i32 = 11;
}

/// `CPFB686`'s reasons.
pub mod configure {
    /// No CCSID for the code page.
    pub const CODE_PAGE: i32 = 5;
    /// WINS primary (QS/400: every WINS value).
    pub const WINS_PRIMARY: i32 = 6;
    /// WINS secondary.
    pub const WINS_SECONDARY: i32 = 7;
    /// Browse interval out of range.
    pub const BROWSE: i32 = 8;
    /// Oplock time-out out of range.
    pub const OPLOCK: i32 = 9;
    /// Idle time-out out of range.
    pub const IDLE: i32 = 10;
    /// WINS proxy not `0` or `1`.
    pub const WINS_PROXY: i32 = 12;
    /// Logon server role (QS/400: only 0).
    pub const ROLE: i32 = 13;
    /// Authentication method not `0`, `1` or `2`.
    pub const AUTH_METHOD: i32 = 14;
    /// Network authentication (Kerberos) is not configured.
    pub const KERBEROS: i32 = 15;
    /// Message authentication not 0, 1 or 2.
    pub const MESSAGE_AUTH: i32 = 16;
    /// Minimum message severity (QS/400: only -1).
    pub const SEVERITY: i32 = 17;
    /// LAN Manager authentication (QS/400: only 0).
    pub const LANMAN: i32 = 18;
}

/// `CPFB685`'s reasons.
pub mod start {
    /// `QSERVER` is not active.
    pub const QSERVER: i32 = 1;
    /// `QZLSSERVER` is active already.
    pub const ACTIVE: i32 = 2;
    /// The reset value is not `0` or `1`.
    pub const RESET_VALUE: i32 = 5;
}

/// `CPFB68E`'s reasons.
pub mod end_session {
    /// No session of the workstation.
    pub const NO_WORKSTATION: i32 = 1;
    /// `QZLSSERVER` is not active.
    pub const NOT_ACTIVE: i32 = 2;
    /// A session identifier without `*SESSID`.
    pub const ID_WITHOUT_SESSID: i32 = 4;
    /// No session has the identifier.
    pub const NO_SESSION: i32 = 5;
    /// `*EXPANDED` without an expanded name.
    pub const EXPANDED: i32 = 6;
}

/// `CPFB68F`'s reasons (and `CPFB688`'s 1 and 2).
pub mod list {
    /// The format does not exist.
    pub const FORMAT: i32 = 1;
    /// `QZLSOLST`: `QZLSLSTI` refused the request.
    pub const LSTI: i32 = 2;
    /// The user space's library is not found.
    pub const LIBRARY: i32 = 2;
    /// The user space is not found.
    pub const SPACE: i32 = 3;
    /// The qualifier is not valid for the format.
    pub const QUALIFIER: i32 = 7;
    /// `*SESSID` given for one of qualifier and user only, or a session
    /// identifier without it.
    pub const SESSID: i32 = 9;
    /// `ZLSL0900` with a qualifier that is not blank.
    pub const NOT_BLANK: i32 = 10;
    /// `*EXPANDED` without an expanded name.
    pub const EXPANDED: i32 = 11;
}

/// The return code a refused WINS value carries in `CPFB686`
/// (`EOPNOTSUPP`).
pub const NOT_SUPPORTED_RC: i32 = 3440;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_is_laid_out_in_order() {
        let e = ApiError::of(msg::OPEN_LIST, "QZLSOLST")
            .bin4(2)
            .text("ZLSL0400", 8);
        assert_eq!(e.data.len(), 22);
        assert_eq!(e.reason_at(10), 2);
    }
}
