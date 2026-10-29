// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The workstation customizing object (`*WSCST`): the command strings
//! a printer wants for the things the transform does. `CRTWSCST`
//! compiles a source member in this crate's own format and keeps the
//! source with the object; `RTVWSCST` gives it back. The source is
//! lines of `KEY = value`, `*` in column 1 a comment; strings are double
//! quoted with `\e` (escape), `\r`, `\n`, `\f`, `\\`, `\"` and `\xHH`
//! escapes, and `&n` in `COPIES` stands for the count. The keys:
//!
//! | Key | Value | Used |
//! |---|---|---|
//! | `NAME` | string | the object's text when `TEXT(*SRCMBRTXT)` |
//! | `INIT` | string | sent first |
//! | `EXIT` | string | sent last |
//! | `NEWLINE` | string | ends a line (`"\r\n"`) |
//! | `FORMFEED` | string | ends a page (`"\f"`) |
//! | `PAPER` | name width height string | a paper size, in hundredths of an inch, and its command |
//! | `PORTRAIT`, `LANDSCAPE` | string | orientation |
//! | `CPI` | n string | pitch `n` |
//! | `LPI` | n string | line density `n` |
//! | `DRAWER` | n string | input tray `n` |
//! | `DUPLEX` | `*NO`/`*YES`/`*TUMBLE` string | sides |
//! | `COPIES` | string | copies, `&n` the count |
//!
//! The object's payload: `QSWMWSCS`, a version (1), the text (50
//! bytes), the source length (`u32` little-endian) and the source as
//! UTF-8 (`docs/PRINT.md`).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use thiserror::Error;

/// A source line that does not parse.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WscstError {
    /// A line with no `=`, or a key the format lacks.
    #[error("line {line}: {what}")]
    Syntax {
        /// The line number.
        line: usize,
        /// What is wrong.
        what: String,
    },
    /// A payload that is not a customizing object.
    #[error("not a workstation customizing object")]
    Corrupt,
}

/// A paper size and its command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaperCmd {
    /// Its name.
    pub name: String,
    /// Width in hundredths of an inch.
    pub width: u32,
    /// Height in hundredths of an inch.
    pub height: u32,
    /// The command.
    pub cmd: Vec<u8>,
}

/// A customizing object.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wscst {
    /// `NAME`.
    pub name: String,
    /// The object's text.
    pub text: String,
    /// The source as compiled.
    pub source: String,
    /// `INIT`.
    pub init: Vec<u8>,
    /// `EXIT`.
    pub exit: Vec<u8>,
    /// `NEWLINE`; CR LF when absent.
    pub newline: Vec<u8>,
    /// `FORMFEED`; FF when absent.
    pub formfeed: Vec<u8>,
    /// `PAPER` entries in source order.
    pub papers: Vec<PaperCmd>,
    /// `PORTRAIT`.
    pub portrait: Vec<u8>,
    /// `LANDSCAPE`.
    pub landscape: Vec<u8>,
    /// `CPI` by pitch.
    pub cpi: BTreeMap<u8, Vec<u8>>,
    /// `LPI` by density.
    pub lpi: BTreeMap<u8, Vec<u8>>,
    /// `DRAWER` by number.
    pub drawer: BTreeMap<u8, Vec<u8>>,
    /// `DUPLEX` by special value.
    pub duplex: BTreeMap<String, Vec<u8>>,
    /// `COPIES`, `&n` the count.
    pub copies: Vec<u8>,
}

const MAGIC: &[u8; 8] = b"QSWMWSCS";
const VERSION: u16 = 1;

/// A quoted string's bytes, and what follows the closing quote.
fn string(s: &str, line: usize) -> Result<(Vec<u8>, &str), WscstError> {
    let err = |what: &str| WscstError::Syntax {
        line,
        what: what.into(),
    };
    let s = s.trim_start();
    let rest = s.strip_prefix('"').ok_or_else(|| err("a string in double quotes expected"))?;
    let mut out = Vec::new();
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &rest[i + 1..])),
            '\\' => {
                let (_, e) = chars.next().ok_or_else(|| err("escape at the end of a string"))?;
                match e {
                    'e' => out.push(0x1b),
                    'r' => out.push(b'\r'),
                    'n' => out.push(b'\n'),
                    'f' => out.push(0x0c),
                    '\\' => out.push(b'\\'),
                    '"' => out.push(b'"'),
                    'x' => {
                        let h: String = chars.by_ref().take(2).map(|(_, c)| c).collect();
                        let b = u8::from_str_radix(&h, 16).map_err(|_| err("\\x needs two hex digits"))?;
                        out.push(b);
                    }
                    _ => return Err(err("unknown escape")),
                }
            }
            c if c.is_ascii() => out.push(c as u8),
            _ => return Err(err("a string holds ASCII only; use \\xHH")),
        }
    }
    Err(err("string not closed"))
}

/// A word, and what follows it.
fn word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    let n = s.find(char::is_whitespace).unwrap_or(s.len());
    (&s[..n], &s[n..])
}

impl Wscst {
    /// Compile `source`.
    pub fn parse(source: &str) -> Result<Self, WscstError> {
        let mut w = Self {
            source: source.to_string(),
            newline: b"\r\n".to_vec(),
            formfeed: vec![0x0c],
            ..Self::default()
        };
        for (i, raw) in source.lines().enumerate() {
            let line = i + 1;
            let err = |what: String| WscstError::Syntax { line, what };
            let text = raw.trim_end();
            if text.trim().is_empty() || text.starts_with('*') || text.trim_start().starts_with('#') {
                continue;
            }
            let (key, value) = text
                .split_once('=')
                .ok_or_else(|| err("no '=' on the line".into()))?;
            let key = key.trim().to_ascii_uppercase();
            let value = value.trim();
            let num = |s: &str| -> Result<u32, WscstError> {
                s.parse().map_err(|_| err(format!("{s}: a number expected")))
            };
            match key.as_str() {
                "NAME" => {
                    let (b, _) = string(value, line)?;
                    w.name = String::from_utf8_lossy(&b).to_string();
                }
                "INIT" => w.init = string(value, line)?.0,
                "EXIT" => w.exit = string(value, line)?.0,
                "NEWLINE" => w.newline = string(value, line)?.0,
                "FORMFEED" => w.formfeed = string(value, line)?.0,
                "PORTRAIT" => w.portrait = string(value, line)?.0,
                "LANDSCAPE" => w.landscape = string(value, line)?.0,
                "COPIES" => w.copies = string(value, line)?.0,
                "PAPER" => {
                    let (name, rest) = word(value);
                    let (width, rest) = word(rest);
                    let (height, rest) = word(rest);
                    let (cmd, _) = string(rest, line)?;
                    w.papers.push(PaperCmd {
                        name: name.to_ascii_uppercase(),
                        width: num(width)?,
                        height: num(height)?,
                        cmd,
                    });
                }
                "CPI" | "LPI" | "DRAWER" => {
                    let (n, rest) = word(value);
                    let n = u8::try_from(num(n)?).map_err(|_| err(format!("{n}: too large")))?;
                    let (cmd, _) = string(rest, line)?;
                    match key.as_str() {
                        "CPI" => w.cpi.insert(n, cmd),
                        "LPI" => w.lpi.insert(n, cmd),
                        _ => w.drawer.insert(n, cmd),
                    };
                }
                "DUPLEX" => {
                    let (which, rest) = word(value);
                    let which = which.to_ascii_uppercase();
                    if !matches!(which.as_str(), "*NO" | "*YES" | "*TUMBLE") {
                        return Err(err(format!("{which}: *NO, *YES or *TUMBLE expected")));
                    }
                    let (cmd, _) = string(rest, line)?;
                    w.duplex.insert(which, cmd);
                }
                k => return Err(err(format!("{k}: not a customizing keyword"))),
            }
        }
        Ok(w)
    }

    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.source.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        let mut text = [b' '; 50];
        for (i, b) in self.text.bytes().take(50).enumerate() {
            text[i] = b;
        }
        out.extend_from_slice(&text);
        out.extend_from_slice(&u32::try_from(self.source.len()).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(self.source.as_bytes());
        out
    }

    /// Read a payload back: the source compiles again.
    pub fn decode(b: &[u8]) -> Result<Self, WscstError> {
        if b.len() < 64 || &b[..8] != MAGIC || u16::from_le_bytes([b[8], b[9]]) != VERSION {
            return Err(WscstError::Corrupt);
        }
        let text = String::from_utf8_lossy(&b[10..60]).trim_end().to_string();
        let n = u32::from_le_bytes([b[60], b[61], b[62], b[63]]) as usize;
        let src = b.get(64..64 + n).ok_or(WscstError::Corrupt)?;
        let mut w = Self::parse(&String::from_utf8_lossy(src))?;
        w.text = text;
        Ok(w)
    }

    /// The command for a pitch: the exact one, else the next larger.
    #[must_use]
    pub fn cpi_cmd(&self, cpi: u8) -> Option<&[u8]> {
        self.cpi
            .get(&cpi)
            .or_else(|| self.cpi.range(cpi..).next().map(|(_, v)| v))
            .map(Vec::as_slice)
    }

    /// The command for a line density.
    #[must_use]
    pub fn lpi_cmd(&self, lpi: u8) -> Option<&[u8]> {
        self.lpi.get(&lpi).map(Vec::as_slice)
    }

    /// The first paper the page fits on, `(paper, landscape)`.
    #[must_use]
    pub fn paper_for(&self, width: u32, height: u32) -> Option<(&PaperCmd, bool)> {
        self.papers
            .iter()
            .find(|p| width <= p.width && height <= p.height)
            .map(|p| (p, false))
            .or_else(|| {
                self.papers
                    .iter()
                    .find(|p| width <= p.height && height <= p.width)
                    .map(|p| (p, true))
            })
    }

    /// A generic PCL5 object: what `MFRTYPMDL(*HP4)` does, as source.
    #[must_use]
    pub fn sample_pcl() -> &'static str {
        SAMPLE_PCL
    }

    /// The source `RTVWSCST DEVTYPE(*TRANSFORM) MFRTYPMDL()` writes for
    /// a LaserJet-class model: the PCL transform's commands as keys, so
    /// a changed copy compiles with `CRTWSCST`.
    #[must_use]
    pub fn source_for(model: crate::transform::Model) -> String {
        let caps = model.capabilities();
        let mut s = format!(
            "* Workstation customizing source for {}, from the host print transform.\nNAME      = \"{} printer\"\n",
            model.name(),
            model.name()
        );
        s.push_str("INIT      = \"\\eE\\e&l0E\\e&a0L\\e&s1C\\e(10U\"\nEXIT      = \"\\eE\"\nNEWLINE   = \"\\r\\n\"\nFORMFEED  = \"\\f\"\n");
        for p in crate::transform::page::SMALL.iter().chain(&crate::transform::page::LARGE) {
            let _ = writeln!(
                s,
                "PAPER     = {} {} {} \"\\e&l{}A\"",
                p.name.to_ascii_uppercase(),
                p.width,
                p.height,
                p.pcl
            );
        }
        s.push_str("PORTRAIT  = \"\\e&l0O\"\nLANDSCAPE = \"\\e&l1O\"\n");
        let pitches: &[(u8, &str, &str)] = if caps.pcl5 {
            &[
                (10, "10", "12"),
                (12, "12", "10"),
                (15, "15", "8"),
                (17, "16.67", "7"),
                (20, "20", "6"),
            ]
        } else {
            &[(10, "10", "12"), (12, "12", "10"), (17, "16.67", "7")]
        };
        let face = if caps.pcl5 { "4099" } else { "3" };
        for (n, h, v) in pitches {
            let _ = writeln!(s, "CPI       = {n} \"\\e(s0p{h}h{v}v0s0b{face}T\"");
        }
        for n in [4, 6, 8, 12] {
            let _ = writeln!(s, "LPI       = {n} \"\\e&l{n}D\"");
        }
        if caps.trays {
            s.push_str("DRAWER    = 1 \"\\e&l1H\"\nDRAWER    = 2 \"\\e&l4H\"\nDRAWER    = 3 \"\\e&l5H\"\n");
        }
        if caps.duplex {
            s.push_str("DUPLEX    = *NO \"\\e&l0S\"\nDUPLEX    = *YES \"\\e&l1S\"\nDUPLEX    = *TUMBLE \"\\e&l2S\"\n");
        }
        s.push_str("COPIES    = \"\\e&l&nX\"\n");
        s
    }
}

/// A customizing source for a PCL5 printer.
pub const SAMPLE_PCL: &str = r#"* Workstation customizing object: a PCL5 laser printer.
NAME      = "PCL5 laser printer"
INIT      = "\eE\e&l0E\e&a0L\e&s1C\e(10U"
EXIT      = "\eE"
NEWLINE   = "\r\n"
FORMFEED  = "\f"
PAPER     = LETTER 850 1100 "\e&l2A"
PAPER     = LEGAL 850 1400 "\e&l3A"
PAPER     = A4 827 1169 "\e&l26A"
PORTRAIT  = "\e&l0O"
LANDSCAPE = "\e&l1O"
CPI       = 10 "\e(s0p10h12v0s0b4099T"
CPI       = 12 "\e(s0p12h10v0s0b4099T"
CPI       = 15 "\e(s0p15h8v0s0b4099T"
CPI       = 17 "\e(s0p16.67h7v0s0b4099T"
CPI       = 20 "\e(s0p20h6v0s0b4099T"
LPI       = 6 "\e&l6D"
LPI       = 8 "\e&l8D"
LPI       = 4 "\e&l4D"
DRAWER    = 1 "\e&l1H"
DRAWER    = 2 "\e&l4H"
DRAWER    = 3 "\e&l5H"
DUPLEX    = *NO "\e&l0S"
DUPLEX    = *YES "\e&l1S"
DUPLEX    = *TUMBLE "\e&l2S"
COPIES    = "\e&l&nX"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_sample() {
        let w = Wscst::parse(SAMPLE_PCL).unwrap();
        assert_eq!(w.name, "PCL5 laser printer");
        assert_eq!(w.init, b"\x1bE\x1b&l0E\x1b&a0L\x1b&s1C\x1b(10U");
        assert_eq!(w.papers.len(), 3);
        assert_eq!(w.papers[2].name, "A4");
        assert_eq!(w.cpi_cmd(17).unwrap(), b"\x1b(s0p16.67h7v0s0b4099T");
        assert_eq!(w.cpi_cmd(11).unwrap(), b"\x1b(s0p12h10v0s0b4099T");
        assert_eq!(w.duplex["*TUMBLE"], b"\x1b&l2S");
        let (p, land) = w.paper_for(1100, 850).unwrap();
        assert_eq!((p.name.as_str(), land), ("LETTER", true));
    }

    #[test]
    fn model_sources_compile() {
        for m in crate::transform::Model::all() {
            let w = Wscst::parse(&Wscst::source_for(m)).unwrap_or_else(|e| panic!("{}: {e}", m.name()));
            assert_eq!(w.papers.len(), 5);
            assert_eq!(w.duplex.is_empty(), !m.capabilities().duplex);
        }
    }

    #[test]
    fn payload_round_trip() {
        let mut w = Wscst::parse(SAMPLE_PCL).unwrap();
        w.text = "Our laser".into();
        let back = Wscst::decode(&w.encode()).unwrap();
        assert_eq!(back, w);
        assert_eq!(Wscst::decode(b"nope"), Err(WscstError::Corrupt));
    }

    #[test]
    fn errors_name_the_line() {
        let e = Wscst::parse("NAME = \"x\"\nCPI = 10 \"\\q\"").unwrap_err();
        assert_eq!(
            e,
            WscstError::Syntax {
                line: 2,
                what: "unknown escape".into()
            }
        );
        assert!(Wscst::parse("WIDTH = 3").is_err());
        assert!(Wscst::parse("DUPLEX = *MAYBE \"\"").is_err());
        assert_eq!(Wscst::parse("INIT = \"\\x1b\\x45\"").unwrap().init, b"\x1bE");
    }
}
