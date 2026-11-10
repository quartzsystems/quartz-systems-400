// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QZLSCHSI`'s request formats: `ZLSS0100` (the attributes to make
//! pending) and `ZLSS0200` (users to enable).

use super::text::{bin4, ch, text_at};

/// `ZLSS0100` in full (7.5).
pub const S0100_SIZE: usize = 310;
/// `ZLSS0100` as 7.1 has it: through LAN Manager authentication, no
/// encryption or authorization list.
pub const S0100_MIN: usize = 296;

/// A `ZLSS0100` request; fields past the length given are `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct S0100 {
    /// Code page.
    pub code_page: i32,
    /// Idle time-out.
    pub idle_timeout: i32,
    /// Oplock time-out.
    pub oplock_timeout: i32,
    /// Browse interval.
    pub browse_interval: i32,
    /// WINS primary.
    pub wins_primary: String,
    /// WINS secondary.
    pub wins_secondary: String,
    /// Scope.
    pub scope: String,
    /// WINS proxy, `0` or `1`.
    pub wins_proxy: char,
    /// Domain logon support.
    pub domain_logon: i32,
    /// Logon server role.
    pub server_role: i32,
    /// Authentication method, `0`, `1` or `2`.
    pub auth_method: char,
    /// Message authentication.
    pub message_auth: i32,
    /// Minimum message severity.
    pub min_message_severity: i32,
    /// LAN Manager authentication.
    pub lanman_auth: i32,
    /// Encrypted connections (7.4 and later).
    pub encryption: Option<i32>,
    /// Server authorization list (7.5).
    pub server_autl: Option<String>,
}

/// Decode `ZLSS0100` from `b` (as long as the caller said).
#[must_use]
pub fn s0100(b: &[u8]) -> S0100 {
    S0100 {
        code_page: bin4(b, 0),
        idle_timeout: bin4(b, 4),
        oplock_timeout: bin4(b, 8),
        browse_interval: bin4(b, 12),
        wins_primary: text_at(b, 16, 15),
        wins_secondary: text_at(b, 31, 15),
        scope: text_at(b, 46, 224),
        wins_proxy: ch(b, 270),
        domain_logon: bin4(b, 272),
        server_role: bin4(b, 276),
        auth_method: ch(b, 280),
        message_auth: bin4(b, 284),
        min_message_severity: bin4(b, 288),
        lanman_auth: bin4(b, 292),
        encryption: (b.len() >= 300).then(|| bin4(b, 296)),
        server_autl: (b.len() >= S0100_SIZE).then(|| text_at(b, 300, 10)),
    }
}

/// Decode `ZLSS0200`: the profile names, as many as the count says and
/// the length holds.
#[must_use]
pub fn s0200(b: &[u8]) -> (i32, Vec<String>) {
    let n = bin4(b, 0);
    let fit = b.len().saturating_sub(4) / 10;
    let take = usize::try_from(n).unwrap_or(0).min(fit);
    (n, (0..take).map(|i| text_at(b, 4 + 10 * i, 10)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt::text::Rec;

    #[test]
    fn zlss0100_decodes_by_length() {
        let mut r = Rec::zeros(S0100_SIZE);
        r.bin4(4, 600);
        r.bin4(8, -1);
        r.blanks(16, 254);
        r.char(270, 1, "0");
        r.char(280, 1, "0");
        r.bin4(288, -1);
        r.bin4(296, 1);
        r.char(300, 10, "NSAUTL");
        let s = s0100(&r.buf);
        assert_eq!(s.idle_timeout, 600);
        assert_eq!(s.oplock_timeout, -1);
        assert_eq!(s.wins_primary, "");
        assert_eq!(s.wins_proxy, '0');
        assert_eq!(s.encryption, Some(1));
        assert_eq!(s.server_autl.as_deref(), Some("NSAUTL"));
        let short = s0100(&r.buf[..300]);
        assert_eq!(short.encryption, Some(1));
        assert_eq!(short.server_autl, None);
        assert_eq!(s0100(&r.buf[..S0100_MIN]).encryption, None);
    }

    #[test]
    fn zlss0200_takes_what_fits() {
        let mut r = Rec::zeros(4);
        r.bin4(0, 3);
        r.char(4, 10, "BOB");
        r.char(14, 10, "ANN");
        assert_eq!(
            s0200(&r.buf),
            (3, vec!["BOB".to_string(), "ANN".to_string()])
        );
    }
}
