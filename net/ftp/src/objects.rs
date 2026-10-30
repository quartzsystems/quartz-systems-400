// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The objects a session reads and writes, through the IFS: stream
//! files anywhere, members, save files and libraries under
//! `/QSYS.LIB`. A member's records come and go as the IFS's binary
//! stream of them, cut at the record length the file's description
//! gives.

use ifs::client::{CHUNK, IfsClient};
use ifs::ops::flags::{O_APPEND, O_CCSID, O_CREAT, O_INHERITMODE, O_RDONLY, O_TRUNC, O_WRONLY, S_IFDIR, S_IFMT};
use ifs::ops::{OpenArgs, Stat};
use ifs::vfs::qsys::QsysNode;
use ifs::{Errno, Result};

use crate::host::{DbFile, Host};
use crate::names;

/// What a path names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A directory outside `/QSYS.LIB`, or `/QSYS.LIB` itself.
    Dir,
    /// A stream file (or another non-directory outside `/QSYS.LIB`).
    Stream,
    /// A library.
    Library(String),
    /// A database file (`lib`, `file`).
    File(String, String),
    /// A member (`lib`, `file`, `member`).
    Member(String, String, String),
    /// A save file (`lib`, `name`).
    Savf(String, String),
    /// Another object (`lib`, `name`, `*TYPE`).
    Object(String, String, String),
}

impl Kind {
    /// Whether it lists as a directory.
    pub fn is_dir(&self) -> bool {
        matches!(self, Kind::Dir | Kind::Library(_) | Kind::File(..))
    }
}

/// What `path` names, from its place and `stat`.
pub fn kind_of(path: &str, st: &Stat) -> Kind {
    match names::qsys_node(path) {
        Some(QsysNode::Root) | None if st.mode & S_IFMT == S_IFDIR => Kind::Dir,
        None => Kind::Stream,
        Some(QsysNode::Root) => Kind::Dir,
        Some(QsysNode::Lib(l)) => Kind::Library(l),
        Some(QsysNode::Mbr(l, f, m)) => Kind::Member(l, f, m),
        Some(QsysNode::Obj(l, o, t)) => match t.as_str() {
            "*FILE" => Kind::File(l, o),
            "*SAVF" => Kind::Savf(l, o),
            _ => Kind::Object(l, o, t),
        },
    }
}

/// `stat` and what it is.
pub fn look(ifs: &dyn IfsClient, path: &str) -> Result<(Kind, Stat)> {
    let st = ifs.stat(path, true)?;
    Ok((kind_of(path, &st), st))
}

/// A whole object's bytes.
pub fn read_bytes(ifs: &dyn IfsClient, path: &str) -> Result<Vec<u8>> {
    let fd = ifs.open(path, OpenArgs::new(O_RDONLY))?;
    let mut out = Vec::new();
    let r = loop {
        match ifs.read(fd, CHUNK) {
            Ok(b) if b.is_empty() => break Ok(()),
            Ok(b) => out.extend_from_slice(&b),
            Err(e) => break Err(e),
        }
    };
    let c = ifs.close(fd);
    r.and(c).map(|()| out)
}

/// How a write opens its object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Write {
    /// Replace the data, making the object (tagged with the CCSID)
    /// when it is not there.
    Replace(u16),
    /// Add to the end, making the object when it is not there.
    Append(u16),
}

/// Write `data` to `path`.
pub fn write_bytes(ifs: &dyn IfsClient, path: &str, data: &[u8], how: Write) -> Result<()> {
    let (flags, ccsid) = match how {
        Write::Replace(c) => (O_WRONLY | O_CREAT | O_TRUNC, c),
        Write::Append(c) => (O_WRONLY | O_CREAT | O_APPEND, c),
    };
    let args = OpenArgs::new(flags | O_CCSID | O_INHERITMODE)
        .mode(0o777)
        .conv(u32::from(ccsid));
    let fd = ifs.open(path, args)?;
    let mut r = Ok(());
    let mut at = 0;
    while at < data.len() {
        let end = (at + CHUNK).min(data.len());
        match ifs.write(fd, &data[at..end]) {
            Ok(0) => {
                r = Err(Errno::EIO);
                break;
            }
            Ok(n) => at += n,
            Err(e) => {
                r = Err(e);
                break;
            }
        }
    }
    let c = ifs.close(fd);
    r.and(c)
}

/// A member's path.
pub fn member_path(lib: &str, file: &str, mbr: &str) -> String {
    names::qsys_path(&QsysNode::Mbr(lib.into(), file.into(), mbr.into()))
}

/// A file's path.
pub fn file_path(lib: &str, file: &str) -> String {
    names::qsys_path(&QsysNode::Obj(lib.into(), file.into(), "*FILE".into()))
}

/// A save file's path.
pub fn savf_path(lib: &str, name: &str) -> String {
    names::qsys_path(&QsysNode::Obj(lib.into(), name.into(), "*SAVF".into()))
}

/// A member's records and its file's description.
pub fn read_member(host: &mut dyn Host, lib: &str, file: &str, mbr: &str) -> Result<(Vec<Vec<u8>>, DbFile)> {
    let desc = host.file(lib, file).ok_or(Errno::ENOENT)?;
    let bytes = read_bytes(host.ifs(), &member_path(lib, file, mbr))?;
    let len = desc.rec_len.max(1);
    let records = bytes.chunks(len).map(<[u8]>::to_vec).collect();
    Ok((records, desc))
}

/// Replace a member's records (each the record length already), making
/// the member when the file has no such member.
pub fn write_member(host: &mut dyn Host, lib: &str, file: &str, mbr: &str, records: &[Vec<u8>]) -> Result<()> {
    let data = records.concat();
    let path = member_path(lib, file, mbr);
    let ifs = host.ifs();
    let fd = match ifs.open(&path, OpenArgs::new(O_WRONLY | O_TRUNC)) {
        Ok(fd) => fd,
        Err(Errno::ENOENT) => ifs.open(&path, OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(0o777))?,
        Err(e) => return Err(e),
    };
    let mut r = Ok(());
    let mut at = 0;
    while at < data.len() {
        let end = (at + CHUNK).min(data.len());
        match ifs.write(fd, &data[at..end]) {
            Ok(0) => {
                r = Err(Errno::EIO);
                break;
            }
            Ok(n) => at += n,
            Err(e) => {
                r = Err(e);
                break;
            }
        }
    }
    let c = ifs.close(fd);
    r.and(c)
}

/// Whether `path` names something.
pub fn exists(ifs: &dyn IfsClient, path: &str) -> bool {
    ifs.stat(path, true).is_ok()
}
