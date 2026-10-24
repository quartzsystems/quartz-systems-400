// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The record's bytes: `QM`, a version byte, then fields as tag (u8),
//! length (u16 little-endian) and value. Integers are little-endian,
//! text is UTF-8. A reader skips tags it does not know, so later
//! versions add fields without breaking older readers; the owner is
//! the one required field. `docs/IFS.md` lists the tags.

use sec::auth::bits::Auth;

use super::{CheckOut, CrtObjAud, Flags, Journal, Meta, ObjAudit, Scan, StgOpt};

/// The magic.
pub const MAGIC: &[u8; 2] = b"QM";
/// The version this code writes.
pub const VERSION: u8 = 1;

/// Field tags.
pub mod tag {
    /// Owner profile.
    pub const OWNER: u8 = 1;
    /// Owner authority.
    pub const OWNER_AUTH: u8 = 2;
    /// Primary group.
    pub const GROUP: u8 = 3;
    /// Primary group authority.
    pub const GROUP_AUTH: u8 = 4;
    /// `*PUBLIC`.
    pub const PUBLIC: u8 = 5;
    /// `*PUBLIC` from the list.
    pub const PUBLIC_AUTL: u8 = 6;
    /// One private authority (repeated).
    pub const PRIVATE: u8 = 7;
    /// Authorization list.
    pub const AUTL: u8 = 8;
    /// CCSID.
    pub const CCSID: u8 = 9;
    /// Code page.
    pub const CODEPAGE: u8 = 10;
    /// Flags.
    pub const FLAGS: u8 = 11;
    /// Disk and main storage options.
    pub const STG: u8 = 12;
    /// Scanning.
    pub const SCAN: u8 = 13;
    /// Auditing.
    pub const AUDIT: u8 = 14;
    /// `*CRTOBJAUD`.
    pub const CRTOBJAUD: u8 = 15;
    /// Check-out.
    pub const CHECKOUT: u8 = 16;
    /// Journaling.
    pub const JOURNAL: u8 = 17;
    /// Creation time.
    pub const CREATED: u8 = 18;
    /// Generation.
    pub const GENERATION: u8 = 19;
    /// Text.
    pub const TEXT: u8 = 20;
}

/// Why a record would not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// Not a record.
    #[error("not a metadata record")]
    Magic,
    /// A version this code cannot read.
    #[error("metadata version {0} is newer than this code")]
    Version(u8),
    /// Cut short or a field of the wrong size.
    #[error("metadata record is malformed")]
    Malformed,
    /// No owner.
    #[error("metadata record has no owner")]
    NoOwner,
}

struct W(Vec<u8>);

impl W {
    fn field(&mut self, tag: u8, v: &[u8]) {
        let len = u16::try_from(v.len()).expect("field under 64K");
        self.0.push(tag);
        self.0.extend_from_slice(&len.to_le_bytes());
        self.0.extend_from_slice(v);
    }
}

fn s(v: &str) -> &[u8] {
    v.as_bytes()
}

/// Encode a record.
#[must_use]
pub fn encode(m: &Meta) -> Vec<u8> {
    let mut w = W(Vec::with_capacity(128));
    w.0.extend_from_slice(MAGIC);
    w.0.push(VERSION);
    w.field(tag::OWNER, s(&m.owner));
    w.field(tag::OWNER_AUTH, &m.owner_auth.0.to_le_bytes());
    if let Some(g) = &m.group {
        w.field(tag::GROUP, s(g));
    }
    w.field(tag::GROUP_AUTH, &m.group_auth.0.to_le_bytes());
    w.field(tag::PUBLIC, &m.public.0.to_le_bytes());
    if m.public_from_autl {
        w.field(tag::PUBLIC_AUTL, &[1]);
    }
    for (who, a) in &m.private {
        let mut v = a.0.to_le_bytes().to_vec();
        v.extend_from_slice(who.as_bytes());
        w.field(tag::PRIVATE, &v);
    }
    if let Some(l) = &m.autl {
        w.field(tag::AUTL, s(l));
    }
    w.field(tag::CCSID, &m.ccsid.to_le_bytes());
    w.field(tag::CODEPAGE, &m.codepage.to_le_bytes());
    w.field(tag::FLAGS, &m.flags.0.to_le_bytes());
    w.field(tag::STG, &[m.disk_stg as u8, m.main_stg as u8]);
    w.field(tag::SCAN, &[m.crtobjscan as u8, m.scan as u8]);
    w.field(tag::AUDIT, &[m.audit as u8]);
    w.field(
        tag::CRTOBJAUD,
        &[match m.crtobjaud {
            CrtObjAud::SysVal => 0xFF,
            CrtObjAud::Value(a) => a as u8,
        }],
    );
    if let Some(c) = &m.checkout {
        let mut v = c.time.to_le_bytes().to_vec();
        v.extend_from_slice(c.user.as_bytes());
        w.field(tag::CHECKOUT, &v);
    }
    if let Some(j) = &m.journal {
        let mut v = j.start.to_le_bytes().to_vec();
        v.push(j.images);
        v.push(u8::from(j.omit_open_close));
        v.push(u8::from(j.inherit));
        v.extend_from_slice(&j.jid);
        v.extend_from_slice(j.journal.as_bytes());
        w.field(tag::JOURNAL, &v);
    }
    w.field(tag::CREATED, &m.created.to_le_bytes());
    w.field(tag::GENERATION, &m.generation.to_le_bytes());
    if !m.text.is_empty() {
        w.field(tag::TEXT, s(&m.text));
    }
    w.0
}

fn arr<const N: usize>(v: &[u8]) -> Result<[u8; N], DecodeError> {
    v.try_into().map_err(|_| DecodeError::Malformed)
}

fn text(v: &[u8]) -> Result<String, DecodeError> {
    String::from_utf8(v.to_vec()).map_err(|_| DecodeError::Malformed)
}

fn stg(b: u8) -> StgOpt {
    match b {
        1 => StgOpt::Minimize,
        2 => StgOpt::Dynamic,
        _ => StgOpt::Normal,
    }
}

fn scan(b: u8) -> Scan {
    match b {
        0 => Scan::No,
        2 => Scan::ChgOnly,
        _ => Scan::Yes,
    }
}

fn audit(b: u8) -> ObjAudit {
    match b {
        1 => ObjAudit::UsrPrf,
        2 => ObjAudit::Change,
        3 => ObjAudit::All,
        _ => ObjAudit::None,
    }
}

/// Decode a record.
pub fn decode(raw: &[u8]) -> Result<Meta, DecodeError> {
    if raw.len() < 3 || &raw[..2] != MAGIC {
        return Err(DecodeError::Magic);
    }
    if raw[2] > VERSION {
        return Err(DecodeError::Version(raw[2]));
    }
    let mut m = Meta::new("", 0, 0);
    m.flags = Flags(0);
    let mut owner = false;
    let mut at = 3;
    while at < raw.len() {
        if at + 3 > raw.len() {
            return Err(DecodeError::Malformed);
        }
        let t = raw[at];
        let len = usize::from(u16::from_le_bytes([raw[at + 1], raw[at + 2]]));
        at += 3;
        let v = raw.get(at..at + len).ok_or(DecodeError::Malformed)?;
        at += len;
        let u32v = || arr::<4>(v).map(u32::from_le_bytes);
        let u16v = || arr::<2>(v).map(u16::from_le_bytes);
        match t {
            tag::OWNER => {
                m.owner = text(v)?;
                owner = !m.owner.is_empty();
            }
            tag::OWNER_AUTH => m.owner_auth = Auth(u32v()?),
            tag::GROUP => m.group = Some(text(v)?),
            tag::GROUP_AUTH => m.group_auth = Auth(u32v()?),
            tag::PUBLIC => m.public = Auth(u32v()?),
            tag::PUBLIC_AUTL => m.public_from_autl = v.first() == Some(&1),
            tag::PRIVATE => {
                if v.len() < 4 {
                    return Err(DecodeError::Malformed);
                }
                let a = Auth(u32::from_le_bytes(arr(&v[..4])?));
                m.private.push((text(&v[4..])?, a));
            }
            tag::AUTL => m.autl = Some(text(v)?),
            tag::CCSID => m.ccsid = u16v()?,
            tag::CODEPAGE => m.codepage = u16v()?,
            tag::FLAGS => m.flags = Flags(u32v()?),
            tag::STG => {
                let [d, mm] = arr::<2>(v)?;
                m.disk_stg = stg(d);
                m.main_stg = stg(mm);
            }
            tag::SCAN => {
                let [c, sc] = arr::<2>(v)?;
                m.crtobjscan = scan(c);
                m.scan = scan(sc);
            }
            tag::AUDIT => m.audit = audit(arr::<1>(v)?[0]),
            tag::CRTOBJAUD => {
                let b = arr::<1>(v)?[0];
                m.crtobjaud = if b == 0xFF {
                    CrtObjAud::SysVal
                } else {
                    CrtObjAud::Value(audit(b))
                };
            }
            tag::CHECKOUT => {
                if v.len() < 8 {
                    return Err(DecodeError::Malformed);
                }
                m.checkout = Some(CheckOut {
                    time: i64::from_le_bytes(arr(&v[..8])?),
                    user: text(&v[8..])?,
                });
            }
            tag::JOURNAL => {
                if v.len() < 21 {
                    return Err(DecodeError::Malformed);
                }
                m.journal = Some(Journal {
                    start: i64::from_le_bytes(arr(&v[..8])?),
                    images: v[8],
                    omit_open_close: v[9] != 0,
                    inherit: v[10] != 0,
                    jid: arr(&v[11..21])?,
                    journal: text(&v[21..])?,
                });
            }
            tag::CREATED => m.created = i64::from_le_bytes(arr(v)?),
            tag::GENERATION => m.generation = u32v()?,
            tag::TEXT => m.text = text(v)?,
            _ => {}
        }
    }
    if !owner {
        return Err(DecodeError::NoOwner);
    }
    m.private.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Meta {
        let mut m = Meta::new("BOB", 1208, 1_700_000_000_000_000_000);
        m.group = Some("DEV".into());
        m.group_auth = Auth::USE;
        m.public = Auth::EXCLUDE;
        m.public_from_autl = true;
        m.set_private("ANN", Some(Auth::CHANGE));
        m.set_private("ZED", Some(Auth::EXCLUDE));
        m.autl = Some("PAYROLL".into());
        m.flags.set(Flags::HIDDEN, true);
        m.disk_stg = StgOpt::Dynamic;
        m.main_stg = StgOpt::Minimize;
        m.crtobjscan = Scan::ChgOnly;
        m.scan = Scan::No;
        m.audit = ObjAudit::All;
        m.crtobjaud = CrtObjAud::Value(ObjAudit::Change);
        m.checkout = Some(CheckOut {
            user: "ANN".into(),
            time: 42,
        });
        m.journal = Some(Journal {
            journal: "QGPL/J".into(),
            images: 1,
            omit_open_close: true,
            inherit: true,
            start: 7,
            jid: *b"0123456789",
        });
        m.generation = 9;
        m.text = "Payroll data".into();
        m
    }

    #[test]
    fn round_trip() {
        let m = full();
        assert_eq!(decode(&encode(&m)).unwrap(), m);
        let plain = Meta::new("QSYS", 37, 0);
        assert_eq!(decode(&encode(&plain)).unwrap(), plain);
    }

    #[test]
    fn unknown_tags_are_skipped() {
        let mut raw = encode(&full());
        raw.extend_from_slice(&[200, 3, 0, 1, 2, 3]);
        assert_eq!(decode(&raw).unwrap(), full());
    }

    #[test]
    fn bad_records() {
        assert_eq!(decode(b"XX\x01"), Err(DecodeError::Magic));
        assert_eq!(decode(b"QM\x09"), Err(DecodeError::Version(9)));
        assert_eq!(decode(b"QM\x01"), Err(DecodeError::NoOwner));
        let raw = encode(&full());
        assert_eq!(decode(&raw[..raw.len() - 2]), Err(DecodeError::Malformed));
        assert_eq!(
            decode(b"QM\x01\x09\x01\x00\x01"),
            Err(DecodeError::Malformed)
        );
    }
}
