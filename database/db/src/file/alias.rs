// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL aliases: a `*FILE` of attribute `DDMF` naming a file and, if it
//! likes, a member. See `docs/DB_FORMAT.md` §2, `AliasAttr`.

use qsname::Name;
use store::obj::ops::{self, CreateParams};
use store::obj::{JobHandle, ObjectId, types};
use store::txn::Txn;

use super::CreateFileOpts;
use crate::error::{DbError, Result};
use crate::spec::{Layout, field};

/// Magic at offset 0.
pub const MAGIC: &[u8; 8] = b"QS400ALS";
/// Version this code writes.
pub const VERSION: u16 = 1;
/// Bytes of the attribute area used.
pub const ATTR_LEN: usize = 64;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "AliasAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 2),
        field("library", 10, 10),
        field("file", 20, 10),
        field("member", 30, 10),
        field("reserved", 40, 24),
    ],
};

/// What an alias names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AliasDef {
    /// The file's library; `None` for the library list.
    pub library: Option<Name>,
    /// The file.
    pub file: Name,
    /// The member; `None` for the first.
    pub member: Option<Name>,
}

/// Ten bytes of a name, blanks for none.
fn name_bytes(n: Option<Name>) -> [u8; 10] {
    n.map_or([ccsid::SPACE; 10], |n| *n.as_bytes())
}

fn name_at(b: &[u8], at: usize) -> Option<Name> {
    let bytes: [u8; 10] = b[at..at + 10].try_into().expect("10 bytes");
    (bytes != [ccsid::SPACE; 10] && bytes != [0; 10]).then(|| Name::from_bytes(bytes))
}

impl AliasDef {
    /// The attribute bytes.
    #[must_use]
    pub fn encode(&self) -> [u8; ATTR_LEN] {
        let mut b = [0u8; ATTR_LEN];
        b[..8].copy_from_slice(MAGIC);
        b[8..10].copy_from_slice(&VERSION.to_le_bytes());
        b[10..20].copy_from_slice(&name_bytes(self.library));
        b[20..30].copy_from_slice(self.file.as_bytes());
        b[30..40].copy_from_slice(&name_bytes(self.member));
        b
    }

    /// From the attribute bytes; `None` when they are not an alias's.
    #[must_use]
    pub fn decode(b: &[u8]) -> Option<Self> {
        if b.len() < ATTR_LEN || &b[..8] != MAGIC {
            return None;
        }
        Some(Self {
            library: name_at(b, 10),
            file: name_at(b, 20)?,
            member: name_at(b, 30),
        })
    }
}

/// `CREATE ALIAS`: the `*FILE` in `library`.
pub fn create_alias(
    txn: &mut Txn<'_>,
    library: ObjectId,
    name: Name,
    def: &AliasDef,
    opts: &CreateFileOpts,
) -> Result<ObjectId> {
    let params = CreateParams {
        library,
        name,
        type_code: types::FILE,
        attribute: "DDMF".into(),
        text: opts.text.clone(),
        owner: opts.owner.clone(),
        system: opts.system.clone(),
        now: opts.now,
        by_system: false,
    };
    let id = ops::create(txn, &params)?;
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 0, &def.encode())?;
    Ok(id)
}

/// What the `*FILE` names when it is an alias.
pub fn alias_of(txn: &mut Txn<'_>, id: ObjectId) -> Result<Option<AliasDef>> {
    let (page, h) = ops::read_header(txn, id)?;
    if h.id.type_code != types::FILE {
        return Ok(None);
    }
    Ok(AliasDef::decode(&ops::read_attr(txn, page)?))
}

/// `DROP ALIAS`: the alias goes, the file it names stays.
pub fn delete_alias(txn: &mut Txn<'_>, job: JobHandle, id: ObjectId) -> Result<()> {
    crate::auth::check(txn, "obj.delete", id)?;
    if alias_of(txn, id)?.is_none() {
        let (_, h) = ops::read_header(txn, id)?;
        return Err(DbError::Definition {
            what: format!("{} is not an alias", h.name),
        });
    }
    ops::delete(txn, job, id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let d = AliasDef {
            library: None,
            file: Name::parse("ASSETS").unwrap(),
            member: Some(Name::parse("M2026").unwrap()),
        };
        assert_eq!(AliasDef::decode(&d.encode()), Some(d));
        assert_eq!(AliasDef::decode(&[0; ATTR_LEN]), None);
    }
}
