// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The stream files of `CPYTOIMPF` (`TOSTMF`) and `CPYFRMIMPF`
//! (`FROMSTMF`): the database side makes and reads UTF-8 text, these
//! write it in the stream file's CCSID and read it back. A new file is
//! tagged with `STMFCCSID`, 1208 for `*STMF` (where IBM takes a CCSID
//! from the job); an existing one keeps its tag unless replaced with
//! another.

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::Attr;

use super::{Opt, StmfAut, StmfCcsid, read_stream, write_stream};
use crate::msg::Msg;

/// How `TOSTMF` is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Write {
    /// `STMFOPT`.
    pub opt: Opt,
    /// `STMFCCSID` (or `STMFCODPAG`).
    pub ccsid: StmfCcsid,
    /// `STMFAUT`.
    pub aut: StmfAut,
}

/// A word or a number of `STMFCCSID`/`STMFCODPAG` as typed: `*STMF`,
/// `*PCASCII`, `*STDASCII`, a CCSID; IBM's numbers for the specials.
#[must_use]
pub fn ccsid_word(w: &str) -> Option<StmfCcsid> {
    match w.trim().to_ascii_uppercase().as_str() {
        "" | "*STMF" | "-1" | "0" => Some(StmfCcsid::Stmf),
        "*PCASCII" | "-2" => Some(StmfCcsid::PcAscii),
        "*STDASCII" | "-3" => Some(StmfCcsid::StdAscii),
        n => n.parse::<u16>().ok().filter(|n| *n > 0).map(StmfCcsid::Ccsid),
    }
}

/// `STMFOPT` as typed.
#[must_use]
pub fn opt_word(w: &str) -> Option<Opt> {
    match w.trim().to_ascii_uppercase().as_str() {
        "" | "*NONE" => Some(Opt::None),
        "*ADD" => Some(Opt::Add),
        "*REPLACE" => Some(Opt::Replace),
        _ => None,
    }
}

/// `STMFAUT` as typed.
#[must_use]
pub fn aut_word(w: &str) -> Option<StmfAut> {
    match w.trim().to_ascii_uppercase().as_str() {
        "" | "*DFT" => Some(StmfAut::Dft),
        "*INDIR" => Some(StmfAut::Indir),
        "*FILE" => Some(StmfAut::File),
        "*INDIRFILE" => Some(StmfAut::IndirFile),
        _ => None,
    }
}

fn resolve(c: StmfCcsid, existing: Option<u16>) -> u16 {
    match c {
        StmfCcsid::Stmf => existing.unwrap_or(1208),
        StmfCcsid::PcAscii => 1252,
        StmfCcsid::StdAscii => 819,
        StmfCcsid::Ccsid(n) => n,
    }
}

/// Write UTF-8 `text` to `path`: the message on refusal (`CPDA090`
/// when it exists and `STMFOPT(*NONE)`, `CPDA08A` adding in another
/// CCSID, `CPDA097` a CCSID not converted to, else the `errno`'s).
pub fn write_text(c: &dyn IfsClient, path: &str, text: &[u8], w: Write) -> Result<u16, Msg> {
    let existing = match c.stat(path, true) {
        Ok(s) if s.objtype == "*DIR" => return Err(Msg::errno(Errno::EISDIR, path)),
        Ok(s) => Some(s.ccsid),
        Err(Errno::ENOENT) => None,
        Err(e) => return Err(Msg::errno(e, path)),
    };
    // An empty file counts as absent for *NONE, as IBM's import does.
    let empty = existing.is_some() && c.stat(path, true).is_ok_and(|s| s.size == 0);
    if existing.is_some() && !empty && w.opt == Opt::None {
        return Err(Msg::paths("CPDA090", &[path, path]));
    }
    let target = resolve(w.ccsid, existing);
    if let (Some(have), Opt::Add) = (existing, w.opt)
        && have != target && !empty {
            return Err(Msg::new("CPDA08A")
                .bin4(i64::from(target))
                .bin4(i64::from(have))
                .vary(path));
        }
    let bytes = if target == 1208 || target == 65535 {
        text.to_vec()
    } else {
        ccsid::conv::convert(1208, target, text)
            .map_err(|_| Msg::new("CPDA097").bin4(1208).bin4(i64::from(target)))?
    };
    let to = super::ToStmf {
        stmfopt: if empty { Opt::Replace } else { w.opt },
        aut: w.aut,
        ..super::ToStmf::default()
    };
    let args = super::open_args(existing, &to, target);
    write_stream(c, path, args, &bytes)
        .and_then(|()| match existing {
            Some(have) if have != target => c.set_attr(path, &Attr::Ccsid(target), true),
            _ => Ok(()),
        })
        .map_err(|e| Msg::errno(e, path))?;
    Ok(target)
}

/// Read `path` as UTF-8 text: its bytes converted from its CCSID (or
/// the one `STMFCCSID` names).
pub fn read_text(c: &dyn IfsClient, path: &str, ccsid: StmfCcsid) -> Result<Vec<u8>, Msg> {
    let (data, tag) = read_stream(c, path).map_err(|e| Msg::errno(e, path))?;
    let from = resolve(ccsid, Some(tag));
    if from == 1208 || from == 65535 {
        return Ok(data);
    }
    ccsid::conv::convert(from, 1208, &data)
        .map_err(|_| Msg::new("CPDA097").bin4(i64::from(from)).bin4(1208))
}
