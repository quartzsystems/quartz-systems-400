// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The list formats of `QZLSLSTI` and `QZLSOLST` (`QSYSINC/H` member
//! `QZLSLSTI`): `ZLSL0100` to `ZLSL0900`. Character fields are CCSID
//! 37, paths in the job's CCSID. [`Level`] picks the 7.5 layout (the
//! encryption and authorization list fields) or the 7.1 one the
//! primary returns, for the captures.

use super::text::Rec;
use crate::model::attrs::Settings;
use crate::model::config::Config;
use crate::model::session::{Session, SessionTable, Stats};
use crate::model::share::{Kind, Share};

/// Which release's layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// V7R1M0: no encryption or authorization list fields.
    V7R1,
    /// 7.5, what QS/400 returns.
    V7R5,
}

/// `ZLSL0100`'s fixed part.
pub const L0100_FIXED: usize = 164;
/// `ZLSL0100`'s record: the fixed part and a 1024-byte path area.
pub const L0100_SIZE: usize = L0100_FIXED + 1024;
/// `ZLSL0200`.
pub const L0200_SIZE: usize = 736;
/// `ZLSL0201` at 7.1.
pub const L0201_SIZE_V7R1: usize = 772;
/// `ZLSL0201` at 7.5.
pub const L0201_SIZE: usize = 800;
/// `ZLSL0300`.
pub const L0300_SIZE: usize = 192;
/// `ZLSL0400`.
pub const L0400_SIZE: usize = 96;
/// `ZLSL0600`.
pub const L0600_SIZE: usize = 64;
/// `ZLSL0700`.
pub const L0700_SIZE: usize = 192;
/// `ZLSL0800`.
pub const L0800_SIZE: usize = 16;
/// `ZLSL0900`.
pub const L0900_SIZE: usize = 10;
/// An extension table entry: its length and 46 bytes.
pub const EXTENSION_SIZE: usize = 50;

/// `ZLSL0101`'s fixed part at a level.
#[must_use]
pub fn l0101_fixed(level: Level) -> usize {
    match level {
        Level::V7R1 => 198,
        Level::V7R5 => 209,
    }
}

/// A path in CCSID `ccsid` (37 when the conversion is not known).
fn path_bytes(path: &str, ccsid: u16) -> Vec<u8> {
    if ccsid != 0
        && ccsid != 37
        && let Ok(b) = ccsid::conv::from_str(ccsid, path)
    {
        return b;
    }
    path.chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(0x6F))
        .collect()
}

/// `name` then `library`, 10 bytes each.
fn qualified(r: &mut Rec, off: usize, name: &str, lib: &str) {
    r.char(off, 10, name);
    r.char(off + 10, 10, lib);
}

/// Connections now using a share.
#[must_use]
pub fn current_users(t: &SessionTable, share: &str) -> usize {
    t.sessions()
        .flat_map(|s| s.connections.iter())
        .filter(|c| c.share.eq_ignore_ascii_case(share))
        .count()
}

fn common(r: &mut Rec, s: &Share, size: usize, users: usize) {
    r.size4(0, size);
    r.char(4, 12, &s.name);
    r.bin4(16, s.device_type());
    match &s.kind {
        Kind::File(f) => {
            r.bin4(20, f.permission.code());
            r.bin4(24, f.max_users);
            r.size4(28, users);
            r.bin4(32, 0);
            r.blanks(44, 20);
            r.blanks(64, 50);
        }
        Kind::Print(p) => {
            r.bin4(20, 0);
            r.bin4(24, 0);
            r.size4(28, users);
            r.bin4(32, p.spool_type.code());
            qualified(r, 44, &p.outq.name, &p.outq.library);
            r.char(64, 50, &p.driver);
        }
    }
    r.char(114, 50, &s.text);
}

/// A `ZLSL0100` record. `offset_base` is added to the path offset: 0
/// for offsets from the entry (`QZLSOLST`), the entry's position for
/// offsets from the user space's start (`QZLSLSTI`).
#[must_use]
pub fn l0100(s: &Share, users: usize, ccsid: u16, offset_base: usize) -> Vec<u8> {
    let mut r = Rec::zeros(L0100_SIZE);
    common(&mut r, s, L0100_SIZE, users);
    let path = s
        .as_file()
        .map(|f| path_bytes(&f.path, ccsid))
        .unwrap_or_default();
    let path = &path[..path.len().min(1024)];
    r.size4(36, offset_base + L0100_FIXED);
    r.size4(40, path.len());
    r.bytes(L0100_FIXED, path);
    r.buf
}

/// A `ZLSL0101` record placed at `at` (offsets count from the
/// buffer's start). Its length is its size.
#[must_use]
pub fn l0101(s: &Share, users: usize, ccsid: u16, at: usize, level: Level) -> Vec<u8> {
    let fixed = l0101_fixed(level);
    let path = s
        .as_file()
        .map(|f| path_bytes(&f.path, ccsid))
        .unwrap_or_default();
    let exts: &[String] = s.as_file().map_or(&[], |f| f.extensions.as_slice());
    let size = fixed + path.len() + EXTENSION_SIZE * exts.len();
    let mut r = Rec::zeros(size);
    common(&mut r, s, size, users);
    r.size4(36, at + fixed);
    r.size4(40, path.len());
    match &s.kind {
        Kind::File(f) => {
            r.blanks(164, 20);
            r.bin4(184, f.text_ccsid);
            r.flag(196, f.text_conversion);
            r.flag(197, false);
        }
        Kind::Print(p) => {
            qualified(&mut r, 164, &p.printer_file.name, &p.printer_file.library);
            r.bin4(184, 0);
            r.flag(196, false);
            r.flag(197, p.publish);
        }
    }
    if exts.is_empty() {
        r.bin4(188, 0);
    } else {
        r.size4(188, at + fixed + path.len());
    }
    r.size4(192, exts.len());
    if level == Level::V7R5 {
        r.flag(198, s.encryption_required);
        r.char(199, 10, &s.autl);
    }
    r.bytes(fixed, &path);
    let mut e = fixed + path.len();
    for x in exts {
        let b = super::text::fixed(x, 46);
        r.size4(e, x.chars().count().min(46));
        r.bytes(e + 4, &b);
        e += EXTENSION_SIZE;
    }
    r.buf
}

fn pair_int(r: &mut Rec, off: usize, c: i32, p: i32) {
    r.bin4(off, c);
    r.bin4(off + 4, p);
}

fn pair_char(r: &mut Rec, off: usize, n: usize, c: &str, p: &str) {
    r.char(off, n, c);
    r.char(off + n, n, p);
}

fn l0200_part(r: &mut Rec, c: &Settings, p: &Settings) {
    pair_int(r, 0, c.code_page, p.code_page);
    pair_int(r, 8, c.idle_timeout, p.idle_timeout);
    pair_int(r, 16, c.oplock_timeout, p.oplock_timeout);
    pair_int(r, 24, c.browse_interval, p.browse_interval);
    pair_int(r, 32, i32::from(c.wins_proxy), i32::from(p.wins_proxy));
    pair_int(
        r,
        40,
        i32::from(c.guest_support()),
        i32::from(p.guest_support()),
    );
    pair_char(r, 48, 10, &c.guest, &p.guest);
    pair_char(r, 68, 15, &c.server_name, &p.server_name);
    pair_char(r, 98, 15, &c.domain, &p.domain);
    pair_char(r, 128, 50, &c.text, &p.text);
    pair_char(r, 228, 15, &c.wins_primary, &p.wins_primary);
    pair_char(r, 258, 15, &c.wins_secondary, &p.wins_secondary);
    pair_char(r, 288, 224, &c.scope, &p.scope);
}

/// The `ZLSL0200` record.
#[must_use]
pub fn l0200(cfg: &Config) -> Vec<u8> {
    let mut r = Rec::zeros(L0200_SIZE);
    l0200_part(&mut r, &cfg.attrs.current, &cfg.attrs.pending);
    r.buf
}

fn digit(n: u8) -> String {
    char::from(b'0' + n.min(9)).to_string()
}

/// The `ZLSL0201` record.
#[must_use]
pub fn l0201(cfg: &Config, level: Level) -> Vec<u8> {
    let (c, p) = (&cfg.attrs.current, &cfg.attrs.pending);
    let size = match level {
        Level::V7R1 => L0201_SIZE_V7R1,
        Level::V7R5 => L0201_SIZE,
    };
    let mut r = Rec::zeros(size);
    l0200_part(&mut r, c, p);
    r.flag(736, c.system_name_access);
    r.flag(737, p.system_name_access);
    r.char(738, 1, &digit(c.auth_method));
    r.char(739, 1, &digit(p.auth_method));
    pair_int(&mut r, 740, c.server_role, p.server_role);
    pair_int(&mut r, 748, c.message_auth, p.message_auth);
    pair_int(&mut r, 756, c.min_message_severity, p.min_message_severity);
    pair_int(&mut r, 764, c.lanman_auth, p.lanman_auth);
    if level == Level::V7R5 {
        pair_int(&mut r, 772, c.encryption, p.encryption);
        pair_char(&mut r, 780, 10, &c.server_autl, &p.server_autl);
    }
    r.buf
}

/// The name a session's workstation goes by in the 15-byte fields, and
/// `ZLSL0300`'s workstation name type: 0 the name fits, 1 it is
/// `*EXPANDED` and the expanded name holds it.
#[must_use]
pub fn workstation(s: &Session) -> (String, i32) {
    if s.workstation.chars().count() > 15 {
        ("*EXPANDED".into(), 1)
    } else {
        (s.workstation.clone(), 0)
    }
}

fn secs(now: u64, since: u64) -> i32 {
    i32::try_from(now.saturating_sub(since)).unwrap_or(i32::MAX)
}

/// A `ZLSL0300` record. Logon type `1` is a user, `0` the guest; the
/// encrypted password flag is always `1` (NTLMv2 is the only
/// authentication).
#[must_use]
pub fn l0300(t: &SessionTable, s: &Session, now: u64) -> Vec<u8> {
    let mut r = Rec::zeros(L0300_SIZE);
    let (ws, ty) = workstation(s);
    r.char(0, 15, &ws);
    r.char(15, 10, &s.user);
    r.size4(28, s.connections.len());
    r.bin4(32, i32::try_from(s.opens()).unwrap_or(i32::MAX));
    r.size4(36, t.of_workstation(&s.workstation).len());
    r.bin4(40, secs(now, s.since));
    r.bin4(44, secs(now, s.last_active));
    r.flag(48, !s.guest);
    r.flag(49, true);
    r.bin8(56, i64::try_from(s.id).unwrap_or(i64::MAX));
    r.bin4(64, ty);
    r.char(68, 124, &s.expanded);
    r.buf
}

/// The `ZLSL0400` record; `stamp` gives a time as `CYYMMDDHHMMSS`.
#[must_use]
pub fn l0400(st: &Stats, guest_support: bool, stamp: &dyn Fn(u64) -> String) -> Vec<u8> {
    let mut r = Rec::zeros(L0400_SIZE);
    let n = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
    r.bin4(0, n(st.opens));
    r.bin4(4, n(st.print_jobs));
    r.bin4(8, n(st.session_starts));
    r.bin4(12, n(st.auto_disconnects));
    r.bin4(16, n(st.normal_disconnects));
    r.bin4(20, n(st.duplicate_sessions));
    r.bin4(24, n(st.session_errors));
    r.bin4(28, n(st.password_violations));
    r.bin4(32, i32::from(guest_support));
    r.bin4(36, n(st.unknown_users));
    let halves = |v: u64| {
        let low = u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0);
        let high = u32::try_from(v >> 32).unwrap_or(0);
        (
            i32::from_be_bytes(low.to_be_bytes()),
            i32::from_be_bytes(high.to_be_bytes()),
        )
    };
    let (sl, sh) = halves(st.bytes_sent);
    let (rl, rh) = halves(st.bytes_received);
    r.bin4(40, sl);
    r.bin4(44, sh);
    r.bin4(48, rl);
    r.bin4(52, rh);
    r.bin4(56, st.average_response());
    let (start, reset) = if st.started == 0 {
        (String::new(), String::new())
    } else {
        (stamp(st.started), stamp(st.reset))
    };
    r.char(60, 13, &start);
    r.char(73, 13, &reset);
    r.buf
}

/// The `ZLSL0600` records of a workstation's sessions.
#[must_use]
pub fn l0600(sessions: &[&Session], now: u64) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for s in sessions {
        for c in &s.connections {
            let mut r = Rec::zeros(L0600_SIZE);
            r.bin4(0, i32::try_from(c.id).unwrap_or(i32::MAX));
            r.bin4(4, c.kind);
            r.bin4(8, i32::try_from(c.opens).unwrap_or(i32::MAX));
            r.bin4(12, 1);
            r.bin4(16, secs(now, c.since));
            r.char(20, 10, &s.user);
            r.char(30, 12, &c.share);
            r.bin8(48, i64::try_from(s.id).unwrap_or(i64::MAX));
            out.push(r.buf);
        }
    }
    out
}

/// The `ZLSL0700` records of the connections to share `share`.
#[must_use]
pub fn l0700(sessions: &[&Session], share: Option<&str>, now: u64) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for s in sessions {
        let (ws, ty) = workstation(s);
        for c in s
            .connections
            .iter()
            .filter(|c| share.is_none_or(|n| c.share.eq_ignore_ascii_case(n)))
        {
            let mut r = Rec::zeros(L0700_SIZE);
            r.bin4(0, i32::try_from(c.id).unwrap_or(i32::MAX));
            r.bin4(4, c.kind);
            r.bin4(8, i32::try_from(c.opens).unwrap_or(i32::MAX));
            r.bin4(12, 1);
            r.bin4(16, secs(now, c.since));
            r.char(20, 10, &s.user);
            r.char(30, 15, &ws);
            r.bin8(48, i64::try_from(s.id).unwrap_or(i64::MAX));
            r.bin4(64, ty);
            r.char(68, 124, &s.expanded);
            out.push(r.buf);
        }
    }
    out
}

/// A `ZLSL0800` record.
#[must_use]
pub fn l0800(s: &Share) -> Vec<u8> {
    let mut r = Rec::zeros(L0800_SIZE);
    r.char(0, 12, &s.name);
    r.bin4(12, s.device_type());
    r.buf
}

/// A `ZLSL0900` record.
#[must_use]
pub fn l0900(user: &str) -> Vec<u8> {
    super::text::fixed(user, L0900_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt::text::{bin4, text_at};
    use crate::model::share::{Permission, Qualified, SpoolType};

    #[test]
    fn a_file_share_in_0100_and_0101() {
        let mut s = Share::file("QIBM", "/QIBM", "IBM Product Directories");
        let r = l0100(&s, 0, 37, 0);
        assert_eq!(r.len(), 1188);
        assert_eq!(bin4(&r, 0), 1188);
        assert_eq!(bin4(&r, 20), 2);
        assert_eq!(bin4(&r, 24), -1);
        assert_eq!(bin4(&r, 36), 164);
        assert_eq!(bin4(&r, 40), 5);
        assert_eq!(text_at(&r, 164, 5), "/QIBM");
        assert_eq!(bin4(&l0100(&s, 0, 37, 619), 36), 783);
        if let Kind::File(f) = &mut s.kind {
            f.permission = Permission::ReadOnly;
            f.extensions = vec!["TXT".into()];
        }
        let r = l0101(&s, 2, 37, 100, Level::V7R5);
        assert_eq!(r.len(), 209 + 5 + 50);
        assert_eq!(bin4(&r, 0), 264);
        assert_eq!(bin4(&r, 28), 2);
        assert_eq!(bin4(&r, 36), 309);
        assert_eq!(bin4(&r, 188), 314);
        assert_eq!(bin4(&r, 192), 1);
        assert_eq!(bin4(&r, 214), 3);
        assert_eq!(text_at(&r, 218, 46), "TXT");
        assert_eq!(r[198], 0xF0);
    }

    #[test]
    fn a_print_share_in_0101() {
        let mut s = Share::print("LASER", Qualified::new("PRT01", "QUSRSYS"), "Laser");
        if let Kind::Print(p) = &mut s.kind {
            p.spool_type = SpoolType::UserAscii;
            p.driver = "HP LaserJet 4".into();
        }
        let r = l0101(&s, 0, 37, 0, Level::V7R5);
        assert_eq!(r.len(), 209);
        assert_eq!(bin4(&r, 16), 1);
        assert_eq!(bin4(&r, 32), 1);
        assert_eq!(text_at(&r, 44, 10), "PRT01");
        assert_eq!(text_at(&r, 54, 10), "QUSRSYS");
        assert_eq!(text_at(&r, 64, 50), "HP LaserJet 4");
        assert_eq!(bin4(&r, 40), 0);
    }

    #[test]
    fn attribute_records_have_their_sizes() {
        let c = Config::shipped("QS400");
        assert_eq!(l0200(&c).len(), 736);
        assert_eq!(l0201(&c, Level::V7R1).len(), 772);
        let r = l0201(&c, Level::V7R5);
        assert_eq!(r.len(), 800);
        assert_eq!(bin4(&r, 756), -1);
        assert_eq!(text_at(&r, 68, 15), "QS400");
    }
}
