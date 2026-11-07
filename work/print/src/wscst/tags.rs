// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM's tag language for customizing source, as `RTVWSCST` writes it
//! and `CRTWSCST` reads it: `:WSCST DEVCLASS=TRANSFORM.`, then tags,
//! each `:NAME` with `KEY=value` attributes and ending at a period, and
//! `:EWSCST.` last. Values are numbers, words, or strings in quotes with
//! an `X` after them for hexadecimal (`'1B45'X`); several strings in a
//! row are one value. `/* ... */` is a comment. Tags the transform has
//! no use for are read and kept out of the object.
//!
//! The tags read: `INITPRT`, `RESETPRT`, `CARRTN`, `LINEFEED`,
//! `FORMFEED`, `PAGSIZE` (`PAGWTH`, `PAGLEN` in 1440ths of an inch),
//! `PRTORIENT` (`ORIENT`), `DWRSLT` (`DRAWER=DRAWERn`), `SMPXPRT`,
//! `DUPXPRT`, `TUMDUPXPRT`, `VARLSPC` (the line spacing, a number put
//! into `DATA` at `VAROFFSET` as `VARLEN` decimal digits, in units of
//! `CNVNUM`/`CNVDEN` inch), and two of QS/400's own: `CPI` (`CPI=n`,
//! the command for a pitch) and `COPIES` (the copy count put in as
//! `VARLSPC`'s number is).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::{PaperCmd, Wscst, WscstError};

/// One tag and its attributes, values as text or bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    /// Its name, upper case.
    pub name: String,
    /// Its attributes in order.
    pub attrs: Vec<(String, Val)>,
    /// The line it starts on.
    pub line: usize,
}

/// An attribute's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Val {
    /// A number or a word.
    Word(String),
    /// Quoted strings, joined (hexadecimal ones as their bytes).
    Bytes(Vec<u8>),
}

impl Tag {
    fn get(&self, k: &str) -> Option<&Val> {
        self.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v)
    }

    fn word(&self, k: &str) -> Option<String> {
        match self.get(k)? {
            Val::Word(w) => Some(w.to_ascii_uppercase()),
            Val::Bytes(b) => Some(String::from_utf8_lossy(b).to_ascii_uppercase()),
        }
    }

    fn num(&self, k: &str) -> Option<u32> {
        self.word(k)?.trim().parse().ok()
    }

    fn data(&self) -> Vec<u8> {
        match self.get("DATA") {
            Some(Val::Bytes(b)) => b.clone(),
            _ => Vec::new(),
        }
    }
}

/// Whether `source` is in the tag language: its first word is a tag.
#[must_use]
pub fn is_tags(source: &str) -> bool {
    strip_comments(source).trim_start().starts_with(':')
}

fn strip_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => {
                // Keep the comment's line breaks so lines still count.
                out.extend(rest[i..i + 2 + j + 2].chars().filter(|c| *c == '\n'));
                rest = &rest[i + 2 + j + 2..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn unhex(s: &str, line: usize) -> Result<Vec<u8>, WscstError> {
    let digits: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !digits.len().is_multiple_of(2) {
        return Err(WscstError::Syntax {
            line,
            what: format!("'{s}'X: an odd number of hexadecimal digits"),
        });
    }
    (0..digits.len() / 2)
        .map(|i| {
            u8::from_str_radix(&digits[2 * i..2 * i + 2], 16).map_err(|_| WscstError::Syntax {
                line,
                what: format!("'{s}'X: not hexadecimal"),
            })
        })
        .collect()
}

/// The tags of `source`.
pub fn tags(source: &str) -> Result<Vec<Tag>, WscstError> {
    let text = strip_comments(source);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut line = 1;
    let mut out = Vec::new();
    let err = |line: usize, what: &str| WscstError::Syntax {
        line,
        what: what.into(),
    };
    let skip = |i: &mut usize, line: &mut usize| {
        while *i < chars.len() && chars[*i].is_whitespace() {
            if chars[*i] == '\n' {
                *line += 1;
            }
            *i += 1;
        }
    };
    loop {
        skip(&mut i, &mut line);
        if i >= chars.len() {
            break;
        }
        if chars[i] != ':' {
            return Err(err(line, "a tag (':NAME') expected"));
        }
        i += 1;
        let start = line;
        let mut name = String::new();
        while i < chars.len() && chars[i].is_ascii_alphanumeric() {
            name.push(chars[i].to_ascii_uppercase());
            i += 1;
        }
        if name.is_empty() {
            return Err(err(line, "a tag name expected after ':'"));
        }
        let mut attrs = Vec::new();
        loop {
            skip(&mut i, &mut line);
            match chars.get(i) {
                None => return Err(err(start, &format!(":{name} does not end with a period"))),
                Some('.') => {
                    i += 1;
                    break;
                }
                Some(_) => {}
            }
            let mut key = String::new();
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                key.push(chars[i].to_ascii_uppercase());
                i += 1;
            }
            skip(&mut i, &mut line);
            if key.is_empty() || chars.get(i) != Some(&'=') {
                return Err(err(
                    line,
                    &format!(":{name}: an attribute (KEY=value) expected"),
                ));
            }
            i += 1;
            skip(&mut i, &mut line);
            let value = if chars.get(i) == Some(&'\'') {
                let mut bytes = Vec::new();
                while chars.get(i) == Some(&'\'') {
                    let close = chars[i + 1..]
                        .iter()
                        .position(|c| *c == '\'')
                        .ok_or_else(|| err(line, "a string is not closed"))?;
                    let s: String = chars[i + 1..i + 1 + close].iter().collect();
                    i += close + 2;
                    if matches!(chars.get(i), Some('X' | 'x')) {
                        i += 1;
                        bytes.extend(unhex(&s, line)?);
                    } else {
                        bytes.extend(s.bytes());
                    }
                    skip(&mut i, &mut line);
                }
                Val::Bytes(bytes)
            } else {
                let mut w = String::new();
                while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '.' {
                    w.push(chars[i]);
                    i += 1;
                }
                Val::Word(w)
            };
            attrs.push((key, value));
        }
        out.push(Tag {
            name,
            attrs,
            line: start,
        });
    }
    Ok(out)
}

/// A paper's name by its size in hundredths of an inch.
fn paper_name(width: u32, height: u32) -> String {
    let known: &[(&str, u32, u32)] = &[
        ("LETTER", 850, 1100),
        ("LEGAL", 850, 1400),
        ("EXECUTIVE", 725, 1050),
        ("LEDGER", 1100, 1700),
        ("A4", 827, 1169),
        ("A3", 1169, 1654),
        ("A5", 583, 827),
        ("B5", 717, 1012),
        ("B4", 1012, 1433),
    ];
    known
        .iter()
        .find(|(_, w, h)| width.abs_diff(*w) <= 3 && height.abs_diff(*h) <= 3)
        .map_or_else(
            || format!("P{width}X{height}"),
            |(n, _, _)| (*n).to_string(),
        )
}

/// `data` with `n` put in at `VAROFFSET` as `VARLEN` decimal digits.
fn insert(t: &Tag, n: u32) -> Vec<u8> {
    let mut d = t.data();
    let off = t.num("VAROFFSET").unwrap_or(0) as usize;
    let len = t.num("VARLEN").unwrap_or(0) as usize;
    if len == 0 || off + len > d.len() {
        return d;
    }
    let digits = format!("{n:0len$}");
    let digits = &digits[digits.len() - len..];
    d[off..off + len].copy_from_slice(digits.as_bytes());
    d
}

/// The densities a `VARLSPC` tag gives commands for.
const DENSITIES: [u8; 6] = [3, 4, 6, 8, 9, 12];

/// Compile tag-language `source`.
pub fn compile(source: &str) -> Result<Wscst, WscstError> {
    let all = tags(source)?;
    let first = all.first().ok_or(WscstError::Syntax {
        line: 1,
        what: "no :WSCST tag".into(),
    })?;
    if first.name != "WSCST" {
        return Err(WscstError::Syntax {
            line: first.line,
            what: format!(":{}: the source starts with :WSCST", first.name),
        });
    }
    if first
        .word("DEVCLASS")
        .is_some_and(|d| d != "TRANSFORM" && d != "CTXFORM")
    {
        return Err(WscstError::Syntax {
            line: first.line,
            what: "DEVCLASS is TRANSFORM or CTXFORM".into(),
        });
    }
    let mut w = Wscst {
        source: source.to_string(),
        newline: b"\r\n".to_vec(),
        formfeed: vec![0x0c],
        ..Wscst::default()
    };
    let (mut cr, mut lf) = (None, None);
    for t in &all[1..] {
        match t.name.as_str() {
            "INITPRT" => w.init = t.data(),
            "RESETPRT" => w.exit = t.data(),
            "CARRTN" => cr = Some(t.data()),
            "LINEFEED" => lf = Some(t.data()),
            "FORMFEED" => w.formfeed = t.data(),
            "PAGSIZE" => {
                let (Some(pw), Some(pl)) = (t.num("PAGWTH"), t.num("PAGLEN")) else {
                    continue;
                };
                let (width, height) = ((pw * 100 + 720) / 1440, (pl * 100 + 720) / 1440);
                w.papers.push(PaperCmd {
                    name: paper_name(width, height),
                    width,
                    height,
                    cmd: t.data(),
                });
            }
            "PRTORIENT" => match t.word("ORIENT").as_deref() {
                Some("PORTRAIT") => w.portrait = t.data(),
                Some("LANDSCAPE") => w.landscape = t.data(),
                _ => {}
            },
            "DWRSLT" => {
                if let Some(n) = t
                    .word("DRAWER")
                    .and_then(|d| d.strip_prefix("DRAWER").and_then(|n| n.parse::<u8>().ok()))
                {
                    w.drawer.insert(n, t.data());
                }
            }
            "SMPXPRT" => {
                w.duplex.insert("*NO".into(), t.data());
            }
            "DUPXPRT" => {
                w.duplex.insert("*YES".into(), t.data());
            }
            "TUMDUPXPRT" => {
                w.duplex.insert("*TUMBLE".into(), t.data());
            }
            "VARLSPC" => {
                let num = t.num("CNVNUM").unwrap_or(1).max(1);
                let den = t.num("CNVDEN").unwrap_or(48).max(1);
                for lpi in DENSITIES {
                    // A line is 1/lpi inch: den / (lpi * num) units.
                    let units = den / (u32::from(lpi) * num);
                    w.lpi.insert(lpi, insert(t, units));
                }
            }
            "CPI" => {
                if let Some(n) = t.num("CPI").and_then(|n| u8::try_from(n).ok()) {
                    w.cpi.insert(n, t.data());
                }
            }
            "COPIES" => {
                let d = t.data();
                let off = t.num("VAROFFSET").unwrap_or(0) as usize;
                let len = t.num("VARLEN").unwrap_or(0) as usize;
                if len > 0 && off + len <= d.len() {
                    let mut c = d[..off].to_vec();
                    c.extend_from_slice(b"&n");
                    c.extend_from_slice(&d[off + len..]);
                    w.copies = c;
                } else {
                    w.copies = d;
                }
            }
            _ => {}
        }
    }
    if cr.is_some() || lf.is_some() {
        let mut nl = cr.unwrap_or_default();
        nl.extend(lf.unwrap_or_default());
        w.newline = nl;
    }
    Ok(w)
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02X}");
        s
    })
}

fn data_tag(s: &mut String, tag: &str, attrs: &[(&str, String)], data: &[u8]) {
    let _ = writeln!(s, "    :{tag}");
    for (k, v) in attrs {
        let _ = writeln!(s, "      {k}={v}");
    }
    let _ = writeln!(s, "      DATA='{}'X.", hex(data));
}

/// Tag-language source of `w`'s commands (what `RTVWSCST` writes).
#[must_use]
pub fn write(w: &Wscst, title: &str) -> String {
    let mut s = format!("/* {title} */\n:WSCST DEVCLASS=TRANSFORM.\n\n    :TRNSFRMTBL.\n");
    data_tag(&mut s, "INITPRT", &[], &w.init);
    data_tag(&mut s, "RESETPRT", &[], &w.exit);
    let (cr, lf) = match w.newline.as_slice() {
        [b'\r', rest @ ..] => (vec![b'\r'], rest.to_vec()),
        other => (Vec::new(), other.to_vec()),
    };
    data_tag(&mut s, "CARRTN", &[], &cr);
    data_tag(&mut s, "LINEFEED", &[], &lf);
    data_tag(&mut s, "FORMFEED", &[], &w.formfeed);
    if !w.papers.is_empty() {
        s.push_str("    :PAGSIZXFM.\n");
        for p in &w.papers {
            data_tag(
                &mut s,
                "PAGSIZE",
                &[
                    ("PAGWTH", format!("{:5}", (p.width * 1440 + 50) / 100)),
                    ("PAGLEN", format!("{:5}", (p.height * 1440 + 50) / 100)),
                ],
                &p.cmd,
            );
        }
        s.push_str("    :EPAGSIZXFM.\n");
    }
    for (o, d) in [("PORTRAIT", &w.portrait), ("LANDSCAPE", &w.landscape)] {
        if !d.is_empty() {
            data_tag(&mut s, "PRTORIENT", &[("ORIENT", o.into())], d);
        }
    }
    for (n, d) in &w.cpi {
        data_tag(&mut s, "CPI", &[("CPI", n.to_string())], d);
    }
    for (n, d) in &w.drawer {
        data_tag(&mut s, "DWRSLT", &[("DRAWER", format!("DRAWER{n}"))], d);
    }
    for (k, tag) in [
        ("*NO", "SMPXPRT"),
        ("*YES", "DUPXPRT"),
        ("*TUMBLE", "TUMDUPXPRT"),
    ] {
        if let Some(d) = w.duplex.get(k) {
            data_tag(&mut s, tag, &[], d);
        }
    }
    s.push_str(":EWSCST.\n");
    s
}

/// Tag-language source with a `VARLSPC` and `COPIES` of PCL's form:
/// `ESC &l#C` in 1/48 inch and `ESC &l#X`.
#[must_use]
pub fn write_pcl(w: &Wscst, title: &str) -> String {
    let mut s = write(
        &Wscst {
            lpi: BTreeMap::new(),
            ..w.clone()
        },
        title,
    );
    let end = s.rfind(":EWSCST.").unwrap_or(s.len());
    let mut extra = String::new();
    let var = |tag: &str, len: usize, data: &[u8], cnv: Option<(u32, u32)>| {
        let mut t = format!(
            "    :{tag}\n      VAROFFSET=    3\n      VARLEN=    {len}\n      VARTYPE=CHRDEC\n"
        );
        if let Some((n, d)) = cnv {
            let _ = write!(t, "      CNVNUM=    {n}\n      CNVDEN=   {d}\n");
        }
        let _ = writeln!(t, "      DATA='{}'X.", hex(data));
        t
    };
    extra.push_str(&var("VARLSPC", 3, b"\x1b&l\0\0\0C", Some((1, 48))));
    extra.push_str(&var("COPIES", 3, b"\x1b&l\0\0\0X", None));
    s.insert_str(end, &extra);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const IBM_FORM: &str = "\
:WSCST DEVCLASS=TRANSFORM.

    :TRNSFRMTBL.
    :PRTDTASTRM
      DATASTREAM=HPPCL5.
    :NOPRTBDR
      OPTION=TOP
      ORIENT=PORTRAIT
      DATA =  240.
    :INITPRT
      DATA ='1B45'X.
    :CARRTN
      DATA ='0D'X.
    :LINEFEED
      DATA ='0A'X.
    :VARLSPC
      VAROFFSET=    3
      VARLEN=    3
      VARTYPE=CHRDEC
      CNVNUM=    1
      CNVDEN=   48
      DATA ='1B266C00000043'X.
    :PAGSIZXFM.
    :PAGSIZE
      PAGWTH=12240
      PAGLEN=15840
      DATA ='1B266C303241'X.
    :EPAGSIZXFM.
    :DWRSLT
      DRAWER=DRAWER2
      DATA ='1B266C3448'X.
    :FNTGRPE
      MINFID=  154
      FNTWTH=
     /* -0-1-2-3 */
       '22AD'X      /* 00- */
       'AEA7'X      /* 01- */
      EURODATA=''X.
:EWSCST.
";

    #[test]
    fn reads_the_tag_language() {
        let w = compile(IBM_FORM).unwrap();
        assert_eq!(w.init, b"\x1bE");
        assert_eq!(w.newline, b"\r\n");
        assert_eq!(w.lpi[&6], b"\x1b&l008C");
        assert_eq!(w.lpi[&8], b"\x1b&l006C");
        assert_eq!(w.papers.len(), 1);
        assert_eq!(
            (
                w.papers[0].name.as_str(),
                w.papers[0].width,
                w.papers[0].height
            ),
            ("LETTER", 850, 1100)
        );
        assert_eq!(w.drawer[&2], b"\x1b&l4H");
        let t = tags(IBM_FORM).unwrap();
        let f = t.iter().find(|t| t.name == "FNTGRPE").unwrap();
        assert_eq!(
            f.get("FNTWTH"),
            Some(&Val::Bytes(vec![0x22, 0xAD, 0xAE, 0xA7]))
        );
    }

    #[test]
    fn what_it_writes_reads_back() {
        let mut w = compile(IBM_FORM).unwrap();
        w.copies = b"\x1b&l&nX".to_vec();
        let back = compile(&write_pcl(&w, "test")).unwrap();
        assert_eq!(back.init, w.init);
        assert_eq!(back.papers, w.papers);
        assert_eq!(back.lpi, w.lpi);
        assert_eq!(back.copies, w.copies);
        assert_eq!(back.drawer, w.drawer);
    }

    #[test]
    fn errors() {
        assert!(compile(":INITPRT DATA='1B'X.").is_err());
        assert!(compile(":WSCST DEVCLASS=TRANSFORM.\n:INITPRT DATA='1B4'X.").is_err());
        assert!(compile(":WSCST DEVCLASS=TRANSFORM.\n:INITPRT DATA='1B45'X").is_err());
        assert!(is_tags("/* x */\n:WSCST DEVCLASS=TRANSFORM.\n:EWSCST."));
        assert!(!is_tags("INIT = \"\""));
    }
}
