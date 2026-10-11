// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*JRNRCV` object: an attribute area and an append-only byte
//! space of entries.

use qsname::Name;
use store::PageNo;
use store::obj::header::ObjectHeader;
use store::obj::ops::{self, CreateParams};
use store::obj::space::Space;
use store::obj::{JobHandle, ObjectId};
use store::spec::{Layout, field};
use store::txn::Txn;

use crate::JRNRCV;
use crate::entry::Entry;
use crate::error::{JrnError, Result};

/// Magic at offset 0 of the attribute area.
pub const MAGIC: &[u8; 8] = b"QS400RCV";
/// Layout version.
pub const VERSION: u16 = 1;
/// Bytes of the attribute area used.
pub const ATTR_LEN: usize = 128;
/// Bytes the entry space grows by.
const GROWTH: u64 = 64 * 1024;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "RcvAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 2),
        field("status", 10, 1),
        field("flags", 11, 1),
        field("reserved", 12, 4),
        field("journal", 16, 16),
        field("first_seq", 32, 8),
        field("last_seq", 40, 8),
        field("entry_count", 48, 8),
        field("bytes_used", 56, 8),
        field("threshold_kb", 64, 4),
        field("reserved2", 68, 4),
        field("attached", 72, 8),
        field("detached", 80, 8),
        field("prev", 88, 16),
        field("next", 104, 16),
        field("reserved3", 120, 8),
    ],
};

/// Receiver status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RcvStatus {
    /// Never attached.
    #[default]
    Empty,
    /// Attached to its journal, receiving entries.
    Attached,
    /// Detached, entries readable (`*ONLINE`).
    Online,
    /// Saved after detaching (`*SAVED`).
    Saved,
}

impl RcvStatus {
    /// The `*NAME`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Empty => "*EMPTY",
            Self::Attached => "*ATTACHED",
            Self::Online => "*ONLINE",
            Self::Saved => "*SAVED",
        }
    }
}

/// The attribute area, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RcvAttr {
    /// Status.
    pub status: RcvStatus,
    /// The journal it was attached to, `NONE` before the first attach.
    pub journal: ObjectId,
    /// First sequence number held, 0 when empty.
    pub first_seq: u64,
    /// Last sequence number held, 0 when empty.
    pub last_seq: u64,
    /// Entries held.
    pub entry_count: u64,
    /// Bytes of the entry space in use.
    pub bytes_used: u64,
    /// `THRESHOLD` in KiB, 0 for none.
    pub threshold_kb: u32,
    /// Attached, Unix seconds.
    pub attached: u64,
    /// Detached, Unix seconds.
    pub detached: u64,
    /// The receiver before this one in the chain.
    pub prev: ObjectId,
    /// The receiver after this one in the chain.
    pub next: ObjectId,
}

impl RcvAttr {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; ATTR_LEN] {
        let mut b = [0u8; ATTR_LEN];
        b[..8].copy_from_slice(MAGIC);
        b[8..10].copy_from_slice(&VERSION.to_le_bytes());
        b[10] = match self.status {
            RcvStatus::Empty => 0,
            RcvStatus::Attached => 1,
            RcvStatus::Online => 2,
            RcvStatus::Saved => 3,
        };
        b[16..32].copy_from_slice(&self.journal.to_bytes());
        b[32..40].copy_from_slice(&self.first_seq.to_le_bytes());
        b[40..48].copy_from_slice(&self.last_seq.to_le_bytes());
        b[48..56].copy_from_slice(&self.entry_count.to_le_bytes());
        b[56..64].copy_from_slice(&self.bytes_used.to_le_bytes());
        b[64..68].copy_from_slice(&self.threshold_kb.to_le_bytes());
        b[72..80].copy_from_slice(&self.attached.to_le_bytes());
        b[80..88].copy_from_slice(&self.detached.to_le_bytes());
        b[88..104].copy_from_slice(&self.prev.to_bytes());
        b[104..120].copy_from_slice(&self.next.to_bytes());
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let bad = |what: &str| JrnError::Corrupt {
            what: format!("receiver attributes: {what}"),
        };
        if b.len() < ATTR_LEN || &b[..8] != MAGIC {
            return Err(bad("bad magic"));
        }
        if u16::from_le_bytes([b[8], b[9]]) != VERSION {
            return Err(bad("unknown version"));
        }
        let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().expect("8"));
        let status = match b[10] {
            0 => RcvStatus::Empty,
            1 => RcvStatus::Attached,
            2 => RcvStatus::Online,
            3 => RcvStatus::Saved,
            _ => return Err(bad("status")),
        };
        Ok(Self {
            status,
            journal: ObjectId::from_bytes(&b[16..32]),
            first_seq: u64_at(32),
            last_seq: u64_at(40),
            entry_count: u64_at(48),
            bytes_used: u64_at(56),
            threshold_kb: u32::from_le_bytes(b[64..68].try_into().expect("4")),
            attached: u64_at(72),
            detached: u64_at(80),
            prev: ObjectId::from_bytes(&b[88..104]),
            next: ObjectId::from_bytes(&b[104..120]),
        })
    }
}

/// `CRTJRNRCV` parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrtJrnRcvOpts {
    /// `TEXT`.
    pub text: String,
    /// `THRESHOLD` in KiB, 0 for `*NONE`.
    pub threshold_kb: u32,
    /// Owner.
    pub owner: String,
    /// System name.
    pub system: String,
    /// Creation time.
    pub now: u64,
}

impl Default for CrtJrnRcvOpts {
    fn default() -> Self {
        Self {
            text: String::new(),
            threshold_kb: 0,
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: 0,
        }
    }
}

/// `CRTJRNRCV`: an empty receiver.
pub fn crtjrnrcv(
    txn: &mut Txn<'_>,
    library: ObjectId,
    name: Name,
    opts: &CrtJrnRcvOpts,
) -> Result<ObjectId> {
    let id = ops::create(
        txn,
        &CreateParams {
            library,
            name,
            type_code: JRNRCV,
            attribute: String::new(),
            text: opts.text.clone(),
            owner: opts.owner.clone(),
            system: opts.system.clone(),
            now: opts.now,
            by_system: false,
        },
    )?;
    let attr = RcvAttr {
        status: RcvStatus::Empty,
        journal: ObjectId::NONE,
        first_seq: 0,
        last_seq: 0,
        entry_count: 0,
        bytes_used: 0,
        threshold_kb: opts.threshold_kb,
        attached: 0,
        detached: 0,
        prev: ObjectId::NONE,
        next: ObjectId::NONE,
    };
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 0, &attr.encode())?;
    Ok(id)
}

/// A receiver, loaded.
#[derive(Clone, Debug)]
pub struct Receiver {
    /// The object.
    pub id: ObjectId,
    /// Its header page.
    pub page: PageNo,
    /// The header.
    pub header: ObjectHeader,
    /// The attributes.
    pub attr: RcvAttr,
}

impl Receiver {
    /// Load a receiver.
    pub fn load(txn: &mut Txn<'_>, id: ObjectId) -> Result<Self> {
        let (page, header) = ops::read_header(txn, id)?;
        let a = ops::read_attr(txn, page)?;
        if header.id.type_code != JRNRCV || a.len() < ATTR_LEN || &a[..8] != MAGIC {
            return Err(JrnError::NotAReceiver {
                name: header.name.to_string(),
            });
        }
        let attr = RcvAttr::decode(&a[..ATTR_LEN])?;
        Ok(Self {
            id,
            page,
            header,
            attr,
        })
    }

    /// The name.
    #[must_use]
    pub fn name(&self) -> Name {
        self.header.name
    }

    /// Write the attributes back.
    pub fn save(&self, txn: &mut Txn<'_>) -> Result<()> {
        ops::write_attr(txn, self.page, 0, &self.attr.encode())?;
        Ok(())
    }

    /// Append an entry (the journal numbered it) and save.
    pub(crate) fn append(&mut self, txn: &mut Txn<'_>, entry: &Entry) -> Result<()> {
        let bytes = entry.encode();
        let off = self.attr.bytes_used;
        let end = off + u64::try_from(bytes.len()).expect("fits");
        let (mut space, _) = Space::load(txn, self.id)?;
        if space.bytes() < end {
            let grow = (space.bytes() + GROWTH).max(end);
            space = Space::ensure(txn, self.id, grow, 0)?;
        }
        space.write(txn, off, &bytes)?;
        if self.attr.entry_count == 0 {
            self.attr.first_seq = entry.seq;
        }
        self.attr.last_seq = entry.seq;
        self.attr.entry_count += 1;
        self.attr.bytes_used = end;
        self.save(txn)
    }

    /// Whether the threshold is reached.
    #[must_use]
    pub fn over_threshold(&self) -> bool {
        self.attr.threshold_kb > 0
            && self.attr.bytes_used >= u64::from(self.attr.threshold_kb) * 1024
    }

    /// Read every entry, in order.
    pub fn entries(&self, txn: &mut Txn<'_>) -> Result<Vec<Entry>> {
        let mut c = Cursor::new(self);
        let mut out = Vec::new();
        while let Some(e) = c.next(txn)? {
            out.push(e);
        }
        Ok(out)
    }
}

/// Reads a receiver's entries front to back.
#[derive(Clone, Debug)]
pub struct Cursor {
    receiver: ObjectId,
    end: u64,
    at: u64,
}

impl Cursor {
    /// From the first entry.
    #[must_use]
    pub fn new(rcv: &Receiver) -> Self {
        Self {
            receiver: rcv.id,
            end: rcv.attr.bytes_used,
            at: 0,
        }
    }

    /// The next entry, or `None` at the end.
    pub fn next(&mut self, txn: &mut Txn<'_>) -> Result<Option<Entry>> {
        if self.at >= self.end {
            return Ok(None);
        }
        let (space, _) = Space::load(txn, self.receiver)?;
        let head = space.read(txn, self.at, 4)?;
        let len = Entry::announced_len(&head)?;
        if self.at + u64::try_from(len).expect("fits") > self.end {
            return Err(JrnError::Corrupt {
                what: "entry runs past the receiver's used bytes".into(),
            });
        }
        let bytes = space.read(txn, self.at, len)?;
        self.at += u64::try_from(len).expect("fits");
        Entry::decode(&bytes).map(Some)
    }
}

/// `DLTJRNRCV`: an attached receiver is refused.
pub fn dltjrnrcv(txn: &mut Txn<'_>, job: JobHandle, rcv: ObjectId) -> Result<()> {
    let r = Receiver::load(txn, rcv)?;
    if r.attr.status == RcvStatus::Attached {
        let jname = ops::read_header(txn, r.attr.journal)
            .map_or_else(|_| r.attr.journal.to_string(), |(_, h)| h.name.to_string());
        return Err(JrnError::ReceiverAttached {
            receiver: r.name().to_string(),
            journal: jname,
        });
    }
    // Unlink from the neighbours that still exist.
    for (neighbour, is_prev) in [(r.attr.prev, true), (r.attr.next, false)] {
        if neighbour.is_none() {
            continue;
        }
        if let Ok(mut n) = Receiver::load(txn, neighbour) {
            if is_prev {
                n.attr.next = ObjectId::NONE;
            } else {
                n.attr.prev = ObjectId::NONE;
            }
            n.save(txn)?;
        }
    }
    ops::delete(txn, job, rcv)?;
    Ok(())
}
