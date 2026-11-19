// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `multipart/form-data` bodies: certificate uploads.

/// One part of the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// The field name.
    pub name: String,
    /// The uploaded file's name, for a file field.
    pub filename: Option<String>,
    /// The part's `Content-Type`.
    pub content_type: Option<String>,
    /// The bytes.
    pub data: Vec<u8>,
}

/// The boundary from a `Content-Type` header, if it is multipart.
pub fn boundary(content_type: &str) -> Option<String> {
    let mut it = content_type.split(';');
    if !it
        .next()?
        .trim()
        .eq_ignore_ascii_case("multipart/form-data")
    {
        return None;
    }
    it.filter_map(|p| p.trim().split_once('='))
        .find(|(k, _)| k.eq_ignore_ascii_case("boundary"))
        .map(|(_, v)| v.trim_matches('"').to_string())
        .filter(|b| !b.is_empty())
}

/// Split `body` at `--boundary` lines; `None` when it is not well formed.
pub fn parse(body: &[u8], boundary: &str) -> Option<Vec<Part>> {
    let delim = format!("--{boundary}").into_bytes();
    let mut at = find(body, &delim, 0)?;
    let mut parts = Vec::new();
    loop {
        at += delim.len();
        if body[at..].starts_with(b"--") {
            return Some(parts);
        }
        at = skip_eol(body, at)?;
        let head_end = find(body, b"\r\n\r\n", at)?;
        let head = std::str::from_utf8(&body[at..head_end]).ok()?;
        let data_start = head_end + 4;
        let mut next_delim = b"\r\n".to_vec();
        next_delim.extend_from_slice(&delim);
        let data_end = find(body, &next_delim, data_start)?;
        parts.push(part(head, body[data_start..data_end].to_vec())?);
        at = data_end + 2;
    }
}

fn part(head: &str, data: Vec<u8>) -> Option<Part> {
    let mut name = None;
    let mut filename = None;
    let mut content_type = None;
    for l in head.split("\r\n") {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        if k.trim().eq_ignore_ascii_case("content-disposition") {
            for p in v.split(';').skip(1) {
                if let Some((pk, pv)) = p.trim().split_once('=') {
                    let pv = pv.trim().trim_matches('"').to_string();
                    match pk.trim() {
                        "name" => name = Some(pv),
                        "filename" => filename = Some(pv),
                        _ => {}
                    }
                }
            }
        } else if k.trim().eq_ignore_ascii_case("content-type") {
            content_type = Some(v.trim().to_string());
        }
    }
    Some(Part {
        name: name?,
        filename,
        content_type,
        data,
    })
}

/// A part's head, as [`Parts`] meets it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartHead {
    /// The field name.
    pub name: String,
    /// The uploaded file's name, for a file field.
    pub filename: Option<String>,
    /// The part's `Content-Type`.
    pub content_type: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum At {
    Preamble,
    Data,
    End,
}

/// A `multipart/form-data` body read a part at a time from a stream
/// (a [`super::request::BodyStream`]): [`Parts::next_part`] gives each
/// part's head, and reading gives its data, so a file of any size
/// passes through a small buffer.
pub struct Parts<R> {
    inner: R,
    /// `\r\n--boundary`.
    delim: Vec<u8>,
    buf: Vec<u8>,
    at: At,
}

/// The most a part's head may take.
const PART_HEAD_MAX: usize = 8 * 1024;

fn invalid(what: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_string())
}

impl<R: std::io::Read> Parts<R> {
    /// The parts of `inner`, delimited by `boundary` ([`boundary`]).
    pub fn new(inner: R, boundary: &str) -> Self {
        Self {
            inner,
            delim: format!("\r\n--{boundary}").into_bytes(),
            // The first delimiter has no line break before it.
            buf: b"\r\n".to_vec(),
            at: At::Preamble,
        }
    }

    /// Read more into the buffer; false at the stream's end.
    fn more(&mut self) -> std::io::Result<bool> {
        let mut chunk = [0u8; 16 * 1024];
        let n = self.inner.read(&mut chunk)?;
        self.buf.extend_from_slice(&chunk[..n]);
        Ok(n > 0)
    }

    /// The next part's head, the rest of the current part skipped;
    /// `None` after the last.
    pub fn next_part(&mut self) -> std::io::Result<Option<PartHead>> {
        match self.at {
            At::End => return Ok(None),
            At::Data => std::io::copy(self, &mut std::io::sink()).map(|_| ())?,
            At::Preamble => loop {
                if let Some(i) = find(&self.buf, &self.delim, 0) {
                    self.buf.drain(..i);
                    break;
                }
                let keep = self.buf.len().saturating_sub(self.delim.len());
                self.buf.drain(..keep);
                if !self.more()? {
                    return Err(invalid("no multipart boundary"));
                }
            },
        }
        // The buffer starts with the delimiter.
        while self.buf.len() < self.delim.len() + 2 {
            if !self.more()? {
                return Err(invalid("multipart body ends early"));
            }
        }
        self.buf.drain(..self.delim.len());
        if self.buf.starts_with(b"--") {
            self.at = At::End;
            return Ok(None);
        }
        let head_end = loop {
            if let Some(i) = find(&self.buf, b"\r\n\r\n", 0) {
                break i;
            }
            if self.buf.len() > PART_HEAD_MAX {
                return Err(invalid("multipart part head too large"));
            }
            if !self.more()? {
                return Err(invalid("multipart body ends early"));
            }
        };
        let head: Vec<u8> = self.buf.drain(..head_end + 4).collect();
        let text = std::str::from_utf8(&head[..head_end])
            .map_err(|_| invalid("multipart part head is not text"))?;
        // The line break after the delimiter.
        let text = text.strip_prefix("\r\n").unwrap_or(text);
        let p = part(text, Vec::new()).ok_or_else(|| invalid("multipart part has no name"))?;
        self.at = At::Data;
        Ok(Some(PartHead {
            name: p.name,
            filename: p.filename,
            content_type: p.content_type,
        }))
    }
}

impl<R: std::io::Read> std::io::Read for Parts<R> {
    /// The current part's data; 0 at its end.
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.at != At::Data || out.is_empty() {
            return Ok(0);
        }
        loop {
            if let Some(i) = find(&self.buf, &self.delim, 0) {
                let n = i.min(out.len());
                out[..n].copy_from_slice(&self.buf[..n]);
                self.buf.drain(..n);
                if i == 0 {
                    self.at = At::Preamble;
                }
                return Ok(n);
            }
            // What cannot be the start of a delimiter is the part's.
            let safe = self.buf.len().saturating_sub(self.delim.len() - 1);
            if safe > 0 {
                let n = safe.min(out.len());
                out[..n].copy_from_slice(&self.buf[..n]);
                self.buf.drain(..n);
                return Ok(n);
            }
            if !self.more()? {
                return Err(invalid("multipart body ends early"));
            }
        }
    }
}

fn skip_eol(b: &[u8], at: usize) -> Option<usize> {
    if b[at..].starts_with(b"\r\n") {
        Some(at + 2)
    } else {
        None
    }
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_fields_and_files() {
        let ct = "multipart/form-data; boundary=\"XyZ\"";
        let b = boundary(ct).unwrap();
        let body = b"--XyZ\r\nContent-Disposition: form-data; name=\"label\"\r\n\r\nweb\r\n\
--XyZ\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.pem\"\r\nContent-Type: application/x-pem-file\r\n\r\n\
-----BEGIN\r\n\x00\xff\r\n--XyZ--\r\n";
        let parts = parse(body, &b).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(
            (parts[0].name.as_str(), parts[0].data.as_slice()),
            ("label", &b"web"[..])
        );
        assert_eq!(parts[1].filename.as_deref(), Some("a.pem"));
        assert_eq!(parts[1].data, b"-----BEGIN\r\n\x00\xff");
        assert_eq!(boundary("text/plain"), None);
        assert_eq!(parse(b"garbage", "XyZ"), None);
    }

    /// One byte at a time: the stream's reads never line up with parts.
    struct Trickle<'a>(&'a [u8]);

    impl std::io::Read for Trickle<'_> {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            let Some((first, rest)) = self.0.split_first() else {
                return Ok(0);
            };
            b[0] = *first;
            self.0 = rest;
            Ok(1)
        }
    }

    #[test]
    fn streams_parts() {
        use std::io::Read;
        let mut file = Vec::new();
        for i in 0..100_000u32 {
            file.push((i % 251) as u8);
        }
        file.extend_from_slice(b"\r\n--Xy not the boundary\r\n");
        let mut body = b"preamble\r\n--XyZ\r\nContent-Disposition: form-data; name=\"label\"\r\n\r\nweb\r\n--XyZ\r\nContent-Disposition: form-data; name=\"file\"; filename=\"big.bin\"\r\n\r\n".to_vec();
        body.extend_from_slice(&file);
        body.extend_from_slice(b"\r\n--XyZ--\r\n");
        for whole in [true, false] {
            let src: Box<dyn Read> = if whole {
                Box::new(&body[..])
            } else {
                Box::new(Trickle(&body))
            };
            let mut p = Parts::new(src, "XyZ");
            let h = p.next_part().unwrap().unwrap();
            assert_eq!(h.name, "label");
            // The label is skipped unread.
            let h = p.next_part().unwrap().unwrap();
            assert_eq!(h.filename.as_deref(), Some("big.bin"));
            let mut got = Vec::new();
            p.read_to_end(&mut got).unwrap();
            assert!(got == file, "{} bytes", got.len());
            assert_eq!(p.next_part().unwrap(), None);
        }
        let mut p = Parts::new(
            &b"--XyZ\r\nContent-Disposition: form-data; name=\"a\"\r\n\r\nx"[..],
            "XyZ",
        );
        p.next_part().unwrap();
        assert!(p.read_to_end(&mut Vec::new()).is_err());
    }
}
