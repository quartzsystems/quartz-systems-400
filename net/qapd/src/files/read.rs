// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ReadFile` (docs/QAP.md, "Reading with conversion"): the file opened
//! for reading as displaying it opens it (`*R`; a share mode or a
//! byte-range write lock of another job is `InUse`), the range read in
//! chunks and, with `as_ccsid`, converted. A reply holds whole
//! characters: `next_offset` is just past the last byte converted. At
//! the end of the file a partial character left is converted as one
//! substitution, so a client reading on from `next_offset` always ends.
//! A mixed file is converted from the shift state it has at `offset`:
//! the memo's when the previous reply stopped there, else scanned from
//! the start.
//!
//! The body is built in place (`size`, `next_offset`, then `data`
//! appended as it is read), so a large range is not copied again.

use ccsid::conv::{BINARY, Converter, UTF8};
use ifs::client::{CHUNK, IfsClient};
use ifs::lock::range::RangeKind;
use ifs::ops::flags::{O_RDONLY, S_IFDIR, S_IFMT};
use ifs::ops::{OpenArgs, Stat};
use ifs::{Errno, Result as IfsResult};
use qap_proto::ops::ifs::{FileData, ReadFile};

use super::shift::{self, Shift};
use super::{Done, Memo, Stop, held};

/// Where `data`'s length and `next_offset` sit in the encoded body.
const NEXT_AT: usize = 8;
const LEN_AT: usize = 16;
const HEAD: usize = 20;

/// A converter from `from` to `to`; `Error` when there is none (either
/// is binary, or not known).
pub(super) fn converter(from: u16, to: u16) -> Result<Converter, Stop> {
    if from == BINARY || to == BINARY {
        return Err(Stop::conversion(from, to));
    }
    Converter::new(from, to).map_err(|_| Stop::conversion(from, to))
}

/// Read `[from, to)` of `fd` in chunks, each handed to `f`; the bytes
/// read (fewer at the end of the file).
fn chunks(
    c: &dyn IfsClient,
    fd: i32,
    from: u64,
    to: u64,
    mut f: impl FnMut(&[u8]),
) -> IfsResult<u64> {
    let mut at = from;
    while at < to {
        let want = usize::try_from(to - at).unwrap_or(usize::MAX).min(CHUNK);
        let b = c.pread(fd, want, at)?;
        if b.is_empty() {
            break;
        }
        at += b.len() as u64;
        f(&b);
    }
    Ok(at - from)
}

/// The shift state of a mixed file at `offset`, scanned from the start.
fn scan(c: &dyn IfsClient, fd: i32, path: &str, ccsid: u16, offset: u64) -> Result<Shift, Stop> {
    let mut conv = Converter::new(ccsid, UTF8).map_err(|_| Stop::conversion(ccsid, UTF8))?;
    let mut sink = Vec::new();
    chunks(c, fd, 0, offset, |b| {
        conv.feed(b, &mut sink);
        sink.clear();
    })
    .map_err(|e| Stop::errno(e, path))?;
    Ok(shift::input(&conv))
}

/// `ReadFile`.
pub(super) fn read(c: &dyn IfsClient, memo: &Memo, req: &ReadFile) -> Done {
    if req.as_ccsid == Some(0) {
        return Err(Stop::Invalid);
    }
    let path = req.path.as_str();
    let fd = c
        .open(path, OpenArgs::new(O_RDONLY))
        .map_err(|e| Stop::errno(e, path))?;
    let r = read_open(c, fd, memo, req);
    let _ = c.close(fd);
    r
}

fn read_open(c: &dyn IfsClient, fd: i32, memo: &Memo, req: &ReadFile) -> Done {
    let path = req.path.as_str();
    let fail = |e: Errno| Stop::errno(e, path);
    let st: Stat = c.fstat(fd).map_err(fail)?;
    if st.mode & S_IFMT == S_IFDIR {
        return Err(fail(Errno::EISDIR));
    }
    held(c, fd, RangeKind::Read, req.offset, req.length).map_err(fail)?;
    let size = st.size;
    let start = req.offset;
    let end = match req.length {
        0 => size,
        n => size.min(start.saturating_add(n)),
    };
    let mut body = ipc::encode(&FileData {
        size,
        next_offset: start,
        data: Vec::new(),
    });
    let next = match req.as_ccsid.filter(|t| *t != st.ccsid) {
        // As stored.
        None => {
            body.reserve(usize::try_from(end.saturating_sub(start)).unwrap_or(0));
            let n = chunks(c, fd, start, end, |b| body.extend_from_slice(b)).map_err(fail)?;
            start + n
        }
        Some(to) => {
            let mut conv = converter(st.ccsid, to)?;
            let mixed = shift::is_mixed(st.ccsid);
            if mixed && start > 0 {
                let s = match memo.get(path, start, st.mtime) {
                    Some(s) => s,
                    None => scan(c, fd, path, st.ccsid, start)?,
                };
                conv = shift::start(conv, s);
            }
            let n = chunks(c, fd, start, end, |b| conv.feed(b, &mut body)).map_err(fail)?;
            let mut next = start + n - conv.pending() as u64;
            if start + n >= size && conv.pending() > 0 {
                // The file ends inside a character.
                conv.finish(&mut body);
                next = start + n;
            } else if shift::is_mixed(to) {
                shift::flush(&mut conv, &mut body);
            }
            if mixed {
                memo.put(path, next, st.mtime, shift::input(&conv));
            }
            next
        }
    };
    let len = u32::try_from(body.len() - HEAD).map_err(|_| fail(Errno::EOVERFLOW))?;
    body[NEXT_AT..LEN_AT].copy_from_slice(&next.to_le_bytes());
    body[LEN_AT..HEAD].copy_from_slice(&len.to_le_bytes());
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_in_place_is_file_data() {
        let mut body = ipc::encode(&FileData {
            size: 9,
            next_offset: 0,
            data: Vec::new(),
        });
        assert_eq!(body.len(), HEAD);
        body.extend_from_slice(b"abc");
        body[NEXT_AT..LEN_AT].copy_from_slice(&7u64.to_le_bytes());
        body[LEN_AT..HEAD].copy_from_slice(&3u32.to_le_bytes());
        let d: FileData = ipc::decode(&body).unwrap();
        assert_eq!(
            (d.size, d.next_offset, d.data.as_slice()),
            (9, 7, &b"abc"[..])
        );
    }

    #[test]
    fn binary_does_not_convert() {
        assert!(matches!(converter(65535, 1208), Err(Stop::Text(..))));
        assert!(matches!(converter(37, 65535), Err(Stop::Text(..))));
        assert!(matches!(converter(37, 4321), Err(Stop::Text(..))));
        assert!(converter(37, 1208).is_ok());
    }
}
