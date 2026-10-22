// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*DTAQ` and `*USRQ`: FIFO, LIFO and keyed queues over a byte space.
//! See the format spec §10.

use std::cmp::Ordering;

use crate::bytes::{get_u16, get_u32, get_u64, put_u16, put_u32, put_u64};
use crate::obj::ObjectId;
use crate::obj::ops::{self, CreateParams};
use crate::obj::space::{PAGE_BYTES, Space};
use crate::obj::types;
use crate::spec::{Layout, field};
use crate::txn::Txn;
use crate::{Result, StoreError};

/// Bytes of the attribute area used.
pub const ATTR_LEN: usize = 48;
/// Entry header bytes.
pub const ENTRY_LEN: usize = 48;
/// Longest entry data.
pub const MAX_ENTRY: u32 = 64512;
/// Longest key.
pub const MAX_KEY: u16 = 256;

const OFF_ORDER: usize = 0;
const OFF_SENDER_ID: usize = 1;
const OFF_KEY_LEN: usize = 2;
const OFF_MAX_ENTRY: usize = 4;
const OFF_HEAD: usize = 8;
const OFF_TAIL: usize = 16;
const OFF_COUNT: usize = 24;
const OFF_NEXT_SEQ: usize = 32;
const OFF_SPACE: usize = 40;

const E_SEQ: usize = 0;
const E_LEN: usize = 8;
const E_KEY_LEN: usize = 12;
const E_FLAGS: usize = 14;
const E_SENDER: usize = 16;
const FLAG_LIVE: u16 = 1;

/// The spec layout of the attribute area.
pub static ATTR_LAYOUT: Layout = Layout {
    name: "DtaqAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("order", OFF_ORDER, 1),
        field("sender_id", OFF_SENDER_ID, 1),
        field("key_len", OFF_KEY_LEN, 2),
        field("max_entry_len", OFF_MAX_ENTRY, 4),
        field("head", OFF_HEAD, 8),
        field("tail", OFF_TAIL, 8),
        field("entry_count", OFF_COUNT, 8),
        field("next_seq", OFF_NEXT_SEQ, 8),
        field("space_bytes", OFF_SPACE, 8),
    ],
};

/// The spec layout of an entry header.
pub static ENTRY_LAYOUT: Layout = Layout {
    name: "DtaqEntry",
    size: Some(ENTRY_LEN),
    fields: &[
        field("seq", E_SEQ, 8),
        field("len", E_LEN, 4),
        field("key_len", E_KEY_LEN, 2),
        field("flags", E_FLAGS, 2),
        field("sender", E_SENDER, 26),
        field("reserved", 42, 6),
    ],
};

/// Queue order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// `*FIFO`.
    Fifo,
    /// `*LIFO`.
    Lifo,
    /// `*KEYED` with keys of this length.
    Keyed {
        /// Key length, 1..=256.
        key_len: u16,
    },
}

/// The attribute area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueAttr {
    /// Order.
    pub order: Order,
    /// Whether entries carry a sender.
    pub sender_id: bool,
    /// Longest entry data.
    pub max_entry_len: u32,
    /// First live entry.
    pub head: u64,
    /// Where the next entry goes.
    pub tail: u64,
    /// Live entries.
    pub entry_count: u64,
    /// Next sequence.
    pub next_seq: u64,
    /// Bytes allocated.
    pub space_bytes: u64,
}

impl QueueAttr {
    fn key_len(&self) -> usize {
        match self.order {
            Order::Keyed { key_len } => usize::from(key_len),
            _ => 0,
        }
    }

    fn encode(&self) -> [u8; ATTR_LEN] {
        let mut a = [0u8; ATTR_LEN];
        a[OFF_ORDER] = match self.order {
            Order::Fifo => 0,
            Order::Lifo => 1,
            Order::Keyed { .. } => 2,
        };
        a[OFF_SENDER_ID] = u8::from(self.sender_id);
        put_u16(
            &mut a,
            OFF_KEY_LEN,
            u16::try_from(self.key_len()).expect("checked"),
        );
        put_u32(&mut a, OFF_MAX_ENTRY, self.max_entry_len);
        put_u64(&mut a, OFF_HEAD, self.head);
        put_u64(&mut a, OFF_TAIL, self.tail);
        put_u64(&mut a, OFF_COUNT, self.entry_count);
        put_u64(&mut a, OFF_NEXT_SEQ, self.next_seq);
        put_u64(&mut a, OFF_SPACE, self.space_bytes);
        a
    }

    fn decode(a: &[u8]) -> Result<Self> {
        let key_len = get_u16(a, OFF_KEY_LEN);
        let order = match a[OFF_ORDER] {
            0 => Order::Fifo,
            1 => Order::Lifo,
            2 => Order::Keyed { key_len },
            k => {
                return Err(StoreError::BadValue {
                    what: format!("queue order {k}"),
                });
            }
        };
        Ok(Self {
            order,
            sender_id: a[OFF_SENDER_ID] != 0,
            max_entry_len: get_u32(a, OFF_MAX_ENTRY),
            head: get_u64(a, OFF_HEAD),
            tail: get_u64(a, OFF_TAIL),
            entry_count: get_u64(a, OFF_COUNT),
            next_seq: get_u64(a, OFF_NEXT_SEQ),
            space_bytes: get_u64(a, OFF_SPACE),
        })
    }
}

/// One entry as received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Sequence number.
    pub seq: u64,
    /// Key, empty unless keyed.
    pub key: Vec<u8>,
    /// Data.
    pub data: Vec<u8>,
    /// Sender (job name, user, number), blank without sender ID.
    pub sender: [u8; 26],
}

/// Key comparison for a keyed receive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Greater or equal.
    Ge,
    /// Greater.
    Gt,
    /// Less or equal.
    Le,
    /// Less.
    Lt,
}

impl KeyOp {
    fn matches(self, entry_key: &[u8], key: &[u8]) -> bool {
        let o = entry_key.cmp(key);
        match self {
            Self::Eq => o == Ordering::Equal,
            Self::Ne => o != Ordering::Equal,
            Self::Ge => o != Ordering::Less,
            Self::Gt => o == Ordering::Greater,
            Self::Le => o != Ordering::Greater,
            Self::Lt => o == Ordering::Less,
        }
    }
}

/// What to receive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Select<'a> {
    /// The next entry in queue order.
    Next,
    /// The first entry whose key compares as given.
    Key(KeyOp, &'a [u8]),
}

/// Create a queue. `type_code` is `*DTAQ` or `*USRQ` (no sender ID).
pub fn create(
    txn: &mut Txn<'_>,
    params: &CreateParams,
    type_code: u16,
    order: Order,
    max_entry_len: u32,
    sender_id: bool,
) -> Result<ObjectId> {
    if !(1..=MAX_ENTRY).contains(&max_entry_len) {
        return Err(StoreError::BadValue {
            what: format!("entry length {max_entry_len}"),
        });
    }
    if let Order::Keyed { key_len } = order
        && !(1..=MAX_KEY).contains(&key_len)
    {
        return Err(StoreError::BadValue {
            what: format!("key length {key_len}"),
        });
    }
    if type_code != types::DTAQ && type_code != types::USRQ {
        return Err(StoreError::BadValue {
            what: "not a queue type".into(),
        });
    }
    let params = CreateParams {
        type_code,
        by_system: params.by_system || type_code == types::USRQ,
        ..params.clone()
    };
    let id = ops::create(txn, &params)?;
    let attr = QueueAttr {
        order,
        sender_id: sender_id && type_code == types::DTAQ,
        max_entry_len,
        head: 0,
        tail: 0,
        entry_count: 0,
        next_seq: 1,
        space_bytes: 0,
    };
    save(txn, id, &attr)?;
    Ok(id)
}

/// The attribute area.
pub fn describe(txn: &mut Txn<'_>, id: ObjectId) -> Result<QueueAttr> {
    let (page, h) = ops::read_header(txn, id)?;
    if h.id.type_code != types::DTAQ && h.id.type_code != types::USRQ {
        return Err(StoreError::ObjectNotFound {
            name: h.name.to_string(),
            library: String::new(),
        });
    }
    let a = ops::read_attr(txn, page)?;
    QueueAttr::decode(&a[..ATTR_LEN])
}

fn save(txn: &mut Txn<'_>, id: ObjectId, attr: &QueueAttr) -> Result<()> {
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 0, &attr.encode())
}

/// Entry size on disk.
fn slot_len(key_len: usize, data_len: usize) -> u64 {
    ((ENTRY_LEN + key_len + data_len) as u64).div_ceil(8) * 8
}

/// Append an entry.
pub fn send(
    txn: &mut Txn<'_>,
    id: ObjectId,
    key: &[u8],
    data: &[u8],
    sender: Option<[u8; 26]>,
) -> Result<u64> {
    crate::obj::guard::check_id(txn, "dtaq.send", id)?;
    let mut attr = describe(txn, id)?;
    if data.len() > attr.max_entry_len as usize || key.len() != attr.key_len() {
        return Err(StoreError::BadValue {
            what: format!(
                "entry of {} bytes with a {}-byte key",
                data.len(),
                key.len()
            ),
        });
    }
    let need = slot_len(key.len(), data.len());
    if attr.tail + need > attr.space_bytes {
        let space = Space::ensure(txn, id, attr.tail + need, 0)?;
        attr.space_bytes = space.bytes();
    }
    let (mut space, _) = Space::load(txn, id)?;
    let mut rec = vec![0u8; usize::try_from(need).expect("entry fits")];
    put_u64(&mut rec, E_SEQ, attr.next_seq);
    put_u32(&mut rec, E_LEN, u32::try_from(data.len()).expect("checked"));
    put_u16(
        &mut rec,
        E_KEY_LEN,
        u16::try_from(key.len()).expect("checked"),
    );
    put_u16(&mut rec, E_FLAGS, FLAG_LIVE);
    rec[E_SENDER..E_SENDER + 26].copy_from_slice(&match (attr.sender_id, sender) {
        (true, Some(s)) => s,
        _ => [ccsid::SPACE; 26],
    });
    rec[ENTRY_LEN..ENTRY_LEN + key.len()].copy_from_slice(key);
    rec[ENTRY_LEN + key.len()..ENTRY_LEN + key.len() + data.len()].copy_from_slice(data);
    space.write(txn, attr.tail, &rec)?;
    let seq = attr.next_seq;
    attr.tail += need;
    attr.entry_count += 1;
    attr.next_seq += 1;
    save(txn, id, &attr)?;
    Ok(seq)
}

/// A stored entry's header and position.
struct Slot {
    off: u64,
    len: u64,
    live: bool,
    entry: Entry,
}

fn read_slot(txn: &mut Txn<'_>, space: &Space, off: u64) -> Result<Slot> {
    let h = space.read(txn, off, ENTRY_LEN)?;
    let data_len = get_u32(&h, E_LEN) as usize;
    let key_len = usize::from(get_u16(&h, E_KEY_LEN));
    let len = slot_len(key_len, data_len);
    let rest = space.read(txn, off + ENTRY_LEN as u64, key_len + data_len)?;
    Ok(Slot {
        off,
        len,
        live: get_u16(&h, E_FLAGS) & FLAG_LIVE != 0,
        entry: Entry {
            seq: get_u64(&h, E_SEQ),
            key: rest[..key_len].to_vec(),
            data: rest[key_len..].to_vec(),
            sender: h[E_SENDER..E_SENDER + 26].try_into().expect("26"),
        },
    })
}

/// Every live slot from head to tail.
fn live_slots(txn: &mut Txn<'_>, space: &Space, attr: &QueueAttr) -> Result<Vec<Slot>> {
    let mut out = Vec::new();
    let mut off = attr.head;
    while off < attr.tail {
        let s = read_slot(txn, space, off)?;
        off += s.len;
        if s.live {
            out.push(s);
        }
    }
    Ok(out)
}

/// Take one entry. `wait` is always zero in Phase 2: `None` when
/// nothing matches.
pub fn receive(txn: &mut Txn<'_>, id: ObjectId, select: &Select<'_>) -> Result<Option<Entry>> {
    crate::obj::guard::check_id(txn, "dtaq.receive", id)?;
    let mut attr = describe(txn, id)?;
    if attr.entry_count == 0 {
        return Ok(None);
    }
    let (mut space, _) = Space::load(txn, id)?;
    let slots = live_slots(txn, &space, &attr)?;
    let pick = match (select, attr.order) {
        (Select::Next, Order::Lifo) => slots.len().checked_sub(1),
        (Select::Next, _) => (!slots.is_empty()).then_some(0),
        (Select::Key(op, key), Order::Keyed { .. }) => {
            slots.iter().position(|s| op.matches(&s.entry.key, key))
        }
        (Select::Key(..), _) => {
            return Err(StoreError::BadValue {
                what: "keyed receive on an unkeyed queue".into(),
            });
        }
    };
    let Some(i) = pick else {
        return Ok(None);
    };
    let slot = &slots[i];
    // Mark dead, then move head past dead entries and pull tail back
    // over a dead last entry.
    let mut flags = [0u8; 2];
    put_u16(&mut flags, 0, 0);
    space.write(txn, slot.off + E_FLAGS as u64, &flags)?;
    attr.entry_count -= 1;
    if attr.entry_count == 0 {
        attr.head = 0;
        attr.tail = 0;
    } else {
        if i == 0 {
            attr.head = slots.get(1).map_or(attr.tail, |s| s.off);
        }
        if i + 1 == slots.len() {
            attr.tail = slot.off;
        }
    }
    save(txn, id, &attr)?;
    Ok(Some(slot.entry.clone()))
}

/// Remove every entry.
pub fn clear(txn: &mut Txn<'_>, id: ObjectId) -> Result<()> {
    crate::obj::guard::check_id(txn, "dtaq.clear", id)?;
    let mut attr = describe(txn, id)?;
    attr.head = 0;
    attr.tail = 0;
    attr.entry_count = 0;
    save(txn, id, &attr)
}

/// Live entries.
pub fn entry_count(txn: &mut Txn<'_>, id: ObjectId) -> Result<u64> {
    Ok(describe(txn, id)?.entry_count)
}

/// Pages the byte space would need for `bytes`.
#[must_use]
pub fn pages_for(bytes: u64) -> u64 {
    bytes.div_ceil(PAGE_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attr_round_trip_and_key_ops() {
        let a = QueueAttr {
            order: Order::Keyed { key_len: 8 },
            sender_id: true,
            max_entry_len: 100,
            head: 8,
            tail: 400,
            entry_count: 3,
            next_seq: 9,
            space_bytes: 4064,
        };
        assert_eq!(QueueAttr::decode(&a.encode()).unwrap(), a);
        assert!(KeyOp::Ge.matches(b"B", b"A"));
        assert!(KeyOp::Ge.matches(b"A", b"A"));
        assert!(!KeyOp::Gt.matches(b"A", b"A"));
        assert!(KeyOp::Lt.matches(b"A", b"B"));
        assert!(KeyOp::Ne.matches(b"A", b"B"));
        assert_eq!(slot_len(0, 1), 56);
        assert_eq!(slot_len(8, 8), 64);
    }
}
