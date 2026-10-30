// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Objects in a transfer's form, for the server's `RETR`/`STOR` and the
//! client's `PUT`/`GET` alike: an object's bytes as they go on the data
//! connection, and data from it written into a stream file, a member
//! (with the file made when it is not there, as IBM's server makes
//! it), or a save file.

use ifs::Errno;
use ifs::vfs::qsys::QsysNode;
use servers::attrs::ftp::CrtCcsid;

use crate::host::Host;
use crate::names;
use crate::objects::{self, Kind, Write as How};
use crate::reply::Reply;
use crate::xlate::{self, Codes, Rep};

/// The record length of a file binary data makes.
pub const BINARY_RCDLEN: usize = 512;
/// A save file's record length: its data comes in whole records.
pub const SAVF_RECORD: usize = 528;

/// How a transfer's data looks.
#[derive(Clone, Copy, Debug)]
pub struct Form {
    /// The representation.
    pub rep: Rep,
    /// The CCSIDs text travels in.
    pub codes: Codes,
    /// Trailing blanks off records sent.
    pub trim: bool,
    /// The CCSID of files made.
    pub crtccsid: CrtCcsid,
    /// The user profile's CCSID (`CRTCCSID(*USER)`), 0 for none.
    pub user_ccsid: u16,
}

/// How messages name what a path is.
pub fn describe(path: &str) -> String {
    match names::qsys_node(path) {
        Some(QsysNode::Mbr(l, f, m)) => format!("member {m} of file {f} in library {l}"),
        Some(QsysNode::Obj(l, o, t)) if t == "*FILE" => format!("file {o} in library {l}"),
        Some(QsysNode::Obj(l, o, t)) => format!("{t} object {o} in library {l}"),
        Some(QsysNode::Lib(l)) => format!("library {l}"),
        _ => path.to_string(),
    }
}

/// The text of an IFS error.
pub fn errno_text(e: Errno) -> String {
    match e {
        Errno::ENOENT => "it does not exist".into(),
        Errno::EACCES | Errno::EPERM => "not authorized".into(),
        Errno::EEXIST => "it exists already".into(),
        Errno::ENOTDIR => "a part of the path is not a directory".into(),
        Errno::EISDIR => "it is a directory".into(),
        Errno::ENOTEMPTY | Errno::EBUSY => "it is not empty or is in use".into(),
        other => format!("{other:?}"),
    }
}

/// A two-line reply: what happened, and that the transfer ended.
pub fn ended(code: u16, why: String) -> Reply {
    Reply::lines(code, vec![why, "The transfer ended.".into()])
}

/// Why a missing member or file cannot be found.
pub fn missing(host: &mut dyn Host, path: &str, e: Errno) -> Reply {
    if let Some(QsysNode::Mbr(l, f, m)) = names::qsys_node(path) {
        if !objects::exists(host.ifs(), &names::library_dir(&l)) {
            return Reply::new(550, format!("Library {l} not found."));
        }
        if !objects::exists(host.ifs(), &objects::file_path(&l, &f)) {
            return Reply::new(550, format!("File {f} in library {l} not found."));
        }
        return ended(426, format!("Member {m} of file {f} in library {l} not found."));
    }
    Reply::new(550, format!("{path} cannot be used: {}.", errno_text(e)))
}

/// An object's bytes as they go out in `form`.
#[allow(clippy::many_single_char_names)]
pub fn fetch(host: &mut dyn Host, form: &Form, path: &str) -> Result<Vec<u8>, Reply> {
    let (k, st) = match objects::look(host.ifs(), path) {
        Ok(x) => x,
        Err(e) => return Err(missing(host, path, e)),
    };
    match k {
        Kind::Stream => objects::read_bytes(host.ifs(), path)
            .map(|d| xlate::stream_out(form.rep, form.codes, st.ccsid, &d))
            .map_err(|e| Reply::new(550, format!("{path} cannot be read: {}.", errno_text(e)))),
        Kind::Member(l, f, m) => match objects::read_member(host, &l, &f, &m) {
            Ok((recs, desc)) => Ok(xlate::member_out(form.rep, form.codes, desc.ccsid, &recs, form.trim)),
            Err(e) => Err(Reply::new(550, format!("{} cannot be read: {}.", describe(path), errno_text(e)))),
        },
        Kind::Savf(..) if form.rep != Rep::Image => Err(Reply::new(504, "Save files go in binary: send TYPE I first.")),
        Kind::Savf(..) | Kind::Object(..) => objects::read_bytes(host.ifs(), path)
            .map_err(|e| Reply::new(550, format!("{} cannot be read: {}.", describe(path), errno_text(e)))),
        Kind::File(l, f) => {
            // A file stands for its member of the same name.
            fetch(host, form, &objects::member_path(&l, &f, &f))
        }
        Kind::Dir | Kind::Library(_) => Err(Reply::new(550, format!("{} is a directory or library.", describe(path)))),
    }
}

/// Write data that came in `form` to `path`: added to the end with
/// `append`, from byte `rest` of a stream file. Whether lines were cut
/// to fit a member's records.
pub fn put(host: &mut dyn Host, form: &Form, path: &str, data: &[u8], append: bool, rest: u64) -> Result<bool, String> {
    let (rep, codes) = (form.rep, form.codes);
    match objects::look(host.ifs(), path) {
        Ok((Kind::Stream, st)) => {
            let (bytes, _) = xlate::stream_in(rep, codes, Some(st.ccsid), data);
            let all = if rest > 0 {
                let mut old = objects::read_bytes(host.ifs(), path).map_err(errno_text)?;
                old.truncate(usize::try_from(rest).unwrap_or(usize::MAX));
                old.extend_from_slice(&bytes);
                old
            } else {
                bytes
            };
            let h = if append && rest == 0 { How::Append(st.ccsid) } else { How::Replace(st.ccsid) };
            objects::write_bytes(host.ifs(), path, &all, h)
                .map(|()| false)
                .map_err(errno_text)
        }
        Ok((Kind::Member(l, f, m), _)) => put_member(host, form, &l, &f, &m, data, append),
        Ok((Kind::Savf(..), _)) => {
            if rep != Rep::Image {
                return Err("a save file takes binary data (TYPE I)".into());
            }
            if !data.len().is_multiple_of(SAVF_RECORD) {
                return Err("the data is not a save file".into());
            }
            objects::write_bytes(host.ifs(), path, data, How::Replace(xlate::BINARY_CCSID))
                .map(|()| false)
                .map_err(errno_text)
        }
        Ok((Kind::Object(..), _)) => objects::write_bytes(host.ifs(), path, data, How::Replace(xlate::BINARY_CCSID))
            .map(|()| false)
            .map_err(errno_text),
        Ok(_) => Err("it is a directory or library".into()),
        Err(_) => match names::qsys_node(path) {
            Some(QsysNode::Mbr(l, f, m)) => {
                if !objects::exists(host.ifs(), &objects::file_path(&l, &f)) {
                    create_file(host, form, &l, &f, &m, data)?;
                }
                put_member(host, form, &l, &f, &m, data, false)
            }
            Some(_) => Err("only members can be made there".into()),
            None => {
                let (bytes, ccsid) = xlate::stream_in(rep, codes, None, data);
                objects::write_bytes(host.ifs(), path, &bytes, How::Replace(ccsid))
                    .map(|()| false)
                    .map_err(errno_text)
            }
        },
    }
}

fn put_member(host: &mut dyn Host, form: &Form, l: &str, f: &str, m: &str, data: &[u8], append: bool) -> Result<bool, String> {
    let desc = host.file(l, f).ok_or_else(|| format!("file {f} in library {l} cannot be used"))?;
    if desc.logical {
        return Err("a logical file cannot be written".into());
    }
    let mut recs = Vec::new();
    if append && let Ok((old, _)) = objects::read_member(host, l, f, m) {
        recs = old;
    }
    recs.extend(xlate::member_in(form.rep, form.codes, desc.ccsid, desc.rec_len, data));
    let cut = xlate::too_long(form.rep, form.codes, desc.rec_len, data);
    objects::write_member(host, l, f, m, &recs)
        .map(|()| cut)
        .map_err(errno_text)
}

/// The CCSID of a file data makes, as `CRTCCSID` says.
fn create_ccsid(host: &mut dyn Host, form: &Form) -> u16 {
    let job = host.job_ccsid();
    match form.crtccsid {
        CrtCcsid::Calc => match form.rep {
            Rep::Ccsid(c) if xlate::is_ebcdic(c) => c,
            _ => job,
        },
        CrtCcsid::User => Some(form.user_ccsid).filter(|c| *c != 0 && *c != 65535).unwrap_or(job),
        CrtCcsid::Sysval => host
            .sysval("QCCSID")
            .and_then(|v| v.trim().parse::<u16>().ok())
            .filter(|c| *c != 65535)
            .unwrap_or(job),
        CrtCcsid::Of(c) => c,
    }
}

/// Make the physical file data needs: records as long as the longest
/// line (512 bytes for binary data), in the CCSID `CRTCCSID` says.
fn create_file(host: &mut dyn Host, form: &Form, l: &str, f: &str, m: &str, data: &[u8]) -> Result<(), String> {
    let len = match form.rep {
        Rep::Image => BINARY_RCDLEN,
        rep => xlate::longest_line(rep, form.codes, data).clamp(1, 32766),
    };
    let ccsid = create_ccsid(host, form);
    host.command(&format!(
        "CRTPF FILE({l}/{f}) RCDLEN({len}) MBR({m}) MAXMBRS(*NOMAX) SIZE(*NOMAX) CCSID({ccsid})"
    ))
    .map_err(|e| e.text)
}

/// Move a member to another file of the library, making that file like
/// the first when it is not there (`RNTO` across files).
pub fn move_member(host: &mut dyn Host, l: &str, f: &str, m: &str, f2: &str, m2: &str) -> Result<(), String> {
    let desc = host.file(l, f).ok_or_else(|| format!("file {f} in library {l} cannot be used"))?;
    let (recs, _) = objects::read_member(host, l, f, m).map_err(errno_text)?;
    if !objects::exists(host.ifs(), &objects::file_path(l, f2)) {
        let cmd = if desc.source {
            format!("CRTSRCPF FILE({l}/{f2}) RCDLEN({}) MBR(*NONE) CCSID({})", desc.rec_len + 12, desc.ccsid)
        } else {
            format!(
                "CRTPF FILE({l}/{f2}) RCDLEN({}) MBR(*NONE) MAXMBRS(*NOMAX) CCSID({})",
                desc.rec_len, desc.ccsid
            )
        };
        host.command(&cmd).map_err(|e| e.text)?;
    }
    if objects::exists(host.ifs(), &objects::member_path(l, f2, m2)) {
        return Err(format!("member {m2} of file {f2} exists already"));
    }
    objects::write_member(host, l, f2, m2, &recs).map_err(errno_text)?;
    host.ifs().unlink(&objects::member_path(l, f, m)).map_err(errno_text)
}
