// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Physical file constraints: unique (`*UNQCST`), primary key
//! (`*PRIKEY`), check (`*CHKCST`) and referential (`*REFCST`), kept as
//! `CONSTRAINT` entries of the file's `*SQLINF` (`docs/DB_FORMAT.md`
//! §8.1) and enforced where records change, whoever changes them.
//!
//! Unique, primary and foreign keys live in one tree per member, header
//! tree root slot 3 (§4): each entry is the constraint's number, two
//! bytes big-endian, then its key, padded to the longest constraint key.
//! Member's `link` keeps it and refuses a repeated unique key; the open
//! data path checks the rest before each change (`enforce`).

pub mod check;
pub mod enforce;
pub mod model;
pub mod ops;

pub use model::{Constraint, Kind, Rule};

use std::ops::Bound;
use std::sync::Arc;

use qsname::Name;
use store::obj::ObjectId;
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::file::FileDesc;
use crate::format::FormatDef;
use crate::path::{self, Path, PathKey};
use crate::sqlinf::{self, tag};

/// Header tree root slot of a member's constraint tree.
pub const CST_ROOT: usize = 3;

/// The constraints of `file`, in the order kept.
pub fn list(txn: &mut Txn<'_>, file: ObjectId) -> Result<Vec<Constraint>> {
    let Some(info) = sqlinf::load(txn, file)? else {
        return Ok(Vec::new());
    };
    info.all(tag::CONSTRAINT).map(Constraint::decode).collect()
}

/// Replace the constraints of `file`.
pub fn store(txn: &mut Txn<'_>, file: ObjectId, csts: &[Constraint], now: u64) -> Result<()> {
    let mut info = sqlinf::load(txn, file)?.unwrap_or_else(|| sqlinf::SqlInfo::new(sqlinf::Kind::Table));
    info.remove(tag::CONSTRAINT);
    for c in csts {
        info.push(tag::CONSTRAINT, c.encode());
    }
    sqlinf::save(txn, file, &info, now)
}

/// One keyed constraint as a member keeps it.
#[derive(Clone, Debug)]
pub struct KeyDef {
    /// Index in [`Active::csts`].
    pub index: usize,
    /// The constraint's number, the tree key's first two bytes.
    pub number: u16,
    /// The file's format keyed on the constraint's columns.
    pub format: FormatDef,
}

/// A member's constraints, ready to be enforced.
#[derive(Clone, Debug)]
pub struct Active {
    /// Every constraint of the file.
    pub csts: Vec<Constraint>,
    /// The keyed ones.
    pub keys: Vec<KeyDef>,
    /// Bytes of a tree key: the number and the longest key.
    pub key_len: usize,
}

/// The file's format keyed on `columns`.
pub fn keyed_format(format: &FormatDef, columns: &[Name]) -> Result<FormatDef> {
    let mut rf = format.format.clone();
    rf.keys.clear();
    for c in columns {
        let (i, f) = rf.field(c).ok_or_else(|| DbError::Definition {
            what: format!("field {c} is not in format {}", rf.name),
        })?;
        rf.keys.push(dds::KeyField {
            name: f.name,
            field: i,
            descend: false,
            transform: if f.data_type.is_numeric() { dds::KeyTransform::Signed } else { dds::KeyTransform::Unsigned },
            noaltseq: false,
            line: 0,
        });
    }
    let mut fd = FormatDef::new(rf, None, None)?;
    fd.null_map_len = format.null_map_len;
    Ok(fd)
}

impl Active {
    /// The constraints of a physical file, `None` when it has none.
    pub fn load(txn: &mut Txn<'_>, file: &FileDesc) -> Result<Option<Arc<Self>>> {
        if file.header.sql.is_none() || file.formats.is_empty() {
            return Ok(None);
        }
        let csts = list(txn, file.id)?;
        if csts.is_empty() {
            return Ok(None);
        }
        Self::build(csts, &file.formats[0]).map(|a| Some(Arc::new(a)))
    }

    /// From the constraints and the file's format.
    pub fn build(csts: Vec<Constraint>, format: &FormatDef) -> Result<Self> {
        let mut keys = Vec::new();
        let mut key_len = 0;
        for (index, c) in csts.iter().enumerate() {
            if !c.kind.keyed() {
                continue;
            }
            let format = keyed_format(format, &c.columns)?;
            key_len = key_len.max(usize::from(format.key_len));
            keys.push(KeyDef { index, number: c.number, format });
        }
        Ok(Self { csts, keys, key_len: key_len + 2 })
    }

    /// The tree over a member's root.
    #[must_use]
    pub fn tree(&self, owner: ObjectId, root: Option<store::PageNo>) -> Path {
        Path { owner, key_len: self.key_len, multi: false, root }
    }

    /// The tree key of constraint `k` for key bytes `key`.
    #[must_use]
    pub fn tree_key(&self, k: &KeyDef, key: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.key_len);
        out.extend_from_slice(&k.number.to_be_bytes());
        out.extend_from_slice(key);
        out.resize(self.key_len, 0);
        out
    }

    /// The keyed constraint named `name`.
    #[must_use]
    pub fn key_of(&self, name: &str) -> Option<&KeyDef> {
        self.keys.iter().find(|k| self.csts[k.index].name == name)
    }

    /// The tree entries a record makes, each with whether it must be
    /// unique: a foreign key with a null part makes none, a unique key
    /// with a null part is not checked (nulls are never equal).
    pub fn entries(&self, rrn: u64, record: &[u8], nulls: &[u8]) -> Result<Vec<(usize, PathKey, bool)>> {
        let mut out = Vec::new();
        for (ki, k) in self.keys.iter().enumerate() {
            let c = &self.csts[k.index];
            let has_null = path::key_has_null(&k.format, nulls);
            if c.kind == Kind::Referential && has_null {
                continue;
            }
            let key = k.key(record, nulls)?;
            out.push((ki, PathKey { key: self.tree_key(k, &key), format: 0, rrn }, c.kind.unique() && !has_null));
        }
        Ok(out)
    }

    /// The RRNs under constraint `k` with key bytes `key`.
    pub fn find(&self, txn: &mut Txn<'_>, tree: &Path, k: &KeyDef, key: &[u8], max: usize) -> Result<Vec<u64>> {
        let tk = self.tree_key(k, key);
        Ok(tree
            .scan(txn, Bound::Included(&tk), Bound::Included(&tk), max)?
            .into_iter()
            .map(|e| e.rrn)
            .collect())
    }
}

/// The system name of the first column, for generated names.
fn first_column(c: &Constraint) -> Option<String> {
    c.columns.first().map(ToString::to_string)
}

/// IBM's name for an unnamed constraint: `Q_<lib>_<file>_<col>_nnnnn`
/// (no column for a check), the number the next free one.
#[must_use]
pub fn generated_name(lib: &str, file: &str, c: &Constraint, taken: &dyn Fn(&str) -> bool) -> String {
    let stem = match (c.kind, first_column(c)) {
        (Kind::Check, _) | (_, None) => format!("Q_{lib}_{file}_"),
        (_, Some(col)) => format!("Q_{lib}_{file}_{col}_"),
    };
    (1..=99_999u32).map(|n| format!("{stem}{n:05}")).find(|n| !taken(n)).unwrap_or_else(|| format!("{stem}99999"))
}

impl KeyDef {
    /// The key bytes of a record: the key fields' parts, without the
    /// null byte a null-capable field's part has in an access path, so a
    /// foreign key and its parent key compare alike.
    pub fn key(&self, record: &[u8], nulls: &[u8]) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        for k in &self.format.format.keys {
            let f = &self.format.format.fields[k.field];
            let v = crate::record::read_field(f, record, nulls, self.format.null_index(k.field))?;
            let mut plain = f.clone();
            plain.allow_null = false;
            path::encode::key_part(&plain, k, &v, &mut out)?;
        }
        Ok(out)
    }
}
