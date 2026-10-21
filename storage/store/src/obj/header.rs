// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The object header page. See `docs/STORE_FORMAT.md` §8.

use qsname::Name;
use release::Release;

use crate::alloc::Run;
use crate::bytes::{get_u16, get_u32, get_u64, put_u16, put_u32, put_u64, u16_of};
use crate::obj::ObjectId;
use crate::page::BODY_SIZE;
use crate::page::pager::PageNo;
use crate::spec::{Layout, field};
use crate::{Result, StoreError};

/// Magic at offset 0.
pub const MAGIC: &[u8; 8] = b"QS400OBJ";
/// Header version this code writes and reads.
pub const HEADER_VERSION: u32 = 1;
/// Bytes of the fixed part.
pub const FIXED_LEN: usize = 1024;
/// Body offset of the type-specific attribute area.
pub const ATTR_OFF: usize = 1024;
/// Bytes of the attribute area.
pub const ATTR_LEN: usize = 1024;
/// Inline private authority slots.
pub const PRIVATE_SLOTS: usize = 16;
/// Inline extent slots.
pub const EXTENT_SLOTS: usize = 16;
/// Extents one overflow page holds.
pub const OVERFLOW_SLOTS: usize = (BODY_SIZE - 16) / 16;

const OFF_MAGIC: usize = 0;
const OFF_VERSION: usize = 8;
const OFF_FLAGS: usize = 12;
const OFF_ID: usize = 16;
const OFF_LIBRARY: usize = 32;
const OFF_NAME: usize = 48;
const OFF_ATTRIBUTE: usize = 58;
const OFF_TEXT: usize = 68;
const OFF_OWNER: usize = 118;
const OFF_GROUP: usize = 128;
const OFF_CREATOR: usize = 138;
const OFF_SYSTEM: usize = 148;
const OFF_RELEASE: usize = 156;
const OFF_CREATED: usize = 160;
const OFF_CHANGED: usize = 168;
const OFF_SAVED: usize = 176;
const OFF_RESTORED: usize = 184;
const OFF_USED: usize = 192;
const OFF_RESET: usize = 200;
const OFF_DAYS_USED: usize = 208;
const OFF_PUBLIC: usize = 212;
const OFF_AUTL: usize = 216;
const OFF_PRIVATE_COUNT: usize = 226;
const OFF_PRIVATE: usize = 228;
const OFF_PRIVATE_OVERFLOW: usize = 484;
const OFF_SIZE: usize = 492;
const OFF_EXTENT_COUNT: usize = 500;
const OFF_DOMAIN: usize = 502;
const OFF_STATE: usize = 503;
const OFF_DAMAGED: usize = 504;
const OFF_TEMPORARY: usize = 505;
const OFF_COMPRESSION: usize = 506;
const OFF_EXTENTS: usize = 508;
const OFF_EXTENT_OVERFLOW: usize = 764;
const OFF_AUDIT: usize = 772;
/// Body offset of the tree root slots.
pub const OFF_TREE_ROOTS: usize = 784;
const OFF_AUTH_FLAGS: usize = 816;
const OFF_OWNER_AUTH: usize = 820;
const OFF_GROUP_AUTH: usize = 824;
const OFF_AUTH_GEN: usize = 832;
/// `auth_flags`: the owner's authority was reduced from `*ALL`.
const OWNER_REDUCED: u32 = 1;

/// Tree roots any type may use; RCLSTG walks and delete frees them.
pub const TREE_ROOT_SLOTS: usize = 4;

/// The spec layout of the fixed part.
pub static LAYOUT: Layout = Layout {
    name: "ObjectHeader",
    size: Some(FIXED_LEN),
    fields: &[
        field("magic", OFF_MAGIC, 8),
        field("header_version", OFF_VERSION, 4),
        field("flags", OFF_FLAGS, 4),
        field("id", OFF_ID, 16),
        field("library", OFF_LIBRARY, 16),
        field("name", OFF_NAME, 10),
        field("attribute", OFF_ATTRIBUTE, 10),
        field("text", OFF_TEXT, 50),
        field("owner", OFF_OWNER, 10),
        field("primary_group", OFF_GROUP, 10),
        field("creator", OFF_CREATOR, 10),
        field("system", OFF_SYSTEM, 8),
        field("release_v", OFF_RELEASE, 1),
        field("release_r", OFF_RELEASE + 1, 1),
        field("release_m", OFF_RELEASE + 2, 1),
        field("reserved2", OFF_RELEASE + 3, 1),
        field("created", OFF_CREATED, 8),
        field("changed", OFF_CHANGED, 8),
        field("last_saved", OFF_SAVED, 8),
        field("last_restored", OFF_RESTORED, 8),
        field("last_used", OFF_USED, 8),
        field("reset_date", OFF_RESET, 8),
        field("days_used", OFF_DAYS_USED, 4),
        field("public_authority", OFF_PUBLIC, 4),
        field("autl", OFF_AUTL, 10),
        field("private_count", OFF_PRIVATE_COUNT, 2),
        field("private_auths", OFF_PRIVATE, 256),
        field("private_overflow", OFF_PRIVATE_OVERFLOW, 8),
        field("size_pages", OFF_SIZE, 8),
        field("extent_count", OFF_EXTENT_COUNT, 2),
        field("domain", OFF_DOMAIN, 1),
        field("state", OFF_STATE, 1),
        field("damaged", OFF_DAMAGED, 1),
        field("temporary", OFF_TEMPORARY, 1),
        field("compression", OFF_COMPRESSION, 1),
        field("reserved3", OFF_COMPRESSION + 1, 1),
        field("extents", OFF_EXTENTS, 256),
        field("extent_overflow", OFF_EXTENT_OVERFLOW, 8),
        field("audit", OFF_AUDIT, 10),
        field("reserved4", 782, 2),
        field("tree_roots", OFF_TREE_ROOTS, 32),
        field("auth_flags", OFF_AUTH_FLAGS, 4),
        field("owner_authority", OFF_OWNER_AUTH, 4),
        field("group_authority", OFF_GROUP_AUTH, 4),
        field("reserved5", 828, 4),
        field("auth_gen", OFF_AUTH_GEN, 8),
        field("reserved6", 840, 184),
    ],
};

/// The spec layout of a private authority slot.
pub static PRIVATE_AUTH_LAYOUT: Layout = Layout {
    name: "PrivateAuth",
    size: Some(16),
    fields: &[
        field("profile", 0, 10),
        field("bits", 10, 4),
        field("reserved", 14, 2),
    ],
};

/// The spec layout of an extent reference.
pub static EXTENT_REF_LAYOUT: Layout = Layout {
    name: "ExtentRef",
    size: Some(16),
    fields: &[field("start", 0, 8), field("len", 8, 8)],
};

/// The spec layout of the overflow page header.
pub static OVERFLOW_HEADER_LAYOUT: Layout = Layout {
    name: "ExtentOverflowHeader",
    size: Some(16),
    fields: &[field("count", 0, 2), field("reserved", 2, 14)],
};

/// Authority bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Authority(pub u32);

impl Authority {
    /// `*OBJOPR`.
    pub const OBJOPR: u32 = 0x0001;
    /// `*OBJMGT`.
    pub const OBJMGT: u32 = 0x0002;
    /// `*OBJEXIST`.
    pub const OBJEXIST: u32 = 0x0004;
    /// `*OBJALTER`.
    pub const OBJALTER: u32 = 0x0008;
    /// `*OBJREF`.
    pub const OBJREF: u32 = 0x0010;
    /// `*READ`.
    pub const READ: u32 = 0x0020;
    /// `*ADD`.
    pub const ADD: u32 = 0x0040;
    /// `*UPD`.
    pub const UPD: u32 = 0x0080;
    /// `*DLT`.
    pub const DLT: u32 = 0x0100;
    /// `*EXECUTE`.
    pub const EXECUTE: u32 = 0x0200;
    /// The public authority comes from the authorization list.
    pub const FROM_AUTL: u32 = 0x8000_0000;

    /// `*EXCLUDE`.
    pub const EXCLUDE: Self = Self(0);
    /// `*USE`: `*OBJOPR *READ *EXECUTE`.
    pub const USE: Self = Self(Self::OBJOPR | Self::READ | Self::EXECUTE);
    /// `*CHANGE`: `*OBJOPR` and all data authorities.
    pub const CHANGE: Self =
        Self(Self::OBJOPR | Self::READ | Self::ADD | Self::UPD | Self::DLT | Self::EXECUTE);
    /// `*ALL`.
    pub const ALL: Self = Self(0x03FF);

    /// The special value name, or the bits in hex when not a special value.
    #[must_use]
    pub fn display(self) -> String {
        match self {
            Self::EXCLUDE => "*EXCLUDE".into(),
            Self::USE => "*USE".into(),
            Self::CHANGE => "*CHANGE".into(),
            Self::ALL => "*ALL".into(),
            Self(b) if b & Self::FROM_AUTL != 0 => "*AUTL".into(),
            Self(b) => format!("{b:#06x}"),
        }
    }
}

/// A private authority entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivateAuth {
    /// User profile.
    pub profile: [u8; 10],
    /// Authority bits.
    pub bits: Authority,
}

/// `*SYSTEM` or `*USER` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    /// `*SYSTEM`.
    System,
    /// `*USER`.
    User,
}

/// Everything in the fixed part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectHeader {
    /// The object.
    pub id: ObjectId,
    /// Its library.
    pub library: ObjectId,
    /// Its name.
    pub name: Name,
    /// Extended attribute, CP037, blank for none.
    pub attribute: [u8; 10],
    /// Text description, CP037.
    pub text: [u8; 50],
    /// Owner profile.
    pub owner: [u8; 10],
    /// Primary group, blank for none.
    pub primary_group: [u8; 10],
    /// Creator profile.
    pub creator: [u8; 10],
    /// Created-on system name.
    pub system: [u8; 8],
    /// Creating release.
    pub release: Release,
    /// Unix seconds.
    pub created: u64,
    /// Unix seconds.
    pub changed: u64,
    /// Unix seconds, 0 for never.
    pub last_saved: u64,
    /// Unix seconds, 0 for never.
    pub last_restored: u64,
    /// Date as Unix seconds.
    pub last_used: u64,
    /// Date as Unix seconds.
    pub reset_date: u64,
    /// Days used.
    pub days_used: u32,
    /// Public authority.
    pub public_authority: Authority,
    /// Authorization list, blank for none.
    pub autl: [u8; 10],
    /// Inline private authorities.
    pub private_auths: Vec<PrivateAuth>,
    /// Overflow page for more, if any.
    pub private_overflow: Option<PageNo>,
    /// Pages in the extents.
    pub size_pages: u64,
    /// Domain.
    pub domain: Domain,
    /// State: false system, true user.
    pub user_state: bool,
    /// Marked by RCLSTG or a checksum failure.
    pub damaged: bool,
    /// Lives in the temporary ASP.
    pub temporary: bool,
    /// Inline extents.
    pub extents: Vec<Run>,
    /// Overflow page for more, if any.
    pub extent_overflow: Option<PageNo>,
    /// Object auditing value.
    pub audit: [u8; 10],
    /// Roots of B+trees the payload owns, `None` for empty or unused.
    pub tree_roots: [Option<PageNo>; TREE_ROOT_SLOTS],
    /// The owner's authority, `*ALL` unless reduced.
    pub owner_authority: Authority,
    /// The primary group's authority.
    pub group_authority: Authority,
    /// Bumped by every authority change; cached decisions compare it.
    pub auth_gen: u64,
}

/// Blank CP037 text of a fixed size.
#[must_use]
pub fn blank<const N: usize>() -> [u8; N] {
    [ccsid::SPACE; N]
}

/// Encode `s` into a fixed CP037 field, blank padded.
pub fn text<const N: usize>(s: &str) -> Result<[u8; N]> {
    let mut out = [ccsid::SPACE; N];
    ccsid::encode_fixed(s, &mut out).map_err(|_| StoreError::InvalidName { name: s.into() })?;
    Ok(out)
}

/// Decode a fixed CP037 field.
#[must_use]
pub fn text_of(field: &[u8]) -> String {
    ccsid::decode_trimmed(field)
}

fn opt(v: u64) -> Option<PageNo> {
    (v != 0).then_some(v)
}

impl ObjectHeader {
    /// A fresh header with every optional field blank or zero.
    #[must_use]
    pub fn new(id: ObjectId, library: ObjectId, name: Name, now: u64) -> Self {
        Self {
            id,
            library,
            name,
            attribute: blank(),
            text: blank(),
            owner: blank(),
            primary_group: blank(),
            creator: blank(),
            system: blank(),
            release: release::RELEASE,
            created: now,
            changed: now,
            last_saved: 0,
            last_restored: 0,
            last_used: 0,
            reset_date: 0,
            days_used: 0,
            public_authority: Authority::CHANGE,
            autl: blank(),
            private_auths: Vec::new(),
            private_overflow: None,
            size_pages: 0,
            domain: Domain::User,
            user_state: true,
            damaged: false,
            temporary: false,
            extents: Vec::new(),
            extent_overflow: None,
            audit: blank(),
            tree_roots: [None; TREE_ROOT_SLOTS],
            owner_authority: Authority::ALL,
            group_authority: Authority::EXCLUDE,
            auth_gen: 0,
        }
    }

    /// Encode the fixed part into a page body (bytes 0..1024). The
    /// attribute area is left alone.
    pub fn encode(&self, body: &mut [u8]) {
        assert!(self.private_auths.len() <= PRIVATE_SLOTS && self.extents.len() <= EXTENT_SLOTS);
        let b = &mut body[..FIXED_LEN];
        b.fill(0);
        b[OFF_MAGIC..OFF_MAGIC + 8].copy_from_slice(MAGIC);
        put_u32(b, OFF_VERSION, HEADER_VERSION);
        b[OFF_ID..OFF_ID + 16].copy_from_slice(&self.id.to_bytes());
        b[OFF_LIBRARY..OFF_LIBRARY + 16].copy_from_slice(&self.library.to_bytes());
        b[OFF_NAME..OFF_NAME + 10].copy_from_slice(self.name.as_bytes());
        b[OFF_ATTRIBUTE..OFF_ATTRIBUTE + 10].copy_from_slice(&self.attribute);
        b[OFF_TEXT..OFF_TEXT + 50].copy_from_slice(&self.text);
        b[OFF_OWNER..OFF_OWNER + 10].copy_from_slice(&self.owner);
        b[OFF_GROUP..OFF_GROUP + 10].copy_from_slice(&self.primary_group);
        b[OFF_CREATOR..OFF_CREATOR + 10].copy_from_slice(&self.creator);
        b[OFF_SYSTEM..OFF_SYSTEM + 8].copy_from_slice(&self.system);
        b[OFF_RELEASE] = self.release.v;
        b[OFF_RELEASE + 1] = self.release.r;
        b[OFF_RELEASE + 2] = self.release.m;
        put_u64(b, OFF_CREATED, self.created);
        put_u64(b, OFF_CHANGED, self.changed);
        put_u64(b, OFF_SAVED, self.last_saved);
        put_u64(b, OFF_RESTORED, self.last_restored);
        put_u64(b, OFF_USED, self.last_used);
        put_u64(b, OFF_RESET, self.reset_date);
        put_u32(b, OFF_DAYS_USED, self.days_used);
        put_u32(b, OFF_PUBLIC, self.public_authority.0);
        b[OFF_AUTL..OFF_AUTL + 10].copy_from_slice(&self.autl);
        put_u16(b, OFF_PRIVATE_COUNT, u16_of(self.private_auths.len()));
        for (i, p) in self.private_auths.iter().enumerate() {
            let at = OFF_PRIVATE + i * 16;
            b[at..at + 10].copy_from_slice(&p.profile);
            put_u32(b, at + 10, p.bits.0);
        }
        put_u64(b, OFF_PRIVATE_OVERFLOW, self.private_overflow.unwrap_or(0));
        put_u64(b, OFF_SIZE, self.size_pages);
        put_u16(b, OFF_EXTENT_COUNT, u16_of(self.extents.len()));
        b[OFF_DOMAIN] = match self.domain {
            Domain::System => 0,
            Domain::User => 1,
        };
        b[OFF_STATE] = u8::from(self.user_state);
        b[OFF_DAMAGED] = u8::from(self.damaged);
        b[OFF_TEMPORARY] = u8::from(self.temporary);
        for (i, e) in self.extents.iter().enumerate() {
            let at = OFF_EXTENTS + i * 16;
            put_u64(b, at, e.start);
            put_u64(b, at + 8, e.len);
        }
        put_u64(b, OFF_EXTENT_OVERFLOW, self.extent_overflow.unwrap_or(0));
        b[OFF_AUDIT..OFF_AUDIT + 10].copy_from_slice(&self.audit);
        for (i, r) in self.tree_roots.iter().enumerate() {
            put_u64(b, OFF_TREE_ROOTS + i * 8, r.unwrap_or(0));
        }
        if self.owner_authority != Authority::ALL {
            put_u32(b, OFF_AUTH_FLAGS, OWNER_REDUCED);
            put_u32(b, OFF_OWNER_AUTH, self.owner_authority.0);
        }
        put_u32(b, OFF_GROUP_AUTH, self.group_authority.0);
        put_u64(b, OFF_AUTH_GEN, self.auth_gen);
    }

    /// Decode the fixed part of a page body. `page` is for errors.
    pub fn decode(body: &[u8], page: PageNo) -> Result<Self> {
        let corrupt = |what: &str| StoreError::Corrupt {
            page,
            what: format!("object header: {what}"),
        };
        if &body[OFF_MAGIC..OFF_MAGIC + 8] != MAGIC {
            return Err(corrupt("bad magic"));
        }
        if get_u32(body, OFF_VERSION) != HEADER_VERSION {
            return Err(corrupt("unknown version"));
        }
        let private_count = usize::from(get_u16(body, OFF_PRIVATE_COUNT));
        let extent_count = usize::from(get_u16(body, OFF_EXTENT_COUNT));
        if private_count > PRIVATE_SLOTS || extent_count > EXTENT_SLOTS {
            return Err(corrupt("slot count"));
        }
        let private_auths = (0..private_count)
            .map(|i| {
                let at = OFF_PRIVATE + i * 16;
                PrivateAuth {
                    profile: body[at..at + 10].try_into().expect("10"),
                    bits: Authority(get_u32(body, at + 10)),
                }
            })
            .collect();
        let extents = (0..extent_count)
            .map(|i| {
                let at = OFF_EXTENTS + i * 16;
                Run {
                    start: get_u64(body, at),
                    len: get_u64(body, at + 8),
                }
            })
            .collect();
        Ok(Self {
            id: ObjectId::from_bytes(&body[OFF_ID..OFF_ID + 16]),
            library: ObjectId::from_bytes(&body[OFF_LIBRARY..OFF_LIBRARY + 16]),
            name: Name::from_bytes(body[OFF_NAME..OFF_NAME + 10].try_into().expect("10")),
            attribute: body[OFF_ATTRIBUTE..OFF_ATTRIBUTE + 10]
                .try_into()
                .expect("10"),
            text: body[OFF_TEXT..OFF_TEXT + 50].try_into().expect("50"),
            owner: body[OFF_OWNER..OFF_OWNER + 10].try_into().expect("10"),
            primary_group: body[OFF_GROUP..OFF_GROUP + 10].try_into().expect("10"),
            creator: body[OFF_CREATOR..OFF_CREATOR + 10].try_into().expect("10"),
            system: body[OFF_SYSTEM..OFF_SYSTEM + 8].try_into().expect("8"),
            release: Release {
                v: body[OFF_RELEASE],
                r: body[OFF_RELEASE + 1],
                m: body[OFF_RELEASE + 2],
            },
            created: get_u64(body, OFF_CREATED),
            changed: get_u64(body, OFF_CHANGED),
            last_saved: get_u64(body, OFF_SAVED),
            last_restored: get_u64(body, OFF_RESTORED),
            last_used: get_u64(body, OFF_USED),
            reset_date: get_u64(body, OFF_RESET),
            days_used: get_u32(body, OFF_DAYS_USED),
            public_authority: Authority(get_u32(body, OFF_PUBLIC)),
            autl: body[OFF_AUTL..OFF_AUTL + 10].try_into().expect("10"),
            private_auths,
            private_overflow: opt(get_u64(body, OFF_PRIVATE_OVERFLOW)),
            size_pages: get_u64(body, OFF_SIZE),
            domain: if body[OFF_DOMAIN] == 0 {
                Domain::System
            } else {
                Domain::User
            },
            user_state: body[OFF_STATE] != 0,
            damaged: body[OFF_DAMAGED] != 0,
            temporary: body[OFF_TEMPORARY] != 0,
            extents,
            extent_overflow: opt(get_u64(body, OFF_EXTENT_OVERFLOW)),
            audit: body[OFF_AUDIT..OFF_AUDIT + 10].try_into().expect("10"),
            tree_roots: std::array::from_fn(|i| opt(get_u64(body, OFF_TREE_ROOTS + i * 8))),
            owner_authority: if get_u32(body, OFF_AUTH_FLAGS) & OWNER_REDUCED == 0 {
                Authority::ALL
            } else {
                Authority(get_u32(body, OFF_OWNER_AUTH))
            },
            group_authority: Authority(get_u32(body, OFF_GROUP_AUTH)),
            auth_gen: get_u64(body, OFF_AUTH_GEN),
        })
    }
}

/// Encode an overflow page body from extents (at most `OVERFLOW_SLOTS`).
pub fn encode_overflow(body: &mut [u8], extents: &[Run]) {
    assert!(extents.len() <= OVERFLOW_SLOTS);
    body.fill(0);
    put_u16(body, 0, u16_of(extents.len()));
    for (i, e) in extents.iter().enumerate() {
        let at = 16 + i * 16;
        put_u64(body, at, e.start);
        put_u64(body, at + 8, e.len);
    }
}

/// Decode an overflow page body.
pub fn decode_overflow(body: &[u8], page: PageNo) -> Result<Vec<Run>> {
    let count = usize::from(get_u16(body, 0));
    if count > OVERFLOW_SLOTS {
        return Err(StoreError::Corrupt {
            page,
            what: format!("extent overflow with {count} entries"),
        });
    }
    Ok((0..count)
        .map(|i| {
            let at = 16 + i * 16;
            Run {
                start: get_u64(body, at),
                len: get_u64(body, at + 8),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let id = ObjectId {
            asp: 1,
            type_code: 2,
            serial: 7,
            generation: 0,
        };
        let mut h = ObjectHeader::new(id, ObjectId::NONE, Name::parse("MYDTA").unwrap(), 1000);
        h.text = text("A data area").unwrap();
        h.owner = text("QSECOFR").unwrap();
        h.private_auths.push(PrivateAuth {
            profile: text("BOB").unwrap(),
            bits: Authority::USE,
        });
        h.extents.push(Run { start: 500, len: 3 });
        h.size_pages = 3;
        h.extent_overflow = Some(900);
        h.owner_authority = Authority::USE;
        h.auth_gen = 5;
        let mut body = vec![0u8; BODY_SIZE];
        h.encode(&mut body);
        let back = ObjectHeader::decode(&body, 1).unwrap();
        assert_eq!(back, h);
        assert_eq!(text_of(&back.text), "A data area");
        assert_eq!(back.public_authority.display(), "*CHANGE");
        assert_eq!(Authority::ALL.display(), "*ALL");
        body[0] = 0;
        assert!(matches!(
            ObjectHeader::decode(&body, 1),
            Err(StoreError::Corrupt { .. })
        ));
        let mut ob = vec![0u8; BODY_SIZE];
        encode_overflow(&mut ob, &[Run { start: 1, len: 2 }]);
        assert_eq!(
            decode_overflow(&ob, 2).unwrap(),
            vec![Run { start: 1, len: 2 }]
        );
    }
}
