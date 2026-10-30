// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CPYTOSTMF` and `CPYFRMSTMF`: database member records to stream
//! file bytes and back, with CCSID conversion, line ends and tabs. The
//! member is reached through a [`QsysView`] (the store in the service,
//! one in memory in the tests), the stream file through the caller's
//! [`IfsClient`]. A refusal is a diagnostic (`CPDA0xx`, or the
//! `CPFA0xx` of an `errno`) followed by IBM's escape: `CPFA097` (object
//! not copied) for `CPYTOSTMF`, `CPFA095` (stream file not copied) for
//! `CPYFRMSTMF`. Success is `CPCA082` and `CPCA081`, the record count
//! as their third value.

pub mod conv;
pub mod impf;

use ifs::client::{IfsClient, read_all, write_all};
use ifs::ops::flags::{O_APPEND, O_CCSID, O_CREAT, O_EXCL, O_INHERITMODE, O_RDONLY, O_TRUNC, O_WRONLY};
use ifs::ops::{Attr, OpenArgs};
use ifs::vfs::qsys::{MemberData, QsysNode, QsysView, node_of};
use ifs::Errno;
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;

use crate::display::absolute;
use crate::msg::{Msg, Reply};

/// `STMFOPT` and `MBROPT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Opt {
    /// `*NONE`: the target must not exist.
    #[default]
    None,
    /// `*ADD`.
    Add,
    /// `*REPLACE`.
    Replace,
}

impl Opt {
    /// From the definitions' values (1, 2, 3).
    #[must_use]
    pub fn of(n: i64) -> Self {
        match n {
            2 => Self::Add,
            3 => Self::Replace,
            _ => Self::None,
        }
    }
}

/// `CVTDTA`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cvt {
    /// `*AUTO`: convert between the CCSIDs.
    #[default]
    Auto,
    /// `*TBL`: through a conversion table.
    Tbl,
    /// `*NONE`: copy the bytes.
    None,
}

impl Cvt {
    /// From the definitions' values (1, 2, 3).
    #[must_use]
    pub fn of(n: i64) -> Self {
        match n {
            2 => Self::Tbl,
            3 => Self::None,
            _ => Self::Auto,
        }
    }
}

/// `ENDLINFMT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndLin {
    /// `*ALL` (`CPYFRMSTMF`): any line end.
    All,
    /// `*CRLF`.
    #[default]
    Crlf,
    /// `*LF`.
    Lf,
    /// `*CR`.
    Cr,
    /// `*LFCR`.
    Lfcr,
    /// `*FIXED`: no line ends, records at their length.
    Fixed,
}

impl EndLin {
    /// From the definitions' values (1 to 6).
    #[must_use]
    pub fn of(n: i64) -> Self {
        match n {
            1 => Self::All,
            3 => Self::Lf,
            4 => Self::Cr,
            5 => Self::Lfcr,
            6 => Self::Fixed,
            _ => Self::Crlf,
        }
    }
}

/// `STMFCCSID` and `STMFCODPAG`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StmfCcsid {
    /// `*STMF`: the stream file's (a new one: the member's).
    #[default]
    Stmf,
    /// `*PCASCII`.
    PcAscii,
    /// `*STDASCII`.
    StdAscii,
    /// A CCSID (or a code page, for `STMFCODPAG`).
    Ccsid(u16),
}

impl StmfCcsid {
    /// From the definitions' values: -1 `*STMF`, -2 `*PCASCII`, -3
    /// `*STDASCII`, else the number.
    #[must_use]
    pub fn of(n: i64) -> Self {
        match n {
            -2 => Self::PcAscii,
            -3 => Self::StdAscii,
            n if n > 0 => u16::try_from(n).map_or(Self::Stmf, Self::Ccsid),
            _ => Self::Stmf,
        }
    }

    /// The CCSID it means: `existing` for `*STMF` when there is one,
    /// else `dbf`.
    fn resolve(self, existing: Option<u16>, dbf: u16) -> u16 {
        match self {
            Self::Stmf => existing.unwrap_or(dbf),
            Self::PcAscii => conv::ascii_of(dbf, true),
            Self::StdAscii => conv::ascii_of(dbf, false),
            Self::Ccsid(n) => ccsid::conv::ccsid_of_codepage(n),
        }
    }
}

/// `AUT` of `CPYTOSTMF` (`STMFAUT` of `CPYTOIMPF`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StmfAut {
    /// `*DFT`: the owner `*RWX`, nobody else.
    #[default]
    Dft,
    /// `*INDIR`: from the directory.
    Indir,
    /// `*FILE`: as `*DFT` (the member's object authorities are not copied).
    File,
    /// `*INDIRFILE`: as `*INDIR`.
    IndirFile,
}

impl StmfAut {
    /// From `CPYTOSTMF`'s values (1 to 4).
    #[must_use]
    pub fn of(n: i64) -> Self {
        match n {
            2 => Self::Indir,
            3 => Self::File,
            4 => Self::IndirFile,
            _ => Self::Dft,
        }
    }
}

/// Shared by both copies.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Common {
    /// `CVTDTA`.
    pub cvtdta: Cvt,
    /// `DBFCCSID`, `None` for `*FILE`.
    pub dbfccsid: Option<u16>,
    /// `STMFCCSID`.
    pub stmfccsid: StmfCcsid,
    /// `STMFCODPAG`, `None` when not given.
    pub stmfcodpag: Option<StmfCcsid>,
    /// `TBL`, `None` when not given.
    pub tbl: Option<String>,
    /// `ENDLINFMT`.
    pub endlinfmt: EndLin,
}

/// `CPYTOSTMF`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToStmf {
    /// `FROMMBR`.
    pub frommbr: String,
    /// `TOSTMF`.
    pub tostmf: String,
    /// `STMFOPT`.
    pub stmfopt: Opt,
    /// The rest.
    pub common: Common,
    /// `AUT`.
    pub aut: StmfAut,
}

/// `CPYFRMSTMF`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FrmStmf {
    /// `FROMSTMF`.
    pub fromstmf: String,
    /// `TOMBR`.
    pub tombr: String,
    /// `MBROPT`.
    pub mbropt: Opt,
    /// The rest.
    pub common: Common,
    /// `TABEXPN(*YES)`.
    pub tabexpn: bool,
}

/// The library, file and member of a `/QSYS.LIB` member path,
/// relative paths taken against the current directory.
pub fn member_of(c: &dyn IfsClient, path: &str) -> Result<(String, String, String), Errno> {
    let full = absolute(&c.getcwd().unwrap_or_else(|_| "/".into()), path);
    let comps: Vec<String> = full
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    match comps.split_first() {
        Some((first, rest)) if first.eq_ignore_ascii_case("QSYS.LIB") => match node_of(rest)? {
            QsysNode::Mbr(l, f, m) => Ok((l, f, m)),
            _ => Err(Errno::EINVAL),
        },
        _ => Err(Errno::EINVAL),
    }
}

/// The checks both commands make on `CVTDTA`, `TBL` and the two CCSID
/// parameters.
fn check(c: &Common) -> Option<Msg> {
    match (c.cvtdta, &c.tbl) {
        (Cvt::Tbl, None) => return Some(Msg::new("CPDA086")),
        // Conversion tables are not kept on this system.
        (Cvt::Tbl, Some(_)) => return Some(Msg::new("CPDA09B")),
        (_, Some(_)) => return Some(Msg::new("CPDA098")),
        _ => {}
    }
    if c.stmfccsid != StmfCcsid::Stmf && c.stmfcodpag.is_some_and(|p| p != StmfCcsid::Stmf) {
        return Some(Msg::new("CPDA0B9"));
    }
    None
}

/// The stream file's CCSID choice: `STMFCODPAG` when given, else
/// `STMFCCSID`.
fn stmf_choice(c: &Common) -> StmfCcsid {
    match c.stmfcodpag {
        Some(p) if p != StmfCcsid::Stmf => p,
        _ => c.stmfccsid,
    }
}

/// The member's CCSID with `DBFCCSID` applied: given on a file that has
/// a CCSID, it must be the file's (`CPDA08B`); a file of 65535 needs it
/// when data is converted (`CPDA08C`).
fn dbf_ccsid(c: &Common, file: u16, path: &str) -> Result<u16, Msg> {
    match c.dbfccsid {
        Some(d) if file != 65535 && d != file => Err(Msg::new("CPDA08B")
            .bin4(i64::from(d))
            .bin4(i64::from(file))),
        Some(d) => Ok(d),
        None if file == 65535 && c.cvtdta == Cvt::Auto => Err(Msg::path("CPDA08C", path)),
        None => Ok(file),
    }
}

fn has(q: &dyn QsysView, lib: &str, file: &str, who: &Principal, need: &[&str]) -> Result<(), Errno> {
    let have = q.authority(lib, file, "*FILE", who)?;
    let need = Auth::parse(need, false).unwrap_or(Auth::ALL);
    if have.has(need) { Ok(()) } else { Err(Errno::EACCES) }
}

/// `CPYTOSTMF`.
#[allow(clippy::many_single_char_names)] // c, q, a, m: the client, view, arguments, member
pub fn cpytostmf(c: &dyn IfsClient, q: &dyn QsysView, who: &Principal, a: &ToStmf) -> Reply {
    let fail = |m: Msg| Reply::fail(Msg::paths("CPFA097", &[&a.frommbr, &a.tostmf])).diag(m);
    if let Some(m) = check(&a.common) {
        return fail(m);
    }
    let Ok((lib, file, mbr)) = member_of(c, &a.frommbr) else {
        return fail(Msg::path("CPDA082", &a.frommbr));
    };
    let m = match has(q, &lib, &file, who, &["*OBJOPR", "*READ"])
        .and_then(|()| q.read_member(&lib, &file, &mbr))
    {
        Ok(m) => m,
        Err(e) => return fail(Msg::errno(e, &a.frommbr)),
    };
    let dbf = match dbf_ccsid(&a.common, m.ccsid, &a.frommbr) {
        Ok(d) => d,
        Err(msg) => return fail(msg),
    };
    let existing = match c.stat(&a.tostmf, true) {
        Ok(s) if s.objtype == "*DIR" => return fail(Msg::errno(Errno::EISDIR, &a.tostmf)),
        Ok(s) => Some(s.ccsid),
        Err(Errno::ENOENT) => None,
        Err(e) => return fail(Msg::errno(e, &a.tostmf)),
    };
    if existing.is_some() && a.stmfopt == Opt::None {
        return fail(Msg::paths("CPDA090", &[&a.frommbr, &a.tostmf]));
    }
    let choice = stmf_choice(&a.common);
    let target = choice.resolve(existing, dbf);
    if let (Some(have), Opt::Add) = (existing, a.stmfopt)
        && have != target {
            return fail(
                Msg::new("CPDA08A")
                    .bin4(i64::from(target))
                    .bin4(i64::from(have))
                    .vary(&a.tostmf),
            );
        }
    let bytes = match conv::to_stream(&m, dbf, target, a.common.cvtdta, a.common.endlinfmt) {
        Ok(b) => b,
        Err(conv::Unsupported(f, t)) => {
            return fail(Msg::new("CPDA097").bin4(i64::from(f)).bin4(i64::from(t)));
        }
    };
    let r = write_stream(c, &a.tostmf, open_args(existing, a, target), &bytes)
        .and_then(|()| match existing {
            Some(have) if have != target => c.set_attr(&a.tostmf, &Attr::Ccsid(target), true),
            _ => Ok(()),
        });
    match r {
        Ok(()) => Reply::done(
            Msg::paths("CPCA082", &[&a.frommbr, &a.tostmf]).bin4(n64(m.records.len())),
        ),
        Err(e) => fail(Msg::errno(e, &a.tostmf)),
    }
}

fn write_stream(c: &dyn IfsClient, path: &str, args: OpenArgs, data: &[u8]) -> Result<(), Errno> {
    let fd = c.open(path, args)?;
    let r = write_all(c, fd, data);
    let closed = c.close(fd);
    r.and(closed)
}

/// A stream file's bytes and CCSID.
pub fn read_stream(c: &dyn IfsClient, path: &str) -> Result<(Vec<u8>, u16), Errno> {
    let st = c.stat(path, true)?;
    if st.objtype == "*DIR" {
        return Err(Errno::EISDIR);
    }
    let fd = c.open(path, OpenArgs::new(O_RDONLY))?;
    let r = read_all(c, fd);
    let _ = c.close(fd);
    Ok((r?, st.ccsid))
}

/// `CPYFRMSTMF`.
#[allow(clippy::many_single_char_names)] // c, q, a, m: the client, view, arguments, member
pub fn cpyfrmstmf(c: &dyn IfsClient, q: &dyn QsysView, who: &Principal, a: &FrmStmf) -> Reply {
    let fail = |m: Msg| Reply::fail(Msg::paths("CPFA095", &[&a.fromstmf, &a.tombr])).diag(m);
    if a.tabexpn && a.common.endlinfmt == EndLin::Fixed {
        return Reply::fail(Msg::new("CPFA08A"));
    }
    if let Some(m) = check(&a.common) {
        return fail(m);
    }
    let Ok((lib, file, mbr)) = member_of(c, &a.tombr) else {
        return fail(Msg::path("CPDA082", &a.tombr));
    };
    let (data, tag) = match read_stream(c, &a.fromstmf) {
        Ok(v) => v,
        Err(e) => return fail(Msg::errno(e, &a.fromstmf)),
    };
    let need: &[&str] = if a.mbropt == Opt::Replace {
        &["*OBJOPR", "*ADD", "*DLT"]
    } else {
        &["*OBJOPR", "*ADD"]
    };
    if let Err(e) = has(q, &lib, &file, who, need) {
        return fail(Msg::errno(e, &a.tombr));
    }
    let exists = match q.members(&lib, &file) {
        Ok(v) => v.iter().any(|e| e.name.eq_ignore_ascii_case(&mbr)),
        Err(e) => return fail(Msg::errno(e, &a.tombr)),
    };
    if exists && a.mbropt == Opt::None {
        return fail(Msg::paths("CPDA089", &[&a.fromstmf, &a.tombr]));
    }
    if !exists
        && let Err(e) = q.create_member(&lib, &file, &mbr, who) {
            return fail(Msg::errno(e, &a.tombr));
        }
    let m: MemberData = match q.read_member(&lib, &file, &mbr) {
        Ok(m) => m,
        Err(e) => return fail(Msg::errno(e, &a.tombr)),
    };
    let dbf = match dbf_ccsid(&a.common, m.ccsid, &a.tombr) {
        Ok(d) => d,
        Err(msg) => return fail(msg),
    };
    let stmf = stmf_choice(&a.common).resolve(Some(tag), dbf);
    let made = match conv::to_records(
        &data,
        stmf,
        dbf,
        m.record_len,
        a.common.cvtdta,
        a.common.endlinfmt,
        a.tabexpn,
    ) {
        Ok(r) => r,
        Err(conv::Unsupported(f, t)) => {
            return fail(Msg::new("CPDA097").bin4(i64::from(f)).bin4(i64::from(t)));
        }
    };
    let count = made.records.len();
    let records = if a.mbropt == Opt::Add {
        let mut v = m.records;
        v.extend(made.records);
        v
    } else {
        made.records
    };
    if let Err(e) = q.write_member(&lib, &file, &mbr, records) {
        return fail(Msg::errno(e, &a.tombr));
    }
    if made.truncated > 0 {
        return Reply::fail(
            Msg::paths("CPFA096", &[&a.fromstmf, &a.tombr])
                .bin4(n64(made.truncated))
                .bin4(n64(m.record_len)),
        );
    }
    Reply::done(Msg::paths("CPCA081", &[&a.fromstmf, &a.tombr]).bin4(n64(count)))
}

/// How the stream file is opened for writing: appended to, truncated,
/// or made new (owner `*RWX` only, or the directory's with `*INDIR`),
/// tagged with `ccsid`.
fn open_args(existing: Option<u16>, a: &ToStmf, ccsid: u16) -> OpenArgs {
    let mut flags = O_WRONLY | O_CCSID;
    let mut mode = 0o700;
    match (existing, a.stmfopt) {
        (Some(_), Opt::Add) => flags |= O_APPEND,
        (Some(_), _) => flags |= O_TRUNC,
        (None, _) => {
            flags |= O_CREAT | O_EXCL;
            if matches!(a.aut, StmfAut::Indir | StmfAut::IndirFile) {
                flags |= O_INHERITMODE;
                mode = 0;
            }
        }
    }
    OpenArgs::new(flags).mode(mode).conv(u32::from(ccsid))
}

/// A count as a message value.
fn n64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}
