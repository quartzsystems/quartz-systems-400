// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400's own formats for the attributes IBM's have no field for
//! (allow NetBIOS, the SMB2 minimum dialect, log sign-on messages):
//! `QSLL0100`, a `QZLSOLST`/`QZLSLSTI` record with the current and the
//! pending values, and `QSLS0100`, a `QZLSCHSI` request that makes new
//! ones pending.

use super::text::{Rec, ch, text_at};
use crate::model::attrs::{Dialect, Settings};

/// `QSLL0100`'s record.
pub const L0100_SIZE: usize = 20;
/// `QSLS0100`'s request.
pub const S0100_SIZE: usize = 10;

/// The `QSLL0100` record: allow NetBIOS current 1, pending 1; minimum
/// dialect current 8, pending 8; log sign-on current 1, pending 1.
#[must_use]
pub fn l0100(current: &Settings, pending: &Settings) -> Vec<u8> {
    let mut r = Rec::zeros(L0100_SIZE);
    r.flag(0, current.allow_netbios);
    r.flag(1, pending.allow_netbios);
    r.char(2, 8, current.min_dialect.name());
    r.char(10, 8, pending.min_dialect.name());
    r.flag(18, current.log_signon);
    r.flag(19, pending.log_signon);
    r.buf
}

/// The QS/400 attributes of one side of a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Own {
    /// Allow NetBIOS.
    pub allow_netbios: bool,
    /// The SMB2 minimum dialect.
    pub min_dialect: Dialect,
    /// Log sign-on messages.
    pub log_signon: bool,
}

impl Own {
    /// The values of `s`.
    #[must_use]
    pub fn of(s: &Settings) -> Self {
        Self {
            allow_netbios: s.allow_netbios,
            min_dialect: s.min_dialect,
            log_signon: s.log_signon,
        }
    }

    /// Put them in `s`.
    pub fn apply(self, s: &mut Settings) {
        s.allow_netbios = self.allow_netbios;
        s.min_dialect = self.min_dialect;
        s.log_signon = self.log_signon;
    }
}

/// Decode a `QSLL0100` record: current, then pending.
#[must_use]
pub fn read_l0100(b: &[u8]) -> (Own, Own) {
    let side = |flag: usize, dialect: usize, log: usize| Own {
        allow_netbios: ch(b, flag) == '1',
        min_dialect: Dialect::parse(&text_at(b, dialect, 8)).unwrap_or_default(),
        log_signon: ch(b, log) == '1',
    };
    (side(0, 2, 18), side(1, 10, 19))
}

/// The `QSLS0100` request: allow NetBIOS 1, minimum dialect 8, log
/// sign-on 1.
#[must_use]
pub fn s0100(own: Own) -> Vec<u8> {
    let mut r = Rec::zeros(S0100_SIZE);
    r.flag(0, own.allow_netbios);
    r.char(1, 8, own.min_dialect.name());
    r.flag(9, own.log_signon);
    r.buf
}

/// A `QSLS0100` request's fields as sent: the two flags and the
/// dialect's name.
#[must_use]
pub fn read_s0100(b: &[u8]) -> (char, String, char) {
    (ch(b, 0), text_at(b, 1, 8), ch(b, 9))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip() {
        let c = Settings::new("A");
        let mut p = c.clone();
        p.allow_netbios = true;
        p.min_dialect = Dialect::Smb311;
        let r = l0100(&c, &p);
        assert_eq!(r.len(), L0100_SIZE);
        let (rc, rp) = read_l0100(&r);
        assert_eq!(rc, Own::of(&c));
        assert_eq!(rp, Own::of(&p));
        let s = s0100(Own::of(&p));
        assert_eq!(read_s0100(&s), ('1', "3.1.1".to_string(), '0'));
    }
}
