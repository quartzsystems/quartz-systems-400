// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A stream file as text: its bytes decoded from the file's CCSID into
//! lines, the line end it uses noted so a save writes the same one, and
//! encoded back. A file without a line end is one line.

/// The line end a file uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Eol {
    /// Carriage return and line feed (`*CRLF`).
    #[default]
    Crlf,
    /// Line feed (`*LF`).
    Lf,
    /// Carriage return (`*CR`).
    Cr,
    /// Line feed and carriage return (`*LFCR`).
    Lfcr,
    /// The EBCDIC new line (`0x15`, U+0085), as a member read in text
    /// mode or an EBCDIC file has.
    Nl,
}

impl Eol {
    /// The characters.
    #[must_use]
    pub fn chars(self) -> &'static str {
        match self {
            Self::Crlf => "\r\n",
            Self::Lf => "\n",
            Self::Cr => "\r",
            Self::Lfcr => "\n\r",
            Self::Nl => "\u{85}",
        }
    }

    /// IBM's special value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Crlf => "*CRLF",
            Self::Lf => "*LF",
            Self::Cr => "*CR",
            Self::Lfcr => "*LFCR",
            Self::Nl => "*NL",
        }
    }

    /// From a special value.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "*CRLF" | "CRLF" => Some(Self::Crlf),
            "*LF" | "LF" => Some(Self::Lf),
            "*CR" | "CR" => Some(Self::Cr),
            "*LFCR" | "LFCR" => Some(Self::Lfcr),
            "*NL" | "NL" => Some(Self::Nl),
            _ => None,
        }
    }
}

/// A file's text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Text {
    /// The lines, without their ends.
    pub lines: Vec<String>,
    /// The line end found first (the save writes it after every line).
    pub eol: Eol,
    /// Whether the last line had its end.
    pub final_eol: bool,
    /// The bytes of each line in the file's encoding, for the hex view.
    pub raw: Vec<Vec<u8>>,
}

/// The first line end in a decoded text, if any.
fn detect(s: &str) -> Option<Eol> {
    let b: Vec<char> = s.chars().collect();
    for (i, c) in b.iter().enumerate() {
        match c {
            '\r' => {
                return Some(if b.get(i + 1) == Some(&'\n') {
                    Eol::Crlf
                } else {
                    Eol::Cr
                });
            }
            '\n' => {
                return Some(if b.get(i + 1) == Some(&'\r') {
                    Eol::Lfcr
                } else {
                    Eol::Lf
                });
            }
            '\u{85}' => return Some(Eol::Nl),
            _ => {}
        }
    }
    None
}

/// Split decoded text at its line ends (any of them: a file may mix).
fn split(s: &str) -> (Vec<String>, bool) {
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut it = s.chars().peekable();
    let mut ended = false;
    while let Some(c) = it.next() {
        ended = false;
        match c {
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                lines.push(std::mem::take(&mut cur));
                ended = true;
            }
            '\n' => {
                if it.peek() == Some(&'\r') {
                    it.next();
                }
                lines.push(std::mem::take(&mut cur));
                ended = true;
            }
            '\u{85}' => {
                lines.push(std::mem::take(&mut cur));
                ended = true;
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    (lines, ended)
}

impl Text {
    /// Decode a file's bytes; `None` when the CCSID is not one the
    /// system converts.
    #[must_use]
    pub fn decode(bytes: &[u8], file_ccsid: u16) -> Option<Self> {
        let s = ccsid::conv::to_string(file_ccsid, bytes).ok()?;
        let eol = detect(&s).unwrap_or(if is_ebcdic(file_ccsid) { Eol::Nl } else { Eol::Crlf });
        let (lines, final_eol) = split(&s);
        let raw = lines
            .iter()
            .map(|l| ccsid::conv::from_str(file_ccsid, l).unwrap_or_default())
            .collect();
        Some(Self {
            lines,
            eol,
            final_eol,
            raw,
        })
    }

    /// The file's bytes for `lines` in `file_ccsid`, every line ended
    /// with `eol` (the last one only when the file had it, or when it
    /// is not the only thing written).
    #[must_use]
    pub fn encode(lines: &[String], eol: Eol, final_eol: bool, file_ccsid: u16) -> Option<Vec<u8>> {
        let mut s = String::new();
        for (i, l) in lines.iter().enumerate() {
            s.push_str(l);
            if i + 1 < lines.len() || final_eol {
                s.push_str(eol.chars());
            }
        }
        ccsid::conv::from_str(file_ccsid, &s).ok()
    }
}

/// Whether a CCSID is EBCDIC.
#[must_use]
pub fn is_ebcdic(ccsid: u16) -> bool {
    ccsid::conv::Codec::of(ccsid)
        .ok()
        .flatten()
        .is_some_and(|t| t.is_ebcdic())
}

/// Text as the job's CCSID shows it: characters the job's CCSID cannot
/// hold become its substitute; control characters (a tab) show as one
/// blank, as the 5250 shows them.
#[must_use]
pub fn shown(line: &str, job_ccsid: u16) -> String {
    let round = ccsid::conv::from_str(job_ccsid, line)
        .ok()
        .and_then(|b| ccsid::conv::to_string(job_ccsid, &b).ok())
        .unwrap_or_else(|| line.to_string());
    round
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// The record length IBM's workspace gives a stream file. Read from one
/// capture only (`tools/conformance/golden/phase10/ifs/edtf.txt`: a
/// 53-byte file of three short lines shows 59): the file's size plus
/// six, never less than the longest line, at most 32 766.
#[must_use]
pub fn record_length(size: usize, longest: usize) -> usize {
    (size + 6).max(longest).min(32_766)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROBE: &[u8] = b"Quartz IFS probe\r\nline two\r\n\tline three after a tab\r\n";

    #[test]
    fn round_trip_keeps_line_ends() {
        let t = Text::decode(PROBE, 819).unwrap();
        assert_eq!(t.lines.len(), 3);
        assert_eq!(t.eol, Eol::Crlf);
        assert!(t.final_eol);
        assert_eq!(Text::encode(&t.lines, t.eol, t.final_eol, 819).unwrap(), PROBE);
        let lf = Text::decode(b"a\nb", 819).unwrap();
        assert_eq!((lf.eol, lf.final_eol), (Eol::Lf, false));
        assert_eq!(Text::encode(&lf.lines, lf.eol, lf.final_eol, 819).unwrap(), b"a\nb");
        assert_eq!(shown("\tx", 37), " x");
        assert_eq!(record_length(PROBE.len(), 22), 59);
    }

    #[test]
    fn ebcdic_files() {
        let bytes = ccsid::conv::from_str(37, "AB\u{85}CD\u{85}").unwrap();
        let t = Text::decode(&bytes, 37).unwrap();
        assert_eq!(t.lines, vec!["AB", "CD"]);
        assert_eq!(t.eol, Eol::Nl);
        assert_eq!(Text::encode(&t.lines, t.eol, true, 37).unwrap(), bytes);
    }
}
