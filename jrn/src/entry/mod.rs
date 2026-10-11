// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One journal entry: the fixed header every entry shares and the
//! entry-specific data after it. The header mirrors the fields IBM's
//! `QjoRetrieveJournalEntries` returns; the byte layout is ours.

use qsname::Name;
use store::obj::JobHandle;
use store::spec::{Layout, field};

use crate::error::{JrnError, Result};

/// Bytes of the fixed header.
pub const HEADER_LEN: usize = 128;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "EntryHeader",
    size: Some(HEADER_LEN),
    fields: &[
        field("len", 0, 4),
        field("seq", 4, 8),
        field("code", 12, 1),
        field("type", 13, 2),
        field("flags", 15, 1),
        field("time", 16, 8),
        field("micros", 24, 4),
        field("job_number", 28, 4),
        field("job_name", 32, 10),
        field("job_user", 42, 10),
        field("program", 52, 10),
        field("library", 62, 10),
        field("object", 72, 10),
        field("member", 82, 10),
        field("object_type", 92, 2),
        field("reserved", 94, 2),
        field("count", 96, 8),
        field("commit_cycle", 104, 8),
        field("data_len", 112, 4),
        field("reserved2", 116, 12),
    ],
};

/// Journal codes.
pub mod code {
    /// Journal lifecycle (`J`).
    pub const JOURNAL: u8 = b'J';
    /// File level (`F`).
    pub const FILE: u8 = b'F';
    /// Record level (`R`).
    pub const RECORD: u8 = b'R';
    /// Commitment control (`C`).
    pub const COMMIT: u8 = b'C';
    /// User entries (`U`).
    pub const USER: u8 = b'U';
    /// Audit (`T`); nothing emits it yet (`MIGRATE(phase6)`).
    pub const AUDIT: u8 = b'T';
}

/// Flag bits.
pub mod flags {
    /// The entry belongs to a commit cycle.
    pub const COMMITTED: u8 = 1;
    /// The data is a minimized image.
    pub const MINIMIZED: u8 = 2;
    /// The entry was written for a rollback.
    pub const ROLLBACK: u8 = 4;
    /// The entry was written by `APYJRNCHG` or `RMVJRNCHG`.
    pub const APPLIED: u8 = 8;
}

/// The job an entry came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobInfo {
    /// Job name.
    pub name: String,
    /// User profile.
    pub user: String,
    /// Job number.
    pub number: u32,
}

impl JobInfo {
    /// What a job handle says until jobs have names (`MIGRATE(phase5)`).
    #[must_use]
    pub fn from_handle(job: JobHandle) -> Self {
        Self {
            name: "QS400".into(),
            user: "QSECOFR".into(),
            number: u32::try_from(job.0 % 1_000_000).expect("six digits"),
        }
    }

    /// `number/user/name`.
    #[must_use]
    pub fn qualified(&self) -> String {
        format!("{:06}/{}/{}", self.number, self.user, self.name)
    }
}

/// The object an entry is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectRef {
    /// Library.
    pub library: Name,
    /// Object.
    pub object: Name,
    /// Member, when the entry is about one.
    pub member: Option<Name>,
    /// Object type code.
    pub type_code: u16,
}

/// An entry as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Sequence number, journal-wide.
    pub seq: u64,
    /// Journal code.
    pub code: u8,
    /// Entry type, two letters.
    pub kind: [u8; 2],
    /// Flags.
    pub flags: u8,
    /// Unix seconds.
    pub time: u64,
    /// Microseconds within the second.
    pub micros: u32,
    /// Job.
    pub job: JobInfo,
    /// Program.
    pub program: String,
    /// Object.
    pub object: Option<ObjectRef>,
    /// Count or relative record number.
    pub count: u64,
    /// Commit cycle identifier, 0 outside commitment control.
    pub commit_cycle: u64,
    /// Entry-specific data.
    pub data: Vec<u8>,
}

/// What a caller gives the journal; it adds the sequence and time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    /// Journal code.
    pub code: u8,
    /// Entry type.
    pub kind: [u8; 2],
    /// Flags.
    pub flags: u8,
    /// Job.
    pub job: JobInfo,
    /// Program.
    pub program: String,
    /// Object.
    pub object: Option<ObjectRef>,
    /// Count or RRN.
    pub count: u64,
    /// Commit cycle, 0 for none.
    pub commit_cycle: u64,
    /// Data.
    pub data: Vec<u8>,
    /// The time to stamp, or the clock's when `None`.
    pub time: Option<u64>,
}

impl Draft {
    /// An entry of `code` and `kind` from `job`, nothing else said.
    #[must_use]
    pub fn new(code: u8, kind: &str, job: JobHandle) -> Self {
        let k = kind.as_bytes();
        Self {
            code,
            kind: [k[0], k[1]],
            flags: 0,
            job: JobInfo::from_handle(job),
            program: String::new(),
            object: None,
            count: 0,
            commit_cycle: 0,
            data: Vec::new(),
            time: None,
        }
    }

    /// About this object.
    #[must_use]
    pub fn object(mut self, o: ObjectRef) -> Self {
        self.object = Some(o);
        self
    }

    /// With this count or RRN.
    #[must_use]
    pub fn count(mut self, n: u64) -> Self {
        self.count = n;
        self
    }

    /// With this data.
    #[must_use]
    pub fn data(mut self, d: Vec<u8>) -> Self {
        self.data = d;
        self
    }

    /// In this commit cycle.
    #[must_use]
    pub fn cycle(mut self, id: u64) -> Self {
        self.commit_cycle = id;
        if id != 0 {
            self.flags |= flags::COMMITTED;
        }
        self
    }

    /// Stamped with this time.
    #[must_use]
    pub fn at(mut self, time: u64) -> Self {
        self.time = Some(time);
        self
    }

    /// The entry with its sequence and time filled in.
    #[must_use]
    pub fn into_entry(self, seq: u64, time: u64, micros: u32) -> Entry {
        Entry {
            seq,
            code: self.code,
            kind: self.kind,
            flags: self.flags,
            time: self.time.unwrap_or(time),
            micros: if self.time.is_some() { 0 } else { micros },
            job: self.job,
            program: self.program,
            object: self.object,
            count: self.count,
            commit_cycle: self.commit_cycle,
            data: self.data,
        }
    }
}

fn put_text(out: &mut [u8], s: &str) {
    out.fill(ccsid::SPACE);
    for (i, ch) in s.chars().take(out.len()).enumerate() {
        out[i] = ccsid::byte_of(ch).unwrap_or(ccsid::SPACE);
    }
}

fn name_at(b: &[u8]) -> Option<Name> {
    let bytes: [u8; 10] = b[..10].try_into().expect("10");
    (bytes != [ccsid::SPACE; 10]).then(|| Name::from_bytes(bytes))
}

impl Entry {
    /// `R PT` and the like.
    #[must_use]
    pub fn type_name(&self) -> String {
        format!(
            "{} {}{}",
            char::from(self.code),
            char::from(self.kind[0]),
            char::from(self.kind[1])
        )
    }

    /// Whether the entry is of `kind`.
    #[must_use]
    pub fn is(&self, code: u8, kind: &str) -> bool {
        self.code == code && self.kind == kind.as_bytes()[..2]
    }

    /// Total bytes on disk.
    #[must_use]
    pub fn len(&self) -> usize {
        HEADER_LEN + self.data.len()
    }

    /// Never empty: there is always the header.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Encode header and data.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; self.len()];
        b[0..4].copy_from_slice(&u32::try_from(self.len()).expect("fits").to_le_bytes());
        b[4..12].copy_from_slice(&self.seq.to_le_bytes());
        b[12] = self.code;
        b[13..15].copy_from_slice(&self.kind);
        b[15] = self.flags;
        b[16..24].copy_from_slice(&self.time.to_le_bytes());
        b[24..28].copy_from_slice(&self.micros.to_le_bytes());
        b[28..32].copy_from_slice(&self.job.number.to_le_bytes());
        put_text(&mut b[32..42], &self.job.name);
        put_text(&mut b[42..52], &self.job.user);
        put_text(&mut b[52..62], &self.program);
        b[62..92].fill(ccsid::SPACE);
        if let Some(o) = &self.object {
            b[62..72].copy_from_slice(o.library.as_bytes());
            b[72..82].copy_from_slice(o.object.as_bytes());
            if let Some(m) = o.member {
                b[82..92].copy_from_slice(m.as_bytes());
            }
            b[92..94].copy_from_slice(&o.type_code.to_le_bytes());
        }
        b[96..104].copy_from_slice(&self.count.to_le_bytes());
        b[104..112].copy_from_slice(&self.commit_cycle.to_le_bytes());
        b[112..116].copy_from_slice(&u32::try_from(self.data.len()).expect("fits").to_le_bytes());
        b[HEADER_LEN..].copy_from_slice(&self.data);
        b
    }

    /// The entry length a header announces, before decoding the rest.
    pub fn announced_len(header: &[u8]) -> Result<usize> {
        if header.len() < 4 {
            return Err(JrnError::Corrupt {
                what: "entry header cut short".into(),
            });
        }
        let len = u32::from_le_bytes(header[..4].try_into().expect("4"));
        let len = usize::try_from(len).expect("fits");
        if len < HEADER_LEN {
            return Err(JrnError::Corrupt {
                what: format!("entry length {len} below the header"),
            });
        }
        Ok(len)
    }

    /// Decode one entry from `b`, which holds exactly it.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let len = Self::announced_len(b)?;
        if b.len() != len {
            return Err(JrnError::Corrupt {
                what: format!("entry of {} bytes announces {len}", b.len()),
            });
        }
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().expect("4"));
        let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().expect("8"));
        let data_len = usize::try_from(u32_at(112)).expect("fits");
        if HEADER_LEN + data_len != len {
            return Err(JrnError::Corrupt {
                what: "entry data length disagrees with the entry length".into(),
            });
        }
        let object = name_at(&b[72..82]).map(|object| ObjectRef {
            library: name_at(&b[62..72]).unwrap_or(object),
            object,
            member: name_at(&b[82..92]),
            type_code: u16::from_le_bytes([b[92], b[93]]),
        });
        Ok(Self {
            seq: u64_at(4),
            code: b[12],
            kind: [b[13], b[14]],
            flags: b[15],
            time: u64_at(16),
            micros: u32_at(24),
            job: JobInfo {
                number: u32_at(28),
                name: ccsid::decode_trimmed(&b[32..42]),
                user: ccsid::decode_trimmed(&b[42..52]),
            },
            program: ccsid::decode_trimmed(&b[52..62]),
            object,
            count: u64_at(96),
            commit_cycle: u64_at(104),
            data: b[HEADER_LEN..].to_vec(),
        })
    }
}

/// A record image: the entry data of record-level entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordImage {
    /// The record bytes.
    pub record: Vec<u8>,
    /// The null map.
    pub null_map: Vec<u8>,
}

impl RecordImage {
    /// Encode: record length, null map length, both.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(6 + self.record.len() + self.null_map.len());
        b.extend(
            u32::try_from(self.record.len())
                .expect("fits")
                .to_le_bytes(),
        );
        b.extend(
            u16::try_from(self.null_map.len())
                .expect("fits")
                .to_le_bytes(),
        );
        b.extend(&self.record);
        b.extend(&self.null_map);
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let bad = || JrnError::Corrupt {
            what: "record image cut short".into(),
        };
        if b.len() < 6 {
            return Err(bad());
        }
        let rl = usize::try_from(u32::from_le_bytes(b[..4].try_into().expect("4"))).expect("fits");
        let nl = usize::from(u16::from_le_bytes([b[4], b[5]]));
        if b.len() != 6 + rl + nl {
            return Err(bad());
        }
        Ok(Self {
            record: b[6..6 + rl].to_vec(),
            null_map: b[6 + rl..].to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let d = Draft::new(code::RECORD, "PT", JobHandle(42))
            .object(ObjectRef {
                library: Name::parse("QSTEST").unwrap(),
                object: Name::parse("CUST").unwrap(),
                member: Some(Name::parse("CUST").unwrap()),
                type_code: 0x0013,
            })
            .count(7)
            .cycle(3)
            .data(
                RecordImage {
                    record: vec![1, 2, 3],
                    null_map: vec![0],
                }
                .encode(),
            );
        let e = d.into_entry(99, 1_767_225_600, 15);
        assert_eq!(e.type_name(), "R PT");
        assert!(e.flags & flags::COMMITTED != 0);
        let b = e.encode();
        assert_eq!(b.len(), HEADER_LEN + 10);
        assert_eq!(Entry::announced_len(&b).unwrap(), b.len());
        let back = Entry::decode(&b).unwrap();
        assert_eq!(back, e);
        assert_eq!(back.job.qualified(), "000042/QSECOFR/QS400");
        let img = RecordImage::decode(&back.data).unwrap();
        assert_eq!(img.record, [1, 2, 3]);
        let none = Draft::new(code::JOURNAL, "PR", JobHandle(0)).into_entry(1, 5, 0);
        assert_eq!(Entry::decode(&none.encode()).unwrap().object, None);
    }
}
