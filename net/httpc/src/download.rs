// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A body streamed to a writer under a size limit, and resumed by
//! range: `download` asks for the bytes after those the caller has and
//! refuses a server that answers from the start ([`Error::RangeIgnored`]);
//! `download_file` appends to a file and, when the range is ignored,
//! starts the file again.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use crate::client::{Request, open};
use crate::wire::{self, Body, Frame, from_io};
use crate::{Error, Result};

/// What a download did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fetched {
    /// 200, 206, or 416 for a download already whole.
    pub status: u16,
    /// The answer's headers.
    pub headers: Vec<(String, String)>,
    /// The offset the bytes written start at.
    pub start: u64,
    /// Bytes written.
    pub written: u64,
    /// The whole body's length, when the server said it.
    pub total: Option<u64>,
}

/// `bytes A-B/T` or `bytes */T`: the first byte (none for `*`) and the
/// total (none for `*`).
fn content_range(v: &str) -> Option<(Option<u64>, Option<u64>)> {
    let rest = v.trim().strip_prefix("bytes")?.trim_start();
    let (range, total) = rest.split_once('/')?;
    let total = match total.trim() {
        "*" => None,
        t => Some(t.parse().ok()?),
    };
    let first = match range.trim() {
        "*" => None,
        r => Some(r.split_once('-')?.0.trim().parse().ok()?),
    };
    Some((first, total))
}

/// Download `req`'s body to `out`: the bytes from offset `from` on (a
/// `Range` asked for when `from` is past 0), the whole body at most
/// `limit` bytes. A body past the limit is refused before a byte is
/// written when the server gives its length, and as soon as it passes
/// the limit otherwise (what was written stays the caller's to
/// discard). A body cut short is an error after what arrived was
/// written: the caller resumes from there. A server that answers a
/// range with the whole body is [`Error::RangeIgnored`], nothing
/// written; a range past the end of a body `from` long is whole (416).
pub fn download(req: &Request, from: u64, limit: u64, out: &mut impl Write) -> Result<Fetched> {
    if from > limit {
        return Err(Error::TooLarge(limit));
    }
    let extra = if from > 0 {
        vec![("Range".to_string(), format!("bytes={from}-"))]
    } else {
        Vec::new()
    };
    let mut o = open(req, &extra)?;
    let (status, headers) = o.head.clone();
    let range = wire::header(&headers, "Content-Range").and_then(content_range);
    let frame = wire::frame(&o.method, &o.head)?;
    let mut f = Fetched {
        status,
        headers,
        start: 0,
        written: 0,
        total: None,
    };
    match status {
        200 if from > 0 => return Err(Error::RangeIgnored),
        200 => {
            f.total = if let Frame::Length(n) = frame {
                Some(n)
            } else {
                None
            }
        }
        206 => {
            let Some((Some(first), total)) = range else {
                return Err(Error::Protocol("a partial answer without its range".into()));
            };
            if first != from {
                return Err(Error::Protocol(format!(
                    "asked for bytes from {from}, sent from {first}"
                )));
            }
            f.start = from;
            f.total = total;
        }
        416 if from > 0 => {
            return match range {
                Some((None, Some(total))) if total == from => {
                    f.start = from;
                    f.total = Some(total);
                    Ok(f)
                }
                _ => Err(Error::Status(416)),
            };
        }
        s => return Err(Error::Status(s)),
    }
    if f.total.is_some_and(|t| t > limit) {
        return Err(Error::TooLarge(limit));
    }
    if let Frame::Length(n) = frame
        && f.start + n > limit
    {
        return Err(Error::TooLarge(limit));
    }
    let room = limit - f.start;
    let mut body = Body::new(&mut o.reader, frame).take(room + 1);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(from_io(e)),
        };
        let n64 = n as u64;
        if f.written + n64 > room {
            return Err(Error::TooLarge(limit));
        }
        out.write_all(&buf[..n])?;
        f.written += n64;
    }
    out.flush()?;
    Ok(f)
}

/// Download `req`'s body into the file at `path`, at most `limit`
/// bytes: resumed from the file's length when it has some, and
/// started again (the file emptied) when the server ignores the range.
pub fn download_file(req: &Request, path: &Path, limit: u64) -> Result<Fetched> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let have = file.metadata()?.len();
    match download(req, have, limit, &mut file) {
        Err(Error::RangeIgnored) => {
            drop(file);
            let mut file = File::create(path)?;
            download(req, 0, limit, &mut file)
        }
        r => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(content_range("bytes 5-9/10"), Some((Some(5), Some(10))));
        assert_eq!(content_range("bytes 5-9/*"), Some((Some(5), None)));
        assert_eq!(content_range("bytes */10"), Some((None, Some(10))));
        assert_eq!(content_range("items 1-2/3"), None);
        assert_eq!(content_range("bytes 5-9"), None);
    }
}
