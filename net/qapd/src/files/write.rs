// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WriteFile` (docs/QAP.md, "Writing with conversion"): the file opened
//! as the FTP server writes stream files (a new one tagged with `ccsid`,
//! its data authorities from its directory), `data` converted per
//! request to the file's CCSID when `data_ccsid` is given, and written
//! at the mode's place. A byte-range lock of another job over it is
//! `InUse`. In a mixed file a converted write at the end drops the
//! file's last shift-in and the data's first shift-out, so text written
//! in pieces makes the same file as one write; it reads that last byte,
//! so it opens the file for reading too when the profile may, else
//! writes the data as converted.

use ccsid::conv::UTF8;
use ifs::Errno;
use ifs::client::{CHUNK, IfsClient};
use ifs::lock::range::RangeKind;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_EXCL, O_INHERITMODE, O_RDWR, O_TRUNC, O_WRONLY};
use qap_proto::ops::ifs::{WriteFile, WriteMode, Written};

use super::read::converter;
use super::shift::{self, SI, SO};
use super::{Done, Stop, held};

/// The open flags for `mode`.
fn flags(mode: WriteMode) -> u32 {
    match mode {
        WriteMode::Create => O_CREAT | O_EXCL | O_CCSID | O_INHERITMODE,
        WriteMode::Replace => O_CREAT | O_TRUNC | O_CCSID | O_INHERITMODE,
        WriteMode::Update => 0,
        WriteMode::Append => O_CREAT | O_CCSID | O_INHERITMODE,
    }
}

/// `WriteFile`.
pub(super) fn write(c: &dyn IfsClient, req: &WriteFile) -> Done {
    if req.data_ccsid == Some(0) {
        return Err(Stop::Invalid);
    }
    let path = req.path.as_str();
    let fail = |e: Errno| Stop::errno(e, path);
    let new_ccsid = if req.ccsid == 0 { UTF8 } else { req.ccsid };
    // The file's CCSID: its own when it is there, else the one given.
    let file_ccsid = match (req.mode, c.stat(path, true)) {
        (WriteMode::Create, _) | (_, Err(_)) => new_ccsid,
        (_, Ok(s)) => s.ccsid,
    };
    let converted = req.data_ccsid.filter(|d| *d != file_ccsid);
    let data = match converted {
        Some(from) => {
            let mut conv = converter(from, file_ccsid)?;
            let mut out = Vec::with_capacity(req.data.len());
            conv.feed(&req.data, &mut out);
            conv.finish(&mut out);
            out
        }
        None => req.data.clone(),
    };
    let join = converted.is_some()
        && shift::is_mixed(file_ccsid)
        && data.first() == Some(&SO)
        && matches!(req.mode, WriteMode::Update | WriteMode::Append);
    let args = |access: u32| OpenArgs::new(access | flags(req.mode)).mode(0o777).conv(u32::from(new_ccsid));
    let (fd, readable) = if join {
        match c.open(path, args(O_RDWR)) {
            Ok(fd) => (fd, true),
            Err(Errno::EACCES) => (c.open(path, args(O_WRONLY)).map_err(fail)?, false),
            Err(e) => return Err(fail(e)),
        }
    } else {
        (c.open(path, args(O_WRONLY)).map_err(fail)?, false)
    };
    let r = write_open(c, fd, req, &data, join && readable);
    let closed = c.close(fd);
    let size = r?;
    closed.map_err(fail)?;
    Ok(ipc::encode(&Written { size }))
}

fn write_open(c: &dyn IfsClient, fd: i32, req: &WriteFile, data: &[u8], join: bool) -> Result<u64, Stop> {
    let fail = |e: Errno| Stop::errno(e, &req.path);
    let size = c.fstat(fd).map_err(fail)?.size;
    let mut at = match req.mode {
        WriteMode::Create | WriteMode::Replace => 0,
        WriteMode::Update => req.offset,
        WriteMode::Append => size,
    };
    held(c, fd, RangeKind::Write, at, data.len() as u64).map_err(fail)?;
    let mut data = data;
    if join && at == size && at > 0 && c.pread(fd, 1, at - 1).map_err(fail)? == [SI] {
        at -= 1;
        data = &data[1..];
    }
    while !data.is_empty() {
        let n = c.pwrite(fd, &data[..data.len().min(CHUNK)], at).map_err(fail)?;
        if n == 0 {
            return Err(fail(Errno::ENOSPC));
        }
        data = &data[n..];
        at += n as u64;
    }
    Ok(c.fstat(fd).map_err(fail)?.size)
}
