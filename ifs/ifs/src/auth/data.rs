// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IFS data authorities (`*R *W *X` and their combinations) over the
//! system's authority bits, as IBM maps them: `*R` is `*OBJOPR *READ`,
//! `*W` is `*OBJOPR *ADD *UPD *DLT`, `*X` is `*OBJOPR *EXECUTE`;
//! `*EXCLUDE` is none of them. POSIX mode bits are a view: owner,
//! primary group and `*PUBLIC` data authorities as `rwx` triples.

use sec::auth::bits::Auth;
use store::obj::header::Authority as A;

use crate::meta::{Flags, Meta};

/// The data authority bits.
const DATA: u32 = A::OBJOPR | A::READ | A::ADD | A::UPD | A::DLT | A::EXECUTE;
/// The object authorities the IFS shows (`*OBJEXIST *OBJMGT *OBJALTER
/// *OBJREF`).
pub const OBJECT: u32 = A::OBJEXIST | A::OBJMGT | A::OBJALTER | A::OBJREF;

/// `*R`.
pub const R: Auth = Auth(A::OBJOPR | A::READ);
/// `*W`.
pub const W: Auth = Auth(A::OBJOPR | A::ADD | A::UPD | A::DLT);
/// `*X`.
pub const X: Auth = Auth(A::OBJOPR | A::EXECUTE);
/// `*RW`.
pub const RW: Auth = Auth(R.0 | W.0);
/// `*RX`.
pub const RX: Auth = Auth(R.0 | X.0);
/// `*WX`.
pub const WX: Auth = Auth(W.0 | X.0);
/// `*RWX`.
pub const RWX: Auth = Auth(R.0 | W.0 | X.0);

/// A data authority as three bits: read 4, write 2, execute 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct DataAuth(pub u8);

impl DataAuth {
    /// Read.
    pub const R: u8 = 4;
    /// Write.
    pub const W: u8 = 2;
    /// Execute or search.
    pub const X: u8 = 1;

    /// From `rwx` mode bits (the low three).
    #[must_use]
    pub fn from_mode_bits(bits: u32) -> Self {
        Self(u8::try_from(bits & 7).expect("three bits"))
    }

    /// Which of `*R`, `*W`, `*X` a set of authorities holds in full.
    #[must_use]
    pub fn of(a: Auth) -> Self {
        let mut b = 0;
        if a.has(R) {
            b |= Self::R;
        }
        if a.has(W) {
            b |= Self::W;
        }
        if a.has(X) {
            b |= Self::X;
        }
        Self(b)
    }

    /// The authority bits.
    #[must_use]
    pub fn auth(self) -> Auth {
        let mut a = 0;
        if self.0 & Self::R != 0 {
            a |= R.0;
        }
        if self.0 & Self::W != 0 {
            a |= W.0;
        }
        if self.0 & Self::X != 0 {
            a |= X.0;
        }
        Auth(a)
    }

    /// These data authorities with `objects`' object authorities.
    #[must_use]
    pub fn with_objects(self, objects: Auth) -> Auth {
        Auth(self.auth().0 | (objects.0 & OBJECT))
    }

    /// The IBM name: `*RWX`, `*RX` ... `*EXCLUDE`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self.0 & 7 {
            7 => "*RWX",
            6 => "*RW",
            5 => "*RX",
            4 => "*R",
            3 => "*WX",
            2 => "*W",
            1 => "*X",
            _ => "*EXCLUDE",
        }
    }

    /// From an IBM name (`*NONE` is no data authority, like `*EXCLUDE`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(Self(match s.to_ascii_uppercase().as_str() {
            "*RWX" => 7,
            "*RW" => 6,
            "*RX" => 5,
            "*R" => 4,
            "*WX" => 3,
            "*W" => 2,
            "*X" => 1,
            "*EXCLUDE" | "*NONE" => 0,
            _ => return None,
        }))
    }
}

/// How `DSPAUT`/`WRKAUT` show a set's data authority: the name when the
/// data bits are exactly a combination, else `USER DEF`.
#[must_use]
pub fn data_name(a: Auth) -> &'static str {
    let d = DataAuth::of(a);
    if d.auth().0 == a.0 & DATA {
        d.name()
    } else {
        "USER DEF"
    }
}

/// The object authorities of a set.
#[must_use]
pub fn objects(a: Auth) -> Auth {
    Auth(a.0 & OBJECT)
}

/// Replace a set's data authorities, keeping its object authorities.
#[must_use]
pub fn set_data(a: Auth, d: DataAuth) -> Auth {
    d.with_objects(a)
}

/// The mode bits shown for an object: owner, group and `*PUBLIC` data
/// authorities, and the set-ID and restricted-rename bits. `*PUBLIC`
/// taken from an authorization list shows as no access.
#[must_use]
pub fn mode_of(m: &Meta) -> u32 {
    let tri = |a: Auth| u32::from(DataAuth::of(a).0);
    let group = if m.group.is_some() {
        tri(m.group_auth)
    } else {
        0
    };
    let public = if m.public_from_autl { 0 } else { tri(m.public) };
    let mut mode = tri(m.owner_auth) << 6 | group << 3 | public;
    if m.flags.has(Flags::SETUID) {
        mode |= 0o4000;
    }
    if m.flags.has(Flags::SETGID) {
        mode |= 0o2000;
    }
    if m.flags.has(Flags::RSTDRNMUNL) {
        mode |= 0o1000;
    }
    mode
}

/// Apply `chmod` mode bits: each triple replaces that party's data
/// authorities, object authorities stay; group bits are kept for a later
/// primary group when there is none.
pub fn apply_mode(m: &mut Meta, mode: u32) {
    m.owner_auth = set_data(m.owner_auth, DataAuth::from_mode_bits(mode >> 6));
    // Kept without a group: it is what a later group gets.
    m.group_auth = set_data(m.group_auth, DataAuth::from_mode_bits(mode >> 3));
    m.public = set_data(m.public, DataAuth::from_mode_bits(mode));
    m.public_from_autl = false;
    m.flags.set(Flags::SETUID, mode & 0o4000 != 0);
    m.flags.set(Flags::SETGID, mode & 0o2000 != 0);
    m.flags.set(Flags::RSTDRNMUNL, mode & 0o1000 != 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ibm_mapping() {
        assert_eq!(R, Auth(A::OBJOPR | A::READ));
        assert_eq!(RWX, Auth::CHANGE);
        assert_eq!(RX, Auth::USE);
        assert_eq!(DataAuth::of(Auth::ALL).name(), "*RWX");
        assert_eq!(DataAuth::of(Auth::EXCLUDE).name(), "*EXCLUDE");
        // *READ without *OBJOPR is not *R.
        assert_eq!(DataAuth::of(Auth(A::READ)).name(), "*EXCLUDE");
        assert_eq!(data_name(Auth(A::OBJOPR | A::READ | A::ADD)), "USER DEF");
        assert_eq!(data_name(Auth::ALL), "*RWX");
        assert_eq!(objects(Auth::ALL), Auth(OBJECT));
        assert_eq!(DataAuth::parse("*wx").unwrap().auth(), WX);
        for n in ["*RWX", "*RW", "*RX", "*R", "*WX", "*W", "*X", "*EXCLUDE"] {
            assert_eq!(DataAuth::parse(n).unwrap().name(), n);
        }
    }

    #[test]
    fn mode_view() {
        let mut m = Meta::new("BOB", 1208, 0);
        m.group = Some("DEV".into());
        m.group_auth = RX;
        m.public = R;
        assert_eq!(mode_of(&m), 0o754);
        apply_mode(&mut m, 0o1640);
        assert_eq!(mode_of(&m), 0o1640);
        assert!(m.owner_auth.has(Auth(OBJECT)), "object authorities stay");
        m.group = None;
        apply_mode(&mut m, 0o777);
        assert_eq!(mode_of(&m), 0o707);
    }
}
