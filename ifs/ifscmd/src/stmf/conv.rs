// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The byte work of the two copies, with no file system: a member's
//! records to a stream (`CPYTOSTMF`) and a stream to records
//! (`CPYFRMSTMF`). With conversion (`CVTDTA(*AUTO)`) lines are found
//! and tabs expanded in the converted text; without it (`*NONE`) the
//! bytes are the member's and its encoding's line ends are looked for.

use ifs::vfs::qsys::MemberData;

use super::{Cvt, EndLin};

/// Why bytes could not be converted: the two CCSIDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported(pub u16, pub u16);

/// Whether a CCSID is EBCDIC.
#[must_use]
pub fn is_ebcdic(ccsid: u16) -> bool {
    matches!(ccsid::conv::Codec::of(ccsid), Ok(Some(c)) if c.is_ebcdic())
}

/// The blank of a CCSID's records.
#[must_use]
pub fn blank(ccsid: u16) -> u8 {
    if is_ebcdic(ccsid) { 0x40 } else { 0x20 }
}

/// An ASCII CCSID for an EBCDIC one: `*PCASCII` the Windows code page
/// (1252), `*STDASCII` ISO 8859-1 (819); an ASCII CCSID stays.
#[must_use]
pub fn ascii_of(ccsid: u16, pc: bool) -> u16 {
    if is_ebcdic(ccsid) || ccsid == 65535 {
        if pc { 1252 } else { 819 }
    } else {
        ccsid
    }
}

fn convert(from: u16, to: u16, b: &[u8]) -> Result<Vec<u8>, Unsupported> {
    if from == to || from == 65535 || to == 65535 {
        return Ok(b.to_vec());
    }
    ccsid::conv::convert(from, to, b).map_err(|_| Unsupported(from, to))
}

fn encode(ccsid: u16, s: &str) -> Result<Vec<u8>, Unsupported> {
    if ccsid == 65535 {
        return Ok(s.as_bytes().to_vec());
    }
    ccsid::conv::from_str(ccsid, s).map_err(|_| Unsupported(1208, ccsid))
}

fn decode(ccsid: u16, b: &[u8]) -> Result<String, Unsupported> {
    if ccsid == 65535 {
        return Ok(String::from_utf8_lossy(b).into_owned());
    }
    ccsid::conv::to_string(ccsid, b).map_err(|_| Unsupported(ccsid, 1208))
}

/// A line end's characters.
fn line_end(e: EndLin) -> &'static str {
    match e {
        EndLin::Crlf | EndLin::All => "\r\n",
        EndLin::Lf => "\n",
        EndLin::Cr => "\r",
        EndLin::Lfcr => "\n\r",
        EndLin::Fixed => "",
    }
}

/// A record without its trailing blanks.
fn trimmed(r: &[u8], blank: u8) -> &[u8] {
    let n = r.iter().rposition(|&b| b != blank).map_or(0, |i| i + 1);
    &r[..n]
}

/// `CPYTOSTMF`: the records of `m` (CCSID `dbf`) as stream bytes in
/// `stmf`. A source member's records lose their trailing blanks
/// (except `*FIXED`); every record but `*FIXED`'s ends with the line
/// end, encoded in the stream's CCSID (the member's with `*NONE`).
pub fn to_stream(m: &MemberData, dbf: u16, stmf: u16, cvt: Cvt, e: EndLin) -> Result<Vec<u8>, Unsupported> {
    let out_ccsid = if cvt == Cvt::None { dbf } else { stmf };
    let end = encode(out_ccsid, line_end(e))?;
    let b = blank(dbf);
    let mut out = Vec::new();
    for r in &m.records {
        let data = if m.source && e != EndLin::Fixed {
            trimmed(r, b)
        } else {
            &r[..]
        };
        if cvt == Cvt::None {
            out.extend_from_slice(data);
        } else {
            out.extend_from_slice(&convert(dbf, stmf, data)?);
        }
        out.extend_from_slice(&end);
    }
    Ok(out)
}

/// What [`to_records`] made.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Records {
    /// The records, each `record_len` bytes.
    pub records: Vec<Vec<u8>>,
    /// How many lines were cut to the record length.
    pub truncated: usize,
}

/// Lines of text at the line ends `e` accepts (`*ALL`: CR LF, LF CR,
/// LF, CR and the EBCDIC new line). A last line end ends the last line.
fn split_text(s: &str, e: EndLin) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let end = match e {
            EndLin::All => match (c, next) {
                ('\r', Some('\n')) | ('\n', Some('\r')) => 2,
                ('\r' | '\n' | '\u{85}', _) => 1,
                _ => 0,
            },
            EndLin::Crlf => usize::from(c == '\r' && next == Some('\n')) * 2,
            EndLin::Lfcr => usize::from(c == '\n' && next == Some('\r')) * 2,
            EndLin::Lf => usize::from(c == '\n' || c == '\u{85}'),
            EndLin::Cr => usize::from(c == '\r'),
            EndLin::Fixed => 0,
        };
        if end > 0 {
            lines.push(std::mem::take(&mut cur));
            i += end;
        } else {
            cur.push(c);
            i += 1;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Tabs to blanks up to the next multiple of eight columns.
#[must_use]
pub fn expand_tabs(s: &str) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    for c in s.chars() {
        if c == '\t' {
            let n = 8 - col % 8;
            out.extend(std::iter::repeat_n(' ', n));
            col += n;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

/// One record from line bytes: cut or padded to `len`.
fn record(mut b: Vec<u8>, len: usize, blank: u8, cut: &mut usize) -> Vec<u8> {
    if b.len() > len {
        b.truncate(len);
        *cut += 1;
    }
    b.resize(len, blank);
    b
}

/// Lines of raw bytes in `ccsid`'s encoding.
fn split_bytes(data: &[u8], ccsid: u16, e: EndLin) -> Vec<Vec<u8>> {
    let (cr, lf, nl) = if is_ebcdic(ccsid) {
        (0x0Du8, 0x25u8, Some(0x15u8))
    } else {
        (0x0D, 0x0A, None)
    };
    let mut lines = Vec::new();
    let mut cur = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let c = data[i];
        let next = data.get(i + 1).copied();
        let is_lf = c == lf || Some(c) == nl;
        let end = match e {
            EndLin::All => {
                if (c == cr && next == Some(lf)) || (c == lf && next == Some(cr)) {
                    2
                } else {
                    usize::from(c == cr || is_lf)
                }
            }
            EndLin::Crlf => usize::from(c == cr && next == Some(lf)) * 2,
            EndLin::Lfcr => usize::from(c == lf && next == Some(cr)) * 2,
            EndLin::Lf => usize::from(is_lf),
            EndLin::Cr => usize::from(c == cr),
            EndLin::Fixed => 0,
        };
        if end > 0 {
            lines.push(std::mem::take(&mut cur));
            i += end;
        } else {
            cur.push(c);
            i += 1;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// `CPYFRMSTMF`: stream bytes in `stmf` as records of `len` bytes in
/// `dbf`. `*FIXED` cuts the converted bytes every `len`; otherwise each
/// line is a record, tabs expanded with `tabs`.
pub fn to_records(
    data: &[u8],
    stmf: u16,
    dbf: u16,
    len: usize,
    cvt: Cvt,
    e: EndLin,
    tabs: bool,
) -> Result<Records, Unsupported> {
    let len = len.max(1);
    let b = blank(dbf);
    let mut out = Records::default();
    if cvt == Cvt::None {
        if e == EndLin::Fixed {
            out.records = data.chunks(len).map(|c| record(c.to_vec(), len, b, &mut 0)).collect();
            return Ok(out);
        }
        for l in split_bytes(data, dbf, e) {
            let r = record(l, len, b, &mut out.truncated);
            out.records.push(r);
        }
        return Ok(out);
    }
    if e == EndLin::Fixed {
        let bytes = convert(stmf, dbf, data)?;
        out.records = bytes.chunks(len).map(|c| record(c.to_vec(), len, b, &mut 0)).collect();
        return Ok(out);
    }
    let text = decode(stmf, data)?;
    for l in split_text(&text, e) {
        let l = if tabs { expand_tabs(&l) } else { l };
        let bytes = encode(dbf, &l)?;
        let r = record(bytes, len, b, &mut out.truncated);
        out.records.push(r);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(lines: &[&str]) -> MemberData {
        MemberData {
            records: lines
                .iter()
                .map(|l| ccsid::conv::from_str(37, &format!("{l:<20}")).unwrap())
                .collect(),
            record_len: 20,
            source: true,
            ccsid: 37,
        }
    }

    #[test]
    fn source_to_ascii_and_back() {
        let m = src(&["PGM", "  SNDPGMMSG 'x'", "ENDPGM"]);
        let s = to_stream(&m, 37, 819, Cvt::Auto, EndLin::Crlf).unwrap();
        assert_eq!(s, b"PGM\r\n  SNDPGMMSG 'x'\r\nENDPGM\r\n");
        let r = to_records(&s, 819, 37, 20, Cvt::Auto, EndLin::All, true).unwrap();
        assert_eq!(r.records, m.records);
        assert_eq!(r.truncated, 0);
    }

    #[test]
    fn line_ends_tabs_and_cuts() {
        assert_eq!(split_text("a\nb\r\nc\rd\n\re", EndLin::All), ["a", "b", "c", "d", "e"]);
        assert_eq!(split_text("a\nb\n", EndLin::Lf), ["a", "b"]);
        assert_eq!(split_text("a\nb\r\n", EndLin::Crlf), ["a\nb"]);
        assert_eq!(expand_tabs("a\tb\t\tc"), format!("a{}b{}c", " ".repeat(7), " ".repeat(15)));
        let r = to_records(b"0123456789ABC\nx", 819, 37, 10, Cvt::Auto, EndLin::Lf, false).unwrap();
        assert_eq!(r.truncated, 1);
        assert_eq!(r.records.len(), 2);
        let f = to_records(b"abcdef", 819, 37, 4, Cvt::Auto, EndLin::Fixed, false).unwrap();
        assert_eq!(f.records, [ccsid::conv::from_str(37, "abcd").unwrap(), ccsid::conv::from_str(37, "ef  ").unwrap()]);
    }

    #[test]
    fn no_conversion_keeps_bytes() {
        let m = src(&["AB"]);
        let s = to_stream(&m, 37, 819, Cvt::None, EndLin::Lf).unwrap();
        assert_eq!(s, vec![0xC1, 0xC2, 0x25]);
        let r = to_records(&s, 819, 37, 20, Cvt::None, EndLin::All, true).unwrap();
        assert_eq!(r.records, m.records);
        assert_eq!(ascii_of(37, true), 1252);
        assert_eq!(ascii_of(37, false), 819);
        assert_eq!(ascii_of(1208, true), 1208);
    }
}
