// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*LIB` payload: the attribute area and the directory tree.

use qsname::Name;

use crate::bytes::{get_u16, get_u64, put_u16, put_u64};
use crate::obj::ObjectId;
use crate::obj::dir::{DirKey, LibDir};
use crate::obj::header::{ATTR_OFF, text, text_of};
use crate::page::pager::PageNo;
use crate::spec::{Layout, field};
use crate::txn::Txn;
use crate::{Result, StoreError};

/// Bytes of the attribute area used.
pub const LIB_ATTR_LEN: usize = 32;

const OFF_DIR_ROOT: usize = 0;
const OFF_LIB_TYPE: usize = 8;
const OFF_CRTAUT: usize = 10;
const OFF_ASP: usize = 20;
const OFF_COUNT: usize = 24;

/// The spec layout.
pub static LIB_ATTR_LAYOUT: Layout = Layout {
    name: "LibAttr",
    size: Some(LIB_ATTR_LEN),
    fields: &[
        field("dir_root", OFF_DIR_ROOT, 8),
        field("lib_type", OFF_LIB_TYPE, 1),
        field("reserved", 9, 1),
        field("create_authority", OFF_CRTAUT, 10),
        field("asp_number", OFF_ASP, 2),
        field("reserved2", 22, 2),
        field("object_count", OFF_COUNT, 8),
    ],
};

/// `*PROD` or `*TEST`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibType {
    /// Production library.
    Prod,
    /// Test library.
    Test,
}

/// The attribute area of a library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibAttr {
    /// Directory tree root.
    pub dir_root: Option<PageNo>,
    /// Library type.
    pub lib_type: LibType,
    /// Default create authority, CP037 (`*SYSVAL`, `*CHANGE`, ...).
    pub create_authority: [u8; 10],
    /// ASP number.
    pub asp_number: u16,
    /// Objects in the directory.
    pub object_count: u64,
}

impl LibAttr {
    /// A new empty library.
    pub fn new(lib_type: LibType, asp_number: u16) -> Result<Self> {
        Ok(Self {
            dir_root: None,
            lib_type,
            create_authority: text("*SYSVAL")?,
            asp_number,
            object_count: 0,
        })
    }

    /// Encode into the attribute area of a header body.
    pub fn encode(&self, body: &mut [u8]) {
        let a = &mut body[ATTR_OFF..ATTR_OFF + LIB_ATTR_LEN];
        a.fill(0);
        put_u64(a, OFF_DIR_ROOT, self.dir_root.unwrap_or(0));
        a[OFF_LIB_TYPE] = match self.lib_type {
            LibType::Prod => 0,
            LibType::Test => 1,
        };
        a[OFF_CRTAUT..OFF_CRTAUT + 10].copy_from_slice(&self.create_authority);
        put_u16(a, OFF_ASP, self.asp_number);
        put_u64(a, OFF_COUNT, self.object_count);
    }

    /// The encoded area, for a patch.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; LIB_ATTR_LEN] {
        let mut body = vec![0u8; ATTR_OFF + LIB_ATTR_LEN];
        self.encode(&mut body);
        body[ATTR_OFF..].try_into().expect("sized")
    }

    /// Decode from a header body.
    #[must_use]
    pub fn decode(body: &[u8]) -> Self {
        let a = &body[ATTR_OFF..ATTR_OFF + LIB_ATTR_LEN];
        let root = get_u64(a, OFF_DIR_ROOT);
        Self {
            dir_root: (root != 0).then_some(root),
            lib_type: if a[OFF_LIB_TYPE] == 1 {
                LibType::Test
            } else {
                LibType::Prod
            },
            create_authority: a[OFF_CRTAUT..OFF_CRTAUT + 10].try_into().expect("10"),
            asp_number: get_u16(a, OFF_ASP),
            object_count: get_u64(a, OFF_COUNT),
        }
    }

    /// The create authority as text.
    #[must_use]
    pub fn create_authority_text(&self) -> String {
        text_of(&self.create_authority)
    }
}

/// A library's header page and attribute area.
pub struct Library {
    /// The library.
    pub id: ObjectId,
    /// Its header page.
    pub page: PageNo,
    /// Its attribute area.
    pub attr: LibAttr,
}

impl Library {
    /// Load a library by ID.
    pub fn load(txn: &mut Txn<'_>, id: ObjectId) -> Result<Self> {
        let page = crate::obj::dir::header_page(txn, id)?;
        let p = crate::page::pager::Pager::read(txn, page)?;
        let h = crate::obj::header::ObjectHeader::decode(p.body(), page)?;
        if h.id.type_code != crate::obj::types::LIB {
            return Err(StoreError::LibraryNotFound {
                name: h.name.to_string(),
            });
        }
        Ok(Self {
            id,
            page,
            attr: LibAttr::decode(p.body()),
        })
    }

    /// The directory tree.
    #[must_use]
    pub fn dir(&self) -> LibDir {
        LibDir::open(self.attr.dir_root, self.id)
    }

    /// Write the attribute area back (root, count) in place.
    pub fn save(&self, txn: &mut Txn<'_>) -> Result<()> {
        txn.patch(self.page, ATTR_OFF, &self.attr.to_bytes())
    }

    /// Look a name up.
    pub fn lookup(
        &self,
        txn: &mut Txn<'_>,
        type_code: u16,
        name: Name,
    ) -> Result<Option<ObjectId>> {
        self.dir().get(txn, &DirKey::new(type_code, name))
    }

    /// Add an entry; the name must be new for its type.
    pub fn add(
        &mut self,
        txn: &mut Txn<'_>,
        type_code: u16,
        name: Name,
        id: ObjectId,
    ) -> Result<()> {
        let mut dir = self.dir();
        if dir
            .insert(txn, DirKey::new(type_code, name), &id)?
            .is_some()
        {
            return Err(StoreError::Corrupt {
                page: self.page,
                what: format!("directory already holds {name}"),
            });
        }
        self.attr.dir_root = dir.root();
        self.attr.object_count += 1;
        self.save(txn)
    }

    /// Remove an entry.
    pub fn remove(&mut self, txn: &mut Txn<'_>, type_code: u16, name: Name) -> Result<ObjectId> {
        let mut dir = self.dir();
        let Some(id) = dir.delete(txn, &DirKey::new(type_code, name))? else {
            return Err(StoreError::Corrupt {
                page: self.page,
                what: format!("directory does not hold {name}"),
            });
        };
        self.attr.dir_root = dir.root();
        self.attr.object_count = self.attr.object_count.saturating_sub(1);
        self.save(txn)?;
        Ok(id)
    }

    /// Entries, in key order (type, then EBCDIC name), optionally one type.
    pub fn list(
        &self,
        txn: &mut Txn<'_>,
        type_code: Option<u16>,
    ) -> Result<Vec<(DirKey, ObjectId)>> {
        let dir = self.dir();
        match type_code {
            Some(t) => {
                let lo = DirKey::new(t, Name::from_bytes([0; 10]));
                let hi = DirKey::new(t, Name::from_bytes([0xFF; 10]));
                dir.range(txn, lo..=hi)?.collect()
            }
            None => dir.entries(txn)?.collect(),
        }
    }

    /// Free the directory tree (when deleting an empty library).
    pub fn free_dir(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        let mut dir = self.dir();
        dir.free_all(txn)?;
        self.attr.dir_root = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::BODY_SIZE;

    #[test]
    fn attr_round_trip() {
        let mut a = LibAttr::new(LibType::Test, 1).unwrap();
        a.dir_root = Some(77);
        a.object_count = 9;
        let mut body = vec![0u8; BODY_SIZE];
        a.encode(&mut body);
        assert_eq!(LibAttr::decode(&body), a);
        assert_eq!(a.create_authority_text(), "*SYSVAL");
        assert_eq!(&a.to_bytes()[..], &body[ATTR_OFF..ATTR_OFF + LIB_ATTR_LEN]);
    }
}
