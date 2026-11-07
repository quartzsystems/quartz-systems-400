// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Range` requests (RFC 9110 §14): one byte range of a file, so an
//! interrupted download resumes where it stopped. Several ranges in
//! one request are answered with the whole file, as the RFC allows.

/// What to send of a body of a known length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    /// All of it: `200`.
    Full,
    /// Bytes `start` to `end`, both included: `206`.
    Part(u64, u64),
    /// No byte of it: `416`.
    Unsatisfiable,
}

impl Range {
    /// The status to answer with.
    pub fn status(self) -> u16 {
        match self {
            Range::Full => 200,
            Range::Part(..) => 206,
            Range::Unsatisfiable => 416,
        }
    }

    /// Offset and length to send of a body of `len` bytes.
    pub fn span(self, len: u64) -> (u64, u64) {
        match self {
            Range::Full => (0, len),
            Range::Part(s, e) => (s, e - s + 1),
            Range::Unsatisfiable => (0, 0),
        }
    }

    /// The `Content-Range` header's value, if the answer has one.
    pub fn content_range(self, len: u64) -> Option<String> {
        match self {
            Range::Full => None,
            Range::Part(s, e) => Some(format!("bytes {s}-{e}/{len}")),
            Range::Unsatisfiable => Some(format!("bytes */{len}")),
        }
    }
}

/// The range a request asks of a body of `len` bytes: `range` its
/// `Range` header, `if_range` its `If-Range`, `validators` the body's
/// current `ETag` and `Last-Modified` (an `If-Range` naming neither
/// means the body changed: all of it).
#[allow(clippy::many_single_char_names)]
pub fn parse(range: Option<&str>, if_range: Option<&str>, validators: &[&str], len: u64) -> Range {
    let Some(r) = range else { return Range::Full };
    if let Some(v) = if_range
        && !validators.iter().any(|c| c.trim() == v.trim())
    {
        return Range::Full;
    }
    let Some(spec) = r.trim().strip_prefix("bytes=") else { return Range::Full };
    if spec.contains(',') {
        return Range::Full;
    }
    let Some((a, b)) = spec.trim().split_once('-') else { return Range::Full };
    let (a, b) = (a.trim(), b.trim());
    let part = if a.is_empty() {
        // The last `b` bytes.
        let Ok(n) = b.parse::<u64>() else { return Range::Full };
        if n == 0 || len == 0 {
            return Range::Unsatisfiable;
        }
        (len.saturating_sub(n), len - 1)
    } else {
        let Ok(s) = a.parse::<u64>() else { return Range::Full };
        let e = if b.is_empty() {
            len.saturating_sub(1)
        } else {
            match b.parse::<u64>() {
                Ok(e) if e >= s => e.min(len.saturating_sub(1)),
                _ => return Range::Full,
            }
        };
        if s >= len {
            return Range::Unsatisfiable;
        }
        (s, e)
    };
    Range::Part(part.0, part.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse(None, None, &[], 10), Range::Full);
        assert_eq!(parse(Some("bytes=4-"), None, &[], 10), Range::Part(4, 9));
        assert_eq!(parse(Some("bytes=2-5"), None, &[], 10), Range::Part(2, 5));
        assert_eq!(parse(Some("bytes=2-50"), None, &[], 10), Range::Part(2, 9));
        assert_eq!(parse(Some("bytes=-3"), None, &[], 10), Range::Part(7, 9));
        assert_eq!(parse(Some("bytes=10-"), None, &[], 10), Range::Unsatisfiable);
        assert_eq!(parse(Some("bytes=1-2,4-5"), None, &[], 10), Range::Full);
        assert_eq!(parse(Some("items=1-2"), None, &[], 10), Range::Full);
        assert_eq!(parse(Some("bytes=5-2"), None, &[], 10), Range::Full);
        // If-Range: the same file resumes, a changed one starts again.
        assert_eq!(parse(Some("bytes=4-"), Some("\"10-7\""), &["\"10-7\""], 10), Range::Part(4, 9));
        assert_eq!(parse(Some("bytes=4-"), Some("\"10-6\""), &["\"10-7\""], 10), Range::Full);
        let p = Range::Part(4, 9);
        assert_eq!((p.status(), p.span(10), p.content_range(10).unwrap()), (206, (4, 6), "bytes 4-9/10".into()));
        assert_eq!(Range::Unsatisfiable.content_range(10).unwrap(), "bytes */10");
    }
}
