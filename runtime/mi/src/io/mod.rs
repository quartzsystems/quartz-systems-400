// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the IR's file, data area and data queue instructions reach:
//! an [`IoSource`] opens files and runs record operations, a
//! [`DataSource`] holds data areas and data queues. [`store::StoreIo`]
//! runs over the database; [`MemData`] keeps data objects in memory.

use std::collections::{HashMap, VecDeque};

pub mod display;
pub mod infds;
pub mod spool;
pub mod store;
mod wire;

pub use wire::{OwnedKey, OwnedRequest};

/// How a file is opened: the module's file declaration, as the
/// runtime hands it over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenSpec {
    /// Object name.
    pub file: String,
    /// Library, `*LIBL` by default.
    pub lib: String,
    /// Usage.
    pub usage: qir::FileUsage,
    /// Keyed access.
    pub keyed: bool,
    /// Format level identifiers to check, when the module asked.
    pub lvlchk: Option<Vec<(String, String)>>,
    /// A member, when one was named.
    pub member: Option<String>,
    /// The activation group opening it, for the overrides that reach it.
    pub actgrp: String,
}

/// A failed open or data operation: the RPG status and the message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IoFail {
    /// Five-digit RPG status, as a number.
    pub status: u16,
    /// Message identifier.
    pub msgid: String,
    /// Message data, EBCDIC.
    pub data: Vec<u8>,
}

impl IoFail {
    /// The status of a display operation whose request the operator
    /// ended (System Request option 2): the program is cancelled to
    /// its control boundary instead of seeing an error.
    pub const REQUEST_ENDED: u16 = u16::MAX;

    /// A failure with a message and blank-padded name fields.
    #[must_use]
    pub fn new(status: u16, msgid: &str, names: &[&str]) -> Self {
        let mut data = Vec::new();
        for n in names {
            let mut b = ccsid::to_ebcdic(n).unwrap_or_default();
            b.resize(10, 0x40);
            data.extend(b);
        }
        Self {
            status,
            msgid: msgid.into(),
            data,
        }
    }
}

/// A record operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoOp {
    /// `READ`.
    Read,
    /// `READP`.
    ReadP,
    /// `READE`.
    ReadE,
    /// `READPE`.
    ReadPE,
    /// `CHAIN`.
    Chain,
    /// `SETLL`.
    SetLL,
    /// `SETGT`.
    SetGT,
    /// `WRITE`.
    Write,
    /// `UPDATE`.
    Update,
    /// `DELETE`.
    Delete,
    /// `UNLOCK`.
    Unlock,
    /// `FEOD`.
    Feod,
}

impl IoOp {
    /// From the code generated code passes (`codegen::rt::db_op_code`).
    #[must_use]
    pub fn from_code(c: i64) -> Option<Self> {
        Some(match c {
            1 => Self::Read,
            2 => Self::ReadP,
            3 => Self::ReadE,
            4 => Self::ReadPE,
            5 => Self::Chain,
            6 => Self::SetLL,
            7 => Self::SetGT,
            8 => Self::Write,
            9 => Self::Update,
            10 => Self::Delete,
            11 => Self::Unlock,
            12 => Self::Feod,
            _ => return None,
        })
    }

    /// The operation code name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Read => "READ",
            Self::ReadP => "READP",
            Self::ReadE => "READE",
            Self::ReadPE => "READPE",
            Self::Chain => "CHAIN",
            Self::SetLL => "SETLL",
            Self::SetGT => "SETGT",
            Self::Write => "WRITE",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
            Self::Unlock => "UNLOCK",
            Self::Feod => "FEOD",
        }
    }
}

/// The key of a positioning operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyRef<'a> {
    /// The key fields' bytes in record layout, a prefix allowed.
    Fields(&'a [u8]),
    /// A relative record number.
    Rrn(u64),
    /// None: `READE`/`READPE` keep the key of the last positioning.
    None,
    /// `*START`: before the first record.
    Start,
    /// `*END`: after the last record.
    End,
    /// `*LOVAL`: below every key.
    LoVal,
    /// `*HIVAL`: above every key.
    HiVal,
}

/// A request to an open file.
#[derive(Clone, Copy, Debug)]
pub struct IoRequest<'a> {
    /// The operation.
    pub op: IoOp,
    /// The record to write or update.
    pub record: Option<&'a [u8]>,
    /// The key or record number.
    pub key: KeyRef<'a>,
    /// The record format of a multi-format file.
    pub format: Option<&'a str>,
    /// Read without a lock.
    pub nolock: bool,
}

/// What an operation reported: the feedback of the open data path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IoResult {
    /// Five-digit RPG status, as a number.
    pub status: u16,
    /// Message identifier of a failure.
    pub msgid: Option<String>,
    /// `%EOF`.
    pub eof: bool,
    /// `%FOUND`.
    pub found: bool,
    /// `%EQUAL`.
    pub equal: bool,
    /// Relative record number touched.
    pub rrn: u64,
    /// Encoded key touched.
    pub key: Vec<u8>,
    /// Record format touched.
    pub format: String,
    /// Member.
    pub member: String,
    /// The record read.
    pub record: Option<Vec<u8>>,
}

/// An open file's standing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileInfo {
    /// Object name.
    pub file: String,
    /// Library it was found in.
    pub lib: String,
    /// Member opened.
    pub member: String,
    /// First record format.
    pub format: String,
    /// Record length.
    pub rec_len: u32,
    /// How long to wait for a locked record, milliseconds: 0 for
    /// `*IMMED`, -1 for `*NOMAX`.
    pub wait_ms: i64,
}

/// Where files are opened and records read and written.
pub trait IoSource {
    /// Open; a handle and the file's standing.
    fn open(&mut self, spec: &OpenSpec) -> Result<(u32, FileInfo), IoFail>;
    /// One operation on an open file.
    fn io(&mut self, handle: u32, req: &IoRequest<'_>) -> IoResult;
    /// Close.
    fn close(&mut self, handle: u32) -> IoResult;
}

/// Data area types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataAreaKind {
    /// `*CHAR`.
    Char,
    /// `*DEC`.
    Dec,
    /// `*LGL`.
    Lgl,
}

/// A data area's value: characters, or a packed decimal of the area's
/// length and decimal positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataAreaValue {
    /// Type.
    pub kind: DataAreaKind,
    /// Length (characters or digits).
    pub len: u32,
    /// Decimal positions.
    pub decimals: u32,
    /// The value bytes.
    pub value: Vec<u8>,
}

impl DataAreaValue {
    /// A character area of `n` blanks.
    #[must_use]
    pub fn chars(n: u32) -> Self {
        Self {
            kind: DataAreaKind::Char,
            len: n,
            decimals: 0,
            value: vec![0x40; n as usize],
        }
    }

    /// A decimal area holding zero.
    #[must_use]
    pub fn decimal(digits: u32, decimals: u32) -> Self {
        let n = digits as usize / 2 + 1;
        let mut v = vec![0; n];
        v[n - 1] = 0x0F;
        Self {
            kind: DataAreaKind::Dec,
            len: digits,
            decimals,
            value: v,
        }
    }
}

/// Where data areas and data queues live.
pub trait DataSource {
    /// `dtaara.lock`.
    fn dtaara_lock(&mut self, name: &str, lib: &str) -> Result<(), IoFail>;
    /// `dtaara.unlock`.
    fn dtaara_unlock(&mut self, name: &str, lib: &str) -> Result<(), IoFail>;
    /// `dtaara.read`.
    fn dtaara_read(&mut self, name: &str, lib: &str) -> Result<DataAreaValue, IoFail>;
    /// `dtaara.write`: the whole value.
    fn dtaara_write(&mut self, name: &str, lib: &str, value: &DataAreaValue) -> Result<(), IoFail>;
    /// `dtaq.snd`.
    fn dtaq_send(&mut self, name: &str, lib: &str, data: &[u8]) -> Result<(), IoFail>;
    /// `dtaq.rcv`: the oldest entry, or none within `wait` seconds
    /// (nothing waits here).
    fn dtaq_receive(&mut self, name: &str, lib: &str, wait: i64)
    -> Result<Option<Vec<u8>>, IoFail>;
}

/// Data areas and queues in memory; `*LDA` is preset.
pub struct MemData {
    /// Data areas by `(library, name)`.
    pub areas: HashMap<(String, String), DataAreaValue>,
    /// Data queues by `(library, name)`, oldest entry first.
    pub queues: HashMap<(String, String), VecDeque<Vec<u8>>>,
    /// Locked data areas.
    pub locked: Vec<(String, String)>,
}

impl Default for MemData {
    fn default() -> Self {
        let mut areas = HashMap::new();
        areas.insert(
            ("*LIBL".to_string(), "*LDA".to_string()),
            DataAreaValue::chars(1024),
        );
        Self {
            areas,
            queues: HashMap::new(),
            locked: Vec::new(),
        }
    }
}

impl MemData {
    fn key<T>(
        map: &HashMap<(String, String), T>,
        name: &str,
        lib: &str,
    ) -> Option<(String, String)> {
        if lib == "*LIBL" || lib == "*CURLIB" || lib.is_empty() {
            map.keys().find(|(_, n)| n == name).cloned()
        } else {
            let k = (lib.to_string(), name.to_string());
            map.contains_key(&k).then_some(k)
        }
    }

    /// Add a data area.
    pub fn add_area(&mut self, lib: &str, name: &str, v: DataAreaValue) {
        self.areas.insert((lib.into(), name.into()), v);
    }

    /// Add an empty data queue.
    pub fn add_queue(&mut self, lib: &str, name: &str) {
        self.queues
            .insert((lib.into(), name.into()), VecDeque::new());
    }
}

fn no_area(name: &str, lib: &str) -> IoFail {
    IoFail::new(1299, "CPF1015", &[name, lib])
}

impl DataSource for MemData {
    fn dtaara_lock(&mut self, name: &str, lib: &str) -> Result<(), IoFail> {
        let k = Self::key(&self.areas, name, lib).ok_or_else(|| no_area(name, lib))?;
        if !self.locked.contains(&k) {
            self.locked.push(k);
        }
        Ok(())
    }

    fn dtaara_unlock(&mut self, name: &str, lib: &str) -> Result<(), IoFail> {
        let k = Self::key(&self.areas, name, lib).ok_or_else(|| no_area(name, lib))?;
        self.locked.retain(|x| *x != k);
        Ok(())
    }

    fn dtaara_read(&mut self, name: &str, lib: &str) -> Result<DataAreaValue, IoFail> {
        let k = Self::key(&self.areas, name, lib).ok_or_else(|| no_area(name, lib))?;
        Ok(self.areas[&k].clone())
    }

    fn dtaara_write(&mut self, name: &str, lib: &str, value: &DataAreaValue) -> Result<(), IoFail> {
        let k = Self::key(&self.areas, name, lib).ok_or_else(|| no_area(name, lib))?;
        let a = self.areas.get_mut(&k).expect("found");
        let mut v = value.value.clone();
        v.resize(
            a.value.len(),
            if a.kind == DataAreaKind::Char {
                0x40
            } else {
                0
            },
        );
        a.value = v;
        Ok(())
    }

    fn dtaq_send(&mut self, name: &str, lib: &str, data: &[u8]) -> Result<(), IoFail> {
        let k = Self::key(&self.queues, name, lib)
            .ok_or_else(|| IoFail::new(1299, "CPF9801", &[name, lib]))?;
        self.queues
            .get_mut(&k)
            .expect("found")
            .push_back(data.to_vec());
        Ok(())
    }

    fn dtaq_receive(
        &mut self,
        name: &str,
        lib: &str,
        _wait: i64,
    ) -> Result<Option<Vec<u8>>, IoFail> {
        let k = Self::key(&self.queues, name, lib)
            .ok_or_else(|| IoFail::new(1299, "CPF9801", &[name, lib]))?;
        Ok(self.queues.get_mut(&k).expect("found").pop_front())
    }
}
