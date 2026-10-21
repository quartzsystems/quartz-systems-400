// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*BNDDIR` payload: a header and fixed entries in order.

use store::spec::{Layout, field};

use crate::bin::{R, W};
use crate::error::Result;

/// Size of `BndDirHdr`.
pub const BNDDIR_HDR_LEN: usize = 16;
/// Size of `BndDirEntry`.
pub const BNDDIR_ENTRY_LEN: usize = 24;
const MAGIC: &[u8; 8] = b"QS400BND";
const VERSION: u32 = 1;

/// Layout of `BndDirHdr`, for the spec test.
pub static HDR_LAYOUT: Layout = Layout {
    name: "BndDirHdr",
    size: Some(BNDDIR_HDR_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 4),
        field("count", 12, 4),
    ],
};

/// Layout of `BndDirEntry`, for the spec test.
pub static ENTRY_LAYOUT: Layout = Layout {
    name: "BndDirEntry",
    size: Some(BNDDIR_ENTRY_LEN),
    fields: &[
        field("name", 0, 10),
        field("library", 10, 10),
        field("kind", 20, 1),
        field("activation", 21, 1),
        field("reserved", 22, 2),
    ],
};

/// What a binding directory entry names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BndEntryKind {
    /// `*SRVPGM`.
    SrvPgm,
    /// `*MODULE`.
    Module,
}

impl BndEntryKind {
    /// The object type name.
    #[must_use]
    pub fn type_name(self) -> &'static str {
        match self {
            Self::SrvPgm => "*SRVPGM",
            Self::Module => "*MODULE",
        }
    }
}

/// An entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BndDirEntry {
    /// Object name.
    pub name: String,
    /// Library, or `*LIBL`.
    pub lib: String,
    /// Service program or module.
    pub kind: BndEntryKind,
    /// `ACTIVATION(*DEFER)`: activate the service program on first use.
    pub defer: bool,
}

/// A `*BNDDIR` object's payload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BndDir {
    /// Entries in order.
    pub entries: Vec<BndDirEntry>,
}

impl BndDir {
    /// Add an entry at the end, or at `position` (`ADDBNDDIRE POSITION`).
    /// A duplicate of an existing entry is ignored, as the command does.
    pub fn add(&mut self, e: BndDirEntry, position: Option<usize>) {
        if self
            .entries
            .iter()
            .any(|x| x.name == e.name && x.lib == e.lib && x.kind == e.kind)
        {
            return;
        }
        match position {
            Some(p) if p < self.entries.len() => self.entries.insert(p, e),
            _ => self.entries.push(e),
        }
    }

    /// Remove the entries matching a name, kind and optionally library;
    /// returns how many went.
    pub fn remove(&mut self, name: &str, lib: Option<&str>, kind: BndEntryKind) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|e| !(e.name == name && e.kind == kind && lib.is_none_or(|l| e.lib == l)));
        before - self.entries.len()
    }

    /// The payload bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W::default();
        w.raw(MAGIC);
        w.u32(VERSION);
        w.u32(crate::bin::len32(self.entries.len()));
        for e in &self.entries {
            w.name(&e.name);
            w.name(&e.lib);
            w.u8(match e.kind {
                BndEntryKind::SrvPgm => 0,
                BndEntryKind::Module => 1,
            });
            w.bool(e.defer);
            w.zeros(2);
        }
        w.out
    }

    /// Read a payload.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = R::new(bytes, "*BNDDIR");
        if r.take(8)? != MAGIC {
            return Err(r.err("wrong magic"));
        }
        let version = r.u32()?;
        if version != VERSION {
            return Err(r.err(format!("payload version {version}")));
        }
        let count = r.u32()? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let name = r.name()?;
            let lib = r.name()?;
            let kind = match r.u8()? {
                0 => BndEntryKind::SrvPgm,
                1 => BndEntryKind::Module,
                v => return Err(r.err(format!("{v} is not an entry kind"))),
            };
            let defer = r.bool()?;
            r.take(2)?;
            entries.push(BndDirEntry {
                name,
                lib,
                kind,
                defer,
            });
        }
        Ok(Self { entries })
    }
}
