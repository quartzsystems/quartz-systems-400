// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What DFU asks the storage service: a file's formats and keys, the
//! job's current library, DFU programs, and the records themselves
//! through the open data path the service keeps for the job (the same
//! remote I/O a program's file operations use).

use dfu::Field;
use mi::io::{FileInfo, IoFail, IoOp, IoRequest, IoResult, IoSource, KeyRef, OpenSpec};
use stg::Client;
use stg::remote::RemoteIo;

use crate::console::host::Host;

/// A record format: name, record length and fields.
#[derive(Clone, Debug)]
pub struct Format {
    /// Name.
    pub name: String,
    /// Record length.
    pub len: usize,
    /// Text.
    pub text: String,
    /// Fields in order, the key fields marked.
    pub fields: Vec<Field>,
    /// Key field names in key order.
    pub keys: Vec<String>,
}

/// A data file as DFU sees it.
#[derive(Clone, Debug)]
pub struct DataFile {
    /// Name.
    pub name: String,
    /// The library it was found in.
    pub lib: String,
    /// Keyed access.
    pub keyed: bool,
    /// Its formats.
    pub formats: Vec<Format>,
}

/// The job's current library, `QGPL` when it has none.
pub fn curlib(host: &Host) -> String {
    host.link
        .borrow_mut()
        .with(Client::rtvjoba)
        .libl
        .cur
        .unwrap_or_else(|| "QGPL".into())
}

/// `*CURLIB` as the job's current library; other names as they are.
pub fn real_lib(host: &Host, lib: &str) -> String {
    if lib.eq_ignore_ascii_case("*CURLIB") {
        curlib(host)
    } else {
        lib.to_string()
    }
}

/// A data file, `None` when it is not there or is not a database file.
pub fn file(host: &Host, lib: &str, name: &str) -> Option<DataFile> {
    let lib = real_lib(host, lib);
    let info = host
        .link
        .borrow_mut()
        .with(|l| l.file(lib.clone(), name.into()))?;
    if info.source || !matches!(info.attr.as_str(), "PF" | "LF") {
        return None;
    }
    let formats = host
        .link
        .borrow_mut()
        .with(|l| l.formats(info.lib.clone(), name.into()));
    let keys = host
        .link
        .borrow_mut()
        .with(|l| l.dfu_keys(info.lib.clone(), name.into()))
        .unwrap_or_default();
    let formats = formats
        .into_iter()
        .map(|f| {
            let k: Vec<String> = keys
                .iter()
                .find(|(n, _)| *n == f.name)
                .map(|(_, k)| k.clone())
                .unwrap_or_default();
            Format {
                len: f.rec_len as usize,
                text: f.text,
                fields: f
                    .fields
                    .into_iter()
                    .map(|x| Field {
                        key: k.contains(&x.name),
                        name: x.name,
                        ty: x.ty,
                        pos: x.pos,
                        len: x.len,
                        digits: x.digits,
                        decimals: x.decimals,
                    })
                    .collect(),
                keys: k,
                name: f.name,
            }
        })
        .collect();
    Some(DataFile {
        name: name.to_string(),
        lib: info.lib,
        keyed: info.keyed,
        formats,
    })
}

/// A DFU program's library and definition.
pub fn program(host: &Host, lib: &str, name: &str) -> Result<Option<(String, dfu::Program)>, String> {
    let lib = real_lib(host, lib);
    let got = host
        .link
        .borrow_mut()
        .with(|l| l.dfu_pgm(lib, name.into()))?;
    match got {
        None => Ok(None),
        Some((real, text)) => dfu::Program::decode(&text)
            .map(|p| Some((real, p)))
            .map_err(|e| e.to_string()),
    }
}

/// Store a DFU program; its library.
pub fn save_program(host: &Host, lib: &str, name: &str, p: &dfu::Program) -> Result<String, String> {
    let lib = real_lib(host, lib);
    let text = format!("DFU program over {}", p.file);
    host.link
        .borrow_mut()
        .with(|l| l.dfu_pgm_put(lib, name.into(), p.encode(), text))
}

/// Delete a DFU program.
pub fn delete_program(host: &Host, lib: &str, name: &str) -> Result<(), String> {
    let lib = real_lib(host, lib);
    host.link
        .borrow_mut()
        .with(|l| l.dfu_pgm_delete(lib, name.into()))
}

/// An open member: its handle on the service's open data path.
pub struct Open {
    io: RemoteIo,
    handle: u32,
    /// The open's standing: member and first format.
    pub info: FileInfo,
}

impl Open {
    /// Open a member for update (or input when `read_only`), keyed when
    /// the file is.
    pub fn new(host: &Host, f: &DataFile, member: Option<String>, read_only: bool) -> Result<Self, IoFail> {
        let mut io = RemoteIo::new(host.link.clone());
        let spec = OpenSpec {
            file: f.name.clone(),
            lib: f.lib.clone(),
            usage: if read_only {
                qir::FileUsage::Input
            } else {
                qir::FileUsage::All
            },
            keyed: f.keyed,
            lvlchk: None,
            member,
            actgrp: "*DFTACTGRP".into(),
            commit: false,
        };
        let (handle, info) = io.open(&spec)?;
        Ok(Self { io, handle, info })
    }

    fn op(&mut self, op: IoOp, key: KeyRef<'_>, record: Option<&[u8]>, format: Option<&str>, nolock: bool) -> IoResult {
        self.io.io(
            self.handle,
            &IoRequest {
                op,
                record,
                key,
                format,
                nolock,
                nulls: None,
            },
        )
    }

    /// The record at a key (or record number), locked for update unless
    /// `nolock`.
    pub fn chain(&mut self, key: KeyRef<'_>, nolock: bool) -> IoResult {
        self.op(IoOp::Chain, key, None, None, nolock)
    }

    /// Read on from the last record, forward or back.
    pub fn step(&mut self, back: bool, nolock: bool) -> IoResult {
        self.op(
            if back { IoOp::ReadP } else { IoOp::Read },
            KeyRef::None,
            None,
            None,
            nolock,
        )
    }

    /// Position before a key or record number.
    pub fn position(&mut self, key: KeyRef<'_>, after: bool) -> IoResult {
        self.op(
            if after { IoOp::SetGT } else { IoOp::SetLL },
            key,
            None,
            None,
            true,
        )
    }

    /// Write a new record of a format.
    pub fn write(&mut self, rec: &[u8], format: &str) -> IoResult {
        self.op(IoOp::Write, KeyRef::None, Some(rec), Some(format), false)
    }

    /// Update the record last read.
    pub fn update(&mut self, rec: &[u8], format: &str) -> IoResult {
        self.op(IoOp::Update, KeyRef::None, Some(rec), Some(format), false)
    }

    /// Delete the record last read.
    pub fn delete(&mut self) -> IoResult {
        self.op(IoOp::Delete, KeyRef::None, None, None, false)
    }

    /// Release the record lock.
    pub fn unlock(&mut self) {
        let _ = self.op(IoOp::Unlock, KeyRef::None, None, None, false);
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        let _ = self.io.close(self.handle);
    }
}
