// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*JRN` object: the attached receiver, the receiver directory,
//! the journaled objects, and appending entries.

use std::time::{SystemTime, UNIX_EPOCH};

use qsname::Name;
use store::PageNo;
use store::obj::header::ObjectHeader;
use store::obj::ops::{self, CreateParams};
use store::obj::space::Space;
use store::obj::{JobHandle, ObjectId};
use store::spec::{Layout, field};
use store::txn::Txn;

use crate::JRN;
use crate::entry::{Draft, ObjectRef, code};
use crate::error::{JrnError, Result};
use crate::rcv::{CrtJrnRcvOpts, Receiver, crtjrnrcv};

pub use crate::rcv::RcvStatus;

/// Magic at offset 0 of the attribute area.
pub const MAGIC: &[u8; 8] = b"QS400JRN";
/// Layout version.
pub const VERSION: u16 = 1;
/// Bytes of the attribute area used.
pub const ATTR_LEN: usize = 128;
/// Bytes of one receiver directory entry.
pub const DIR_ENTRY_LEN: usize = 32;
/// Bytes of one journaled object entry.
pub const OBJ_ENTRY_LEN: usize = 32;
/// Entries the tables start with.
const INITIAL_CAP: u32 = 8;

/// The spec layout of the attributes.
pub static LAYOUT: Layout = Layout {
    name: "JrnAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 2),
        field("flags", 10, 2),
        field("rcvsizopt", 12, 2),
        field("reserved", 14, 2),
        field("next_seq", 16, 8),
        field("attached", 24, 16),
        field("receiver_count", 40, 4),
        field("receiver_cap", 44, 4),
        field("dir_off", 48, 8),
        field("object_count", 56, 4),
        field("object_cap", 60, 4),
        field("obj_off", 64, 8),
        field("def_len", 72, 8),
        field("reserved2", 80, 48),
    ],
};

/// The spec layout of a receiver directory entry.
pub static DIR_LAYOUT: Layout = Layout {
    name: "RcvDirEntry",
    size: Some(DIR_ENTRY_LEN),
    fields: &[
        field("receiver", 0, 16),
        field("status", 16, 1),
        field("flags", 17, 1),
        field("reserved", 18, 6),
        field("attached", 24, 8),
    ],
};

/// The spec layout of a journaled object entry.
pub static OBJ_LAYOUT: Layout = Layout {
    name: "JrnObjEntry",
    size: Some(OBJ_ENTRY_LEN),
    fields: &[
        field("object", 0, 16),
        field("object_type", 16, 2),
        field("images", 18, 1),
        field("flags", 19, 1),
        field("reserved", 20, 4),
        field("started", 24, 8),
    ],
};

/// The attribute area, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct JrnAttr {
    /// `MNGRCV(*SYSTEM)`: a new receiver is attached at the threshold.
    pub mngrcv_system: bool,
    /// `DLTRCV(*YES)`: a detached receiver is deleted by the system.
    pub dltrcv: bool,
    /// `MINENTDTA`: minimized record images (kept, not applied).
    pub minentdta: bool,
    /// `FIXLENDTA` flags (kept).
    pub fixlendta: bool,
    /// `RCVSIZOPT` flags (kept).
    pub rcvsizopt: u16,
    /// The next sequence number.
    pub next_seq: u64,
    /// The attached receiver, `NONE` when none.
    pub attached: ObjectId,
    /// Receivers in the directory.
    pub receiver_count: u32,
    /// Directory capacity.
    pub receiver_cap: u32,
    /// Byte space offset of the directory.
    pub dir_off: u64,
    /// Journaled objects.
    pub object_count: u32,
    /// Object table capacity.
    pub object_cap: u32,
    /// Byte space offset of the object table.
    pub obj_off: u64,
    /// Bytes of the byte space in use.
    pub def_len: u64,
}

impl JrnAttr {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; ATTR_LEN] {
        let mut b = [0u8; ATTR_LEN];
        b[..8].copy_from_slice(MAGIC);
        b[8..10].copy_from_slice(&VERSION.to_le_bytes());
        let flags = u16::from(self.mngrcv_system)
            | u16::from(self.dltrcv) << 1
            | u16::from(self.minentdta) << 2
            | u16::from(self.fixlendta) << 3;
        b[10..12].copy_from_slice(&flags.to_le_bytes());
        b[12..14].copy_from_slice(&self.rcvsizopt.to_le_bytes());
        b[16..24].copy_from_slice(&self.next_seq.to_le_bytes());
        b[24..40].copy_from_slice(&self.attached.to_bytes());
        b[40..44].copy_from_slice(&self.receiver_count.to_le_bytes());
        b[44..48].copy_from_slice(&self.receiver_cap.to_le_bytes());
        b[48..56].copy_from_slice(&self.dir_off.to_le_bytes());
        b[56..60].copy_from_slice(&self.object_count.to_le_bytes());
        b[60..64].copy_from_slice(&self.object_cap.to_le_bytes());
        b[64..72].copy_from_slice(&self.obj_off.to_le_bytes());
        b[72..80].copy_from_slice(&self.def_len.to_le_bytes());
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let bad = |what: &str| JrnError::Corrupt {
            what: format!("journal attributes: {what}"),
        };
        if b.len() < ATTR_LEN || &b[..8] != MAGIC {
            return Err(bad("bad magic"));
        }
        if u16::from_le_bytes([b[8], b[9]]) != VERSION {
            return Err(bad("unknown version"));
        }
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().expect("4"));
        let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().expect("8"));
        let flags = u16::from_le_bytes([b[10], b[11]]);
        Ok(Self {
            mngrcv_system: flags & 1 != 0,
            dltrcv: flags & 2 != 0,
            minentdta: flags & 4 != 0,
            fixlendta: flags & 8 != 0,
            rcvsizopt: u16::from_le_bytes([b[12], b[13]]),
            next_seq: u64_at(16),
            attached: ObjectId::from_bytes(&b[24..40]),
            receiver_count: u32_at(40),
            receiver_cap: u32_at(44),
            dir_off: u64_at(48),
            object_count: u32_at(56),
            object_cap: u32_at(60),
            obj_off: u64_at(64),
            def_len: u64_at(72),
        })
    }
}

/// One receiver of the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RcvDirEntry {
    /// The receiver.
    pub receiver: ObjectId,
    /// Its status when last seen by the journal.
    pub status: RcvStatus,
    /// The chain is broken before this receiver (`*PARTIAL`).
    pub partial: bool,
    /// Attached, Unix seconds.
    pub attached: u64,
}

impl RcvDirEntry {
    fn encode(&self) -> [u8; DIR_ENTRY_LEN] {
        let mut b = [0u8; DIR_ENTRY_LEN];
        b[..16].copy_from_slice(&self.receiver.to_bytes());
        b[16] = match self.status {
            RcvStatus::Empty => 0,
            RcvStatus::Attached => 1,
            RcvStatus::Online => 2,
            RcvStatus::Saved => 3,
        };
        b[17] = u8::from(self.partial);
        b[24..32].copy_from_slice(&self.attached.to_le_bytes());
        b
    }

    fn decode(b: &[u8]) -> Self {
        Self {
            receiver: ObjectId::from_bytes(&b[..16]),
            status: match b[16] {
                1 => RcvStatus::Attached,
                2 => RcvStatus::Online,
                3 => RcvStatus::Saved,
                _ => RcvStatus::Empty,
            },
            partial: b[17] & 1 != 0,
            attached: u64::from_le_bytes(b[24..32].try_into().expect("8")),
        }
    }
}

/// One journaled object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JrnObjEntry {
    /// The object.
    pub object: ObjectId,
    /// Its type code.
    pub object_type: u16,
    /// `IMAGES(*BOTH)`.
    pub both_images: bool,
    /// `OMTJRNE(*OPNCLO)`.
    pub omit_open_close: bool,
    /// Journaling started, Unix seconds.
    pub started: u64,
}

impl JrnObjEntry {
    fn encode(&self) -> [u8; OBJ_ENTRY_LEN] {
        let mut b = [0u8; OBJ_ENTRY_LEN];
        b[..16].copy_from_slice(&self.object.to_bytes());
        b[16..18].copy_from_slice(&self.object_type.to_le_bytes());
        b[18] = if self.both_images { 2 } else { 1 };
        b[19] = u8::from(self.omit_open_close);
        b[24..32].copy_from_slice(&self.started.to_le_bytes());
        b
    }

    fn decode(b: &[u8]) -> Self {
        Self {
            object: ObjectId::from_bytes(&b[..16]),
            object_type: u16::from_le_bytes([b[16], b[17]]),
            both_images: b[18] == 2,
            omit_open_close: b[19] & 1 != 0,
            started: u64::from_le_bytes(b[24..32].try_into().expect("8")),
        }
    }
}

/// `CRTJRN` parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct CrtJrnOpts {
    /// `TEXT`.
    pub text: String,
    /// `MNGRCV(*SYSTEM)`.
    pub mngrcv_system: bool,
    /// `DLTRCV(*YES)`.
    pub dltrcv: bool,
    /// `MINENTDTA`.
    pub minentdta: bool,
    /// `FIXLENDTA`.
    pub fixlendta: bool,
    /// `RCVSIZOPT` flags.
    pub rcvsizopt: u16,
    /// Owner.
    pub owner: String,
    /// System name.
    pub system: String,
    /// Creation time.
    pub now: u64,
}

impl Default for CrtJrnOpts {
    fn default() -> Self {
        Self {
            text: String::new(),
            mngrcv_system: true,
            dltrcv: false,
            minentdta: false,
            fixlendta: false,
            rcvsizopt: 0,
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: 0,
        }
    }
}

/// `CHGJRN JRNRCV`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewReceiver {
    /// `*GEN`: a receiver named after the current one.
    Gen,
    /// A receiver that exists and was never attached.
    Named(ObjectId),
}

/// A journal, loaded.
#[derive(Clone, Debug)]
pub struct Journal {
    /// The object.
    pub id: ObjectId,
    /// Its header page.
    pub page: PageNo,
    /// The header.
    pub header: ObjectHeader,
    /// The attributes.
    pub attr: JrnAttr,
}

/// The clock, as seconds and microseconds.
fn clock() -> (u64, u32) {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (d.as_secs(), d.subsec_micros())
}

/// `CRTJRN`: a journal with `receiver` attached.
pub fn crtjrn(
    txn: &mut Txn<'_>,
    library: ObjectId,
    name: Name,
    receiver: ObjectId,
    opts: &CrtJrnOpts,
) -> Result<ObjectId> {
    let rcv = Receiver::load(txn, receiver)?;
    if rcv.attr.status != RcvStatus::Empty {
        return Err(JrnError::ReceiverUsed {
            receiver: rcv.name().to_string(),
            journal: ops::read_header(txn, rcv.attr.journal).map_or_else(
                |_| rcv.attr.journal.to_string(),
                |(_, h)| h.name.to_string(),
            ),
        });
    }
    let id = ops::create(
        txn,
        &CreateParams {
            library,
            name,
            type_code: JRN,
            attribute: String::new(),
            text: opts.text.clone(),
            owner: opts.owner.clone(),
            system: opts.system.clone(),
            now: opts.now,
            by_system: false,
        },
    )?;
    let dir_len = u64::from(INITIAL_CAP) * DIR_ENTRY_LEN as u64;
    let obj_len = u64::from(INITIAL_CAP) * OBJ_ENTRY_LEN as u64;
    Space::ensure(txn, id, dir_len + obj_len, 0)?;
    let attr = JrnAttr {
        mngrcv_system: opts.mngrcv_system,
        dltrcv: opts.dltrcv,
        minentdta: opts.minentdta,
        fixlendta: opts.fixlendta,
        rcvsizopt: opts.rcvsizopt,
        next_seq: 1,
        attached: ObjectId::NONE,
        receiver_count: 0,
        receiver_cap: INITIAL_CAP,
        dir_off: 0,
        object_count: 0,
        object_cap: INITIAL_CAP,
        obj_off: dir_len,
        def_len: dir_len + obj_len,
    };
    let (page, header) = ops::read_header(txn, id)?;
    let mut j = Journal {
        id,
        page,
        header,
        attr,
    };
    j.attach(txn, rcv, opts.now)?;
    Ok(id)
}

impl Journal {
    /// Load a journal.
    pub fn load(txn: &mut Txn<'_>, id: ObjectId) -> Result<Self> {
        let (page, header) = ops::read_header(txn, id)?;
        let a = ops::read_attr(txn, page)?;
        if header.id.type_code != JRN || a.len() < ATTR_LEN || &a[..8] != MAGIC {
            return Err(JrnError::NotAJournal {
                name: header.name.to_string(),
            });
        }
        let attr = JrnAttr::decode(&a[..ATTR_LEN])?;
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

    /// The receiver directory, oldest first.
    pub fn receivers(&self, txn: &mut Txn<'_>) -> Result<Vec<RcvDirEntry>> {
        let (space, _) = Space::load(txn, self.id)?;
        let n = usize::try_from(self.attr.receiver_count).expect("fits");
        let bytes = space.read(txn, self.attr.dir_off, n * DIR_ENTRY_LEN)?;
        Ok(bytes
            .chunks(DIR_ENTRY_LEN)
            .map(RcvDirEntry::decode)
            .collect())
    }

    /// The journaled objects.
    pub fn objects(&self, txn: &mut Txn<'_>) -> Result<Vec<JrnObjEntry>> {
        let (space, _) = Space::load(txn, self.id)?;
        let n = usize::try_from(self.attr.object_count).expect("fits");
        let bytes = space.read(txn, self.attr.obj_off, n * OBJ_ENTRY_LEN)?;
        Ok(bytes
            .chunks(OBJ_ENTRY_LEN)
            .map(JrnObjEntry::decode)
            .collect())
    }

    /// The journaled object entry for `object`, if any.
    pub fn object(&self, txn: &mut Txn<'_>, object: ObjectId) -> Result<Option<JrnObjEntry>> {
        Ok(self.objects(txn)?.into_iter().find(|o| o.object == object))
    }

    fn write_receivers(&mut self, txn: &mut Txn<'_>, dir: &[RcvDirEntry]) -> Result<()> {
        let count = u32::try_from(dir.len()).expect("few");
        if count > self.attr.receiver_cap {
            let cap = (self.attr.receiver_cap * 2).max(count);
            self.attr.dir_off = self.attr.def_len;
            self.attr.receiver_cap = cap;
            self.attr.def_len += u64::from(cap) * DIR_ENTRY_LEN as u64;
            Space::ensure(txn, self.id, self.attr.def_len, 0)?;
        }
        let mut bytes = Vec::with_capacity(dir.len() * DIR_ENTRY_LEN);
        for e in dir {
            bytes.extend(e.encode());
        }
        let (space, _) = Space::load(txn, self.id)?;
        space.write(txn, self.attr.dir_off, &bytes)?;
        self.attr.receiver_count = count;
        self.save(txn)
    }

    fn write_objects(&mut self, txn: &mut Txn<'_>, objs: &[JrnObjEntry]) -> Result<()> {
        let count = u32::try_from(objs.len()).expect("few");
        if count > self.attr.object_cap {
            let cap = (self.attr.object_cap * 2).max(count);
            self.attr.obj_off = self.attr.def_len;
            self.attr.object_cap = cap;
            self.attr.def_len += u64::from(cap) * OBJ_ENTRY_LEN as u64;
            Space::ensure(txn, self.id, self.attr.def_len, 0)?;
        }
        let mut bytes = Vec::with_capacity(objs.len() * OBJ_ENTRY_LEN);
        for e in objs {
            bytes.extend(e.encode());
        }
        let (space, _) = Space::load(txn, self.id)?;
        space.write(txn, self.attr.obj_off, &bytes)?;
        self.attr.object_count = count;
        self.save(txn)
    }

    /// Start journaling `object`; refused when it is journaled already.
    pub fn add_object(&mut self, txn: &mut Txn<'_>, entry: JrnObjEntry, name: &str) -> Result<()> {
        let mut objs = self.objects(txn)?;
        if objs.iter().any(|o| o.object == entry.object) {
            return Err(JrnError::AlreadyJournaled {
                name: name.to_string(),
                journal: self.name().to_string(),
            });
        }
        objs.push(entry);
        self.write_objects(txn, &objs)
    }

    /// End journaling `object`.
    pub fn remove_object(&mut self, txn: &mut Txn<'_>, object: ObjectId, name: &str) -> Result<()> {
        let mut objs = self.objects(txn)?;
        let before = objs.len();
        objs.retain(|o| o.object != object);
        if objs.len() == before {
            return Err(JrnError::NotJournaled {
                name: name.to_string(),
            });
        }
        self.write_objects(txn, &objs)
    }

    /// The attached receiver.
    pub fn attached(&self, txn: &mut Txn<'_>) -> Result<Receiver> {
        if self.attr.attached.is_none() {
            return Err(JrnError::NoReceiver {
                journal: self.name().to_string(),
            });
        }
        Receiver::load(txn, self.attr.attached)
    }

    /// Attach an empty receiver after the current one.
    fn attach(&mut self, txn: &mut Txn<'_>, mut rcv: Receiver, now: u64) -> Result<()> {
        let prev = self.attr.attached;
        rcv.attr.status = RcvStatus::Attached;
        rcv.attr.journal = self.id;
        rcv.attr.attached = now;
        rcv.attr.prev = prev;
        rcv.save(txn)?;
        let mut dir = self.receivers(txn)?;
        for d in &mut dir {
            if d.receiver == prev {
                d.status = RcvStatus::Online;
            }
        }
        dir.push(RcvDirEntry {
            receiver: rcv.id,
            status: RcvStatus::Attached,
            partial: false,
            attached: now,
        });
        self.attr.attached = rcv.id;
        self.write_receivers(txn, &dir)
    }

    /// Number and write an entry to the attached receiver; the sequence
    /// number it took. At the threshold under `MNGRCV(*SYSTEM)` a new
    /// receiver is generated and attached first.
    pub fn append(&mut self, txn: &mut Txn<'_>, draft: Draft) -> Result<u64> {
        let (now, micros) = clock();
        let rcv = self.attached(txn)?;
        if rcv.over_threshold() && self.attr.mngrcv_system {
            self.chgjrn(txn, NewReceiver::Gen, draft.time.unwrap_or(now))?;
        }
        let mut rcv = self.attached(txn)?;
        let seq = self.attr.next_seq;
        let entry = draft.into_entry(seq, now, micros);
        rcv.append(txn, &entry)?;
        self.attr.next_seq = seq + 1;
        self.save(txn)?;
        Ok(seq)
    }

    /// `CHGJRN JRNRCV(...)`: detach the current receiver with a `J NR`
    /// entry naming the next, attach the new one with a `J PR` entry
    /// naming the previous.
    pub fn chgjrn(&mut self, txn: &mut Txn<'_>, new: NewReceiver, now: u64) -> Result<ObjectId> {
        store::obj::guard::check_id(txn, "jrn.change", self.id)?;
        let old = self.attached(txn)?;
        let new_id = match new {
            NewReceiver::Named(id) => {
                let r = Receiver::load(txn, id)?;
                if r.attr.status != RcvStatus::Empty {
                    return Err(JrnError::ReceiverUsed {
                        receiver: r.name().to_string(),
                        journal: self.name().to_string(),
                    });
                }
                id
            }
            NewReceiver::Gen => crtjrnrcv(
                txn,
                old.header.library,
                generate_name(old.name()),
                &CrtJrnRcvOpts {
                    text: ccsid::decode_trimmed(&old.header.text),
                    threshold_kb: old.attr.threshold_kb,
                    owner: ccsid::decode_trimmed(&old.header.owner),
                    system: ccsid::decode_trimmed(&old.header.system),
                    now,
                },
            )?,
        };
        let new_rcv = Receiver::load(txn, new_id)?;
        let (lib_name, _) = self.names(txn, &old)?;
        // J NR in the old receiver.
        let nr = Draft::new(code::JOURNAL, "NR", JobHandle(0))
            .object(ObjectRef {
                library: lib_name,
                object: new_rcv.name(),
                member: None,
                type_code: crate::JRNRCV,
            })
            .at(now);
        let (clock_now, micros) = clock();
        let mut old = old;
        old.attr.next = new_id;
        let seq = self.attr.next_seq;
        old.append(txn, &nr.into_entry(seq, clock_now, micros))?;
        old.attr.status = RcvStatus::Online;
        old.attr.detached = now;
        old.save(txn)?;
        self.attr.next_seq = seq + 1;
        self.attach(txn, new_rcv, now)?;
        // J PR in the new one.
        let pr = Draft::new(code::JOURNAL, "PR", JobHandle(0))
            .object(ObjectRef {
                library: lib_name,
                object: old.name(),
                member: None,
                type_code: crate::JRNRCV,
            })
            .at(now);
        let mut new_rcv = self.attached(txn)?;
        let seq = self.attr.next_seq;
        new_rcv.append(txn, &pr.into_entry(seq, clock_now, micros))?;
        self.attr.next_seq = seq + 1;
        self.save(txn)?;
        Ok(new_id)
    }

    /// The library name of a receiver and the journal's own.
    fn names(&self, txn: &mut Txn<'_>, rcv: &Receiver) -> Result<(Name, Name)> {
        let (_, lib) = ops::read_header(txn, rcv.header.library)?;
        Ok((lib.name, self.name()))
    }

    /// `SNDJRNE`: a user entry of `kind` with `data`.
    pub fn sndjrne(
        &mut self,
        txn: &mut Txn<'_>,
        job: JobHandle,
        kind: &str,
        data: &[u8],
    ) -> Result<u64> {
        self.append(txn, Draft::new(code::USER, kind, job).data(data.to_vec()))
    }

    /// The whole chain of receivers that still exist, oldest first.
    pub fn chain(&self, txn: &mut Txn<'_>) -> Result<Vec<Receiver>> {
        let mut out = Vec::new();
        for d in self.receivers(txn)? {
            if let Ok(r) = Receiver::load(txn, d.receiver) {
                out.push(r);
            }
        }
        Ok(out)
    }
}

/// `DLTJRN`: refused while objects are journaled; the attached receiver
/// is detached and left online.
pub fn dltjrn(txn: &mut Txn<'_>, job: JobHandle, jrn: ObjectId, now: u64) -> Result<()> {
    store::obj::guard::check_id(txn, "obj.delete", jrn)?;
    let mut j = Journal::load(txn, jrn)?;
    if j.attr.object_count > 0 {
        return Err(JrnError::ObjectsJournaled {
            journal: j.name().to_string(),
            count: j.attr.object_count,
        });
    }
    if let Ok(mut r) = j.attached(txn) {
        r.attr.status = RcvStatus::Online;
        r.attr.detached = now;
        r.save(txn)?;
    }
    j.attr.attached = ObjectId::NONE;
    j.save(txn)?;
    ops::delete(txn, job, jrn)?;
    Ok(())
}

/// The next receiver name: the trailing digits plus one, else `0001`
/// appended (the name cut to make room).
#[must_use]
pub fn generate_name(name: Name) -> Name {
    let s = name.to_string();
    let digits = s.chars().rev().take_while(char::is_ascii_digit).count();
    let (stem, num) = s.split_at(s.len() - digits);
    let next = if digits == 0 {
        format!("{}0001", &stem[..stem.len().min(6)])
    } else {
        let n: u64 = num.parse().unwrap_or(0) + 1;
        let width = digits.max(4);
        let mut text = format!("{n:0width$}");
        if text.len() > width {
            text = "0".repeat(width);
        }
        let room = 10 - text.len();
        format!("{}{text}", &stem[..stem.len().min(room)])
    };
    Name::parse(&next).unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_advance() {
        let n = |s: &str| Name::parse(s).unwrap();
        assert_eq!(generate_name(n("RCV0001")), n("RCV0002"));
        assert_eq!(generate_name(n("RCV0999")), n("RCV1000"));
        assert_eq!(generate_name(n("RCV9999")), n("RCV0000"));
        assert_eq!(generate_name(n("JRNRCV")), n("JRNRCV0001"));
        assert_eq!(generate_name(n("LONGNAMEXY")), n("LONGNA0001"));
        assert_eq!(generate_name(n("R7")), n("R0008"));
    }
}
