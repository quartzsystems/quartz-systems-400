// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The data representation (`TYPE`) and what it does to an object's
//! bytes on the way out (`RETR`) and in (`STOR`): code page conversion
//! between the object's CCSID and the transfer's, line ends (`CRLF` in
//! ASCII, `NL` X'15' in EBCDIC), records of a member, trailing blanks.
//! `TYPE A` text is in the server's ASCII CCSID (`CHGFTPA CCSID`),
//! `TYPE E` text in the job's EBCDIC CCSID.

/// EBCDIC new line, the line end of `TYPE E`.
pub const NL: u8 = 0x15;
/// EBCDIC line feed.
pub const EBCDIC_LF: u8 = 0x25;
/// The EBCDIC CCSID when the job has none.
pub const EBCDIC_CCSID: u16 = 37;
/// Binary data.
pub const BINARY_CCSID: u16 = 65535;

/// The data representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rep {
    /// `TYPE A`: ASCII in the server's CCSID.
    Ascii,
    /// `TYPE E`: EBCDIC in the job's CCSID.
    Ebcdic,
    /// `TYPE I` (and `TYPE L 8`): bytes as they are.
    Image,
    /// `TYPE C n`: text in CCSID `n`.
    Ccsid(u16),
}

/// The CCSIDs text travels in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Codes {
    /// `TYPE A`'s.
    pub ascii: u16,
    /// `TYPE E`'s.
    pub ebcdic: u16,
}

impl Rep {
    /// The CCSID text goes in; `None` for image.
    pub fn ccsid(self, c: Codes) -> Option<u16> {
        match self {
            Rep::Ascii => Some(c.ascii),
            Rep::Ebcdic => Some(c.ebcdic),
            Rep::Image => None,
            Rep::Ccsid(n) => Some(n),
        }
    }
}

/// The EBCDIC CCSID a representation's text is in, when it is EBCDIC
/// (`TYPE E`, or `TYPE C` of an EBCDIC CCSID): its lines end in `NL`.
pub fn ebcdic_of(rep: Rep, c: Codes) -> Option<u16> {
    match rep {
        Rep::Ebcdic => Some(c.ebcdic),
        Rep::Ccsid(n) if is_ebcdic(n) => Some(n),
        _ => None,
    }
}

/// Whether a CCSID is EBCDIC.
pub fn is_ebcdic(ccsid: u16) -> bool {
    matches!(ccsid::conv::Codec::of(ccsid), Ok(Some(c)) if c.is_ebcdic())
}

/// Whether a CCSID holds text the server converts.
pub fn is_text(ccsid: u16) -> bool {
    ccsid != BINARY_CCSID && ccsid != 0 && matches!(ccsid::conv::Codec::of(ccsid), Ok(Some(_)))
}

/// Convert, passing bytes through when either side is not text.
pub fn convert(from: u16, to: u16, data: &[u8]) -> Vec<u8> {
    if from == to || !is_text(from) || !is_text(to) {
        return data.to_vec();
    }
    ccsid::conv::convert(from, to, data).unwrap_or_else(|_| data.to_vec())
}

/// The line end of text in `ccsid`.
pub fn line_end(ccsid: u16) -> &'static [u8] {
    if is_ebcdic(ccsid) { &[NL] } else { b"\r\n" }
}

/// The blank of `ccsid`.
pub fn blank(ccsid: u16) -> u8 {
    if is_ebcdic(ccsid) { 0x40 } else { 0x20 }
}

/// Lines of text in `ccsid`: split at line feeds (EBCDIC: `NL` or
/// `LF`), a carriage return before one dropped; a last line without a
/// line end counts.
pub fn lines(ccsid: u16, data: &[u8]) -> Vec<Vec<u8>> {
    let ebcdic = is_ebcdic(ccsid);
    let is_end = |b: u8| if ebcdic { b == NL || b == EBCDIC_LF } else { b == 0x0A };
    let mut out = Vec::new();
    let mut cur = Vec::new();
    for &b in data {
        if is_end(b) {
            if cur.last() == Some(&0x0D) {
                cur.pop();
            }
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(b);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// EBCDIC text with every line end (`CR LF`, `LF`) made `NL`.
pub fn ebcdic_lines(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            0x0D if text.get(i + 1) == Some(&EBCDIC_LF) => {
                out.push(NL);
                i += 1;
            }
            EBCDIC_LF => out.push(NL),
            b => out.push(b),
        }
        i += 1;
    }
    out
}

/// A stream file's bytes for `RETR`.
pub fn stream_out(rep: Rep, c: Codes, file_ccsid: u16, data: &[u8]) -> Vec<u8> {
    if rep == Rep::Image {
        return data.to_vec();
    }
    match ebcdic_of(rep, c) {
        Some(e) if is_text(file_ccsid) => ebcdic_lines(&convert(file_ccsid, e, data)),
        Some(_) => data.to_vec(),
        None => convert(file_ccsid, rep.ccsid(c).unwrap_or(c.ascii), data),
    }
}

/// What `STOR` writes into a stream file of `file_ccsid` (or a new one
/// when `None`): the bytes and the CCSID a new file is tagged with.
pub fn stream_in(rep: Rep, c: Codes, file_ccsid: Option<u16>, data: &[u8]) -> (Vec<u8>, u16) {
    if let (Some(e), false) = (ebcdic_of(rep, c), rep == Rep::Image) {
        // EBCDIC lines become the file's carriage return and line feed.
        let target = file_ccsid.unwrap_or(e);
        let mut text = Vec::with_capacity(data.len());
        for &b in data {
            if b == NL {
                text.extend_from_slice(&[0x0D, EBCDIC_LF]);
            } else {
                text.push(b);
            }
        }
        return (convert(e, target, &text), target);
    }
    match (rep, file_ccsid) {
        (Rep::Image, Some(f)) => (data.to_vec(), f),
        (Rep::Image, None) => (data.to_vec(), c.ascii),
        (Rep::Ebcdic, f) => {
            let target = f.unwrap_or(c.ebcdic);
            let mut text = Vec::with_capacity(data.len());
            for &b in data {
                if b == NL {
                    text.extend_from_slice(&[0x0D, EBCDIC_LF]);
                } else {
                    text.push(b);
                }
            }
            (convert(c.ebcdic, target, &text), target)
        }
        (rep, f) => {
            let from = rep.ccsid(c).unwrap_or(c.ascii);
            match f {
                Some(t) => (convert(from, t, data), t),
                None => (data.to_vec(), from),
            }
        }
    }
}

/// A member's records for `RETR`. `trim` drops trailing blanks.
pub fn member_out(rep: Rep, c: Codes, file_ccsid: u16, records: &[Vec<u8>], trim: bool) -> Vec<u8> {
    if rep == Rep::Image {
        return records.concat();
    }
    let to = rep.ccsid(c).unwrap_or(c.ascii);
    let end = line_end(to);
    let pad = blank(to);
    let mut out = Vec::new();
    for r in records {
        let mut t = convert(file_ccsid, to, r);
        if trim {
            while t.last() == Some(&pad) {
                t.pop();
            }
        }
        out.extend_from_slice(&t);
        out.extend_from_slice(end);
    }
    out
}

/// What `STOR` makes of the data for a member of `rec_len` in
/// `file_ccsid`: its records, each padded or cut to the length.
pub fn member_in(rep: Rep, c: Codes, file_ccsid: u16, rec_len: usize, data: &[u8]) -> Vec<Vec<u8>> {
    let len = rec_len.max(1);
    if rep == Rep::Image {
        // Binary data is padded with nulls, not blanks.
        return data
            .chunks(len)
            .map(|ch| {
                let mut r = ch.to_vec();
                r.resize(len, 0);
                r
            })
            .collect();
    }
    let pad = blank(file_ccsid);
    let from = rep.ccsid(c).unwrap_or(c.ascii);
    lines(from, data)
        .into_iter()
        .map(|l| {
            let mut r = convert(from, file_ccsid, &l);
            r.truncate(len);
            r.resize(len, pad);
            r
        })
        .collect()
}

/// The longest line of text `STOR` sends, for a file it creates.
pub fn longest_line(rep: Rep, c: Codes, data: &[u8]) -> usize {
    let from = rep.ccsid(c).unwrap_or(c.ascii);
    lines(from, data).iter().map(Vec::len).max().unwrap_or(0)
}

/// Whether text `STOR` sends has a line longer than `rec_len`.
pub fn too_long(rep: Rep, c: Codes, rec_len: usize, data: &[u8]) -> bool {
    rep != Rep::Image && longest_line(rep, c, data) > rec_len
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &[u8] = b"Quartz FTP probe line 1\r\nsecond line, 2\r\n\r\nlast line\r\n";
    const C: Codes = Codes { ascii: 819, ebcdic: 37 };

    #[test]
    fn stream_types() {
        assert_eq!(stream_out(Rep::Ascii, C, 819, TEXT), TEXT);
        let e = stream_out(Rep::Ebcdic, C, 819, TEXT);
        assert_eq!(e.len(), 50);
        assert_eq!(e[23], NL);
        assert_eq!(&e[..2], &[0xD8, 0xA4]);
        let (back, c) = stream_in(Rep::Ascii, C, None, TEXT);
        assert_eq!((back.as_slice(), c), (TEXT, 819));
        let (e, c) = stream_in(Rep::Ebcdic, C, None, &[0xC8, NL]);
        assert_eq!((e, c), (vec![0xC8, 0x0D, EBCDIC_LF], 37));
    }

    #[test]
    fn member_types() {
        let recs = member_in(Rep::Ascii, C, 37, 23, TEXT);
        assert_eq!(recs.len(), 4);
        assert!(recs.iter().all(|r| r.len() == 23));
        assert_eq!(recs[2], vec![0x40; 23]);
        assert_eq!(member_out(Rep::Ascii, C, 37, &recs, true), TEXT);
        let e = member_out(Rep::Ebcdic, C, 37, &recs, true);
        assert_eq!(e.len(), 50);
        assert_eq!(member_out(Rep::Image, C, 37, &recs, true).len(), 92);
        assert_eq!(longest_line(Rep::Ascii, C, TEXT), 23);
        let bin = member_in(Rep::Image, C, 37, 10, &[1u8; 25]);
        assert_eq!(bin.len(), 3);
        assert_eq!(bin[2][5..], [0; 5]);
        assert!(too_long(Rep::Ascii, C, 10, TEXT));
    }
}
