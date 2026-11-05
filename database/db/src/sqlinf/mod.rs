// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL information: the SQL attributes of an object (its SQL name,
//! definition text, constraints, triggers, routine, sequence, variable or
//! type) in a `*SQLINF` it owns, named by its header's `sql` link; and a
//! library's SQL name index. See `docs/DB_FORMAT.md` §8.

pub mod body;
pub mod entry;
pub mod names;
pub mod program;
pub mod seq;
pub mod var;
pub mod routine;

use store::obj::ops::{self, CreateParams};
use store::obj::space::Space;
use store::obj::{ObjectId, types};
use store::txn::Txn;

pub use entry::{Entry, Kind, tag};

use crate::error::Result;

/// The SQL information of one object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlInfo {
    /// What SQL object the owner is.
    pub kind: Kind,
    /// The entries, in the order written.
    pub entries: Vec<Entry>,
}

impl SqlInfo {
    /// Empty information of `kind`.
    #[must_use]
    pub fn new(kind: Kind) -> Self {
        Self { kind, entries: Vec::new() }
    }

    /// The first entry with `tag`.
    #[must_use]
    pub fn first(&self, tag: u16) -> Option<&[u8]> {
        self.entries.iter().find(|e| e.tag == tag).map(|e| e.body.as_slice())
    }

    /// Every entry with `tag`, in order.
    pub fn all(&self, tag: u16) -> impl Iterator<Item = &[u8]> {
        self.entries.iter().filter(move |e| e.tag == tag).map(|e| e.body.as_slice())
    }

    /// Replace every entry with `tag` by one holding `body`.
    pub fn set(&mut self, tag: u16, body: Vec<u8>) {
        self.entries.retain(|e| e.tag != tag);
        self.entries.push(Entry { tag, body });
    }

    /// Add an entry.
    pub fn push(&mut self, tag: u16, body: Vec<u8>) {
        self.entries.push(Entry { tag, body });
    }

    /// Drop every entry with `tag`.
    pub fn remove(&mut self, tag: u16) {
        self.entries.retain(|e| e.tag != tag);
    }

    /// The SQL name, when one is kept.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        self.first(tag::NAME).map(|b| String::from_utf8_lossy(b).into_owned())
    }

    /// The definition text, when kept.
    #[must_use]
    pub fn text(&self) -> Option<String> {
        self.first(tag::TEXT).map(|b| String::from_utf8_lossy(b).into_owned())
    }
}

/// The SQL information of `owner`, `None` when it has none.
pub fn load(txn: &mut Txn<'_>, owner: ObjectId) -> Result<Option<SqlInfo>> {
    let (_, h) = ops::read_header(txn, owner)?;
    if h.sql.is_none() {
        return Ok(None);
    }
    let (page, _) = ops::read_header(txn, h.sql)?;
    let attr = ops::read_attr(txn, page)?;
    let (kind, count, len) = entry::decode_attr(&attr)?;
    let (space, _) = Space::load(txn, h.sql)?;
    let bytes = space.read(txn, 0, usize::try_from(len).expect("fits"))?;
    Ok(Some(SqlInfo { kind, entries: entry::decode(&bytes, count)? }))
}

/// Write `info` as `owner`'s SQL information, making its `*SQLINF` the
/// first time.
pub fn save(txn: &mut Txn<'_>, owner: ObjectId, info: &SqlInfo, now: u64) -> Result<()> {
    let (owner_page, mut h) = ops::read_header(txn, owner)?;
    if h.sql.is_none() {
        let params = CreateParams {
            library: owner,
            name: h.name,
            type_code: types::SQLINF,
            attribute: String::new(),
            text: String::new(),
            owner: store::obj::header::text_of(&h.owner),
            system: store::obj::header::text_of(&h.system),
            now,
            by_system: true,
        };
        h.sql = ops::create_owned(txn, &params, owner)?;
        ops::write_header(txn, owner_page, &h)?;
    }
    let body = entry::encode(&info.entries);
    let space = Space::truncate(txn, h.sql, 0)?;
    drop(space);
    let mut space = Space::ensure(txn, h.sql, body.len() as u64, 0)?;
    space.write_relocating(txn, 0, &body)?;
    let (page, _) = ops::read_header(txn, h.sql)?;
    let count = u32::try_from(info.entries.len()).expect("entries fit");
    ops::write_attr(txn, page, 0, &entry::encode_attr(info.kind, count, body.len() as u64))?;
    Ok(())
}

/// Drop `owner`'s SQL information.
pub fn clear(txn: &mut Txn<'_>, job: store::obj::JobHandle, owner: ObjectId) -> Result<()> {
    let (page, mut h) = ops::read_header(txn, owner)?;
    if h.sql.is_none() {
        return Ok(());
    }
    let sql = h.sql;
    h.sql = ObjectId::NONE;
    ops::write_header(txn, page, &h)?;
    ops::delete(txn, job, sql)?;
    Ok(())
}
