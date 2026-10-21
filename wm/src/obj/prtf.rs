// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*FILE PRTF`: a printer file's attributes, those of `CRTPRTF` that
//! are not DDS, and its compiled DDS formats as an opaque tail (the
//! `spool` crate's encoding; empty for a program-described file).
//! `OVRPRTF` applies the same keywords over them at open.

use std::fmt::Write as _;

use ipc::wire_struct;

use crate::cmd::Params;
use crate::obj::splf::Schedule;
use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The fixed part.
pub const LAYOUT: Layout = Layout {
    name: "PrinterFile",
    size: Some(256),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("PageLen", 10, 2),
        field("PageWidth", 12, 2),
        field("Lpi", 14, 1),
        field("Cpi", 15, 1),
        field("Ovrflw", 16, 2),
        field("Fold", 18, 1),
        field("RplUnprt", 19, 1),
        field("RplChar", 20, 1),
        field("Align", 21, 1),
        field("PrtQlty", 22, 1),
        field("Hold", 23, 1),
        field("Save", 24, 1),
        field("OutPty", 25, 1),
        field("Copies", 26, 2),
        field("Schedule", 28, 1),
        field("CtlChar", 29, 1),
        field("FileSep", 30, 2),
        field("MaxRcds", 32, 4),
        field("Drawer", 36, 1),
        field("Duplex", 37, 1),
        field("PagRtt", 38, 2),
        field("Share", 40, 1),
        field("LvlChk", 41, 1),
        field("Reserved", 42, 2),
        field("FormType", 44, 10),
        field("OutqName", 54, 10),
        field("OutqLib", 64, 10),
        field("Dev", 74, 10),
        field("UsrDta", 84, 10),
        field("SplfName", 94, 10),
        field("Font", 104, 10),
        field("DevType", 114, 10),
        field("ChrId", 124, 10),
        field("Text", 134, 50),
        field("PrtTxt", 184, 30),
        field("Reserved2", 214, 38),
        field("FmtLen", 252, 4),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMPRTF";
const VERSION: u64 = 1;

/// A printer file.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // CRTPRTF's yes/no keywords
pub struct Prtf {
    /// `PAGESIZE` length.
    pub page_len: u16,
    /// `PAGESIZE` width.
    pub page_width: u16,
    /// `LPI`: 3, 4, 6, 8, 9, 12.
    pub lpi: u8,
    /// `CPI`: 5, 10, 12, 13, 15, 16, 18, 20.
    pub cpi: u8,
    /// `OVRFLW`.
    pub ovrflw: u16,
    /// `FOLD`.
    pub fold: bool,
    /// `RPLUNPRT`.
    pub rplunprt: bool,
    /// Its replacement character.
    pub rplchar: char,
    /// `ALIGN`.
    pub align: bool,
    /// `PRTQLTY`: 0 `*STD`, 1 `*DRAFT`, 2 `*NLQ`, 3 `*DEVD`.
    pub prtqlty: u8,
    /// `HOLD`.
    pub hold: bool,
    /// `SAVE`.
    pub save: bool,
    /// `OUTPTY`; 0 is `*JOB`.
    pub outpty: u8,
    /// `COPIES`.
    pub copies: u16,
    /// `SCHEDULE`.
    pub schedule: Schedule,
    /// `CTLCHAR(*FCFC)`: each record starts with a carriage control.
    pub fcfc: bool,
    /// `FILESEP`.
    pub filesep: u16,
    /// `MAXRCDS`; 0 is `*NOMAX`.
    pub maxrcds: u32,
    /// `DRAWER`.
    pub drawer: u8,
    /// `DUPLEX`: 0 `*NO`, 1 `*YES`, 2 `*TUMBLE`.
    pub duplex: u8,
    /// `PAGRTT` in degrees; 0xFFFF is `*DEVD`.
    pub pagrtt: u16,
    /// `SHARE`.
    pub share: bool,
    /// `LVLCHK`.
    pub lvlchk: bool,
    /// `FORMTYPE`.
    pub formtype: String,
    /// `OUTQ`, `lib/name` or `*JOB`.
    pub outq: String,
    /// `DEV`: `*JOB`, `*SYSVAL` or a device.
    pub dev: String,
    /// `USRDTA`.
    pub usrdta: String,
    /// `SPLFNAME`: `*FILE` or a name.
    pub splfname: String,
    /// `FONT`.
    pub font: String,
    /// `DEVTYPE`: `*SCS`.
    pub devtype: String,
    /// `CHRID`.
    pub chrid: String,
    /// `TEXT`.
    pub text: String,
    /// `PRTTXT`.
    pub prttxt: String,
    /// The compiled DDS formats; empty for a program-described file.
    pub formats: Vec<u8>,
}

wire_struct!(Prtf {
    page_len,
    page_width,
    lpi,
    cpi,
    ovrflw,
    fold,
    rplunprt,
    rplchar,
    align,
    prtqlty,
    hold,
    save,
    outpty,
    copies,
    schedule,
    fcfc,
    filesep,
    maxrcds,
    drawer,
    duplex,
    pagrtt,
    share,
    lvlchk,
    formtype,
    outq,
    dev,
    usrdta,
    splfname,
    font,
    devtype,
    chrid,
    text,
    prttxt,
    formats
});

impl Default for Prtf {
    fn default() -> Self {
        Self {
            page_len: 66,
            page_width: 132,
            lpi: 6,
            cpi: 10,
            ovrflw: 60,
            fold: false,
            rplunprt: true,
            rplchar: ' ',
            align: false,
            prtqlty: 3,
            hold: false,
            save: false,
            outpty: 0,
            copies: 1,
            schedule: Schedule::FileEnd,
            fcfc: false,
            filesep: 0,
            maxrcds: 100_000,
            drawer: 1,
            duplex: 0,
            pagrtt: 0xFFFF,
            share: false,
            lvlchk: true,
            formtype: "*STD".into(),
            outq: "*JOB".into(),
            dev: "*JOB".into(),
            usrdta: String::new(),
            splfname: "*FILE".into(),
            font: "*CPI".into(),
            devtype: "*SCS".into(),
            chrid: "*DEVD".into(),
            text: String::new(),
            prttxt: "*JOB".into(),
            formats: Vec::new(),
        }
    }
}

fn split_q(q: &str) -> (&str, &str) {
    q.split_once('/').unwrap_or(("", q))
}

const QUALITIES: [&str; 4] = ["*STD", "*DRAFT", "*NLQ", "*DEVD"];

impl Prtf {
    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_uint("PageLen", u64::from(self.page_len));
        r.set_uint("PageWidth", u64::from(self.page_width));
        r.set_uint("Lpi", u64::from(self.lpi));
        r.set_uint("Cpi", u64::from(self.cpi));
        r.set_uint("Ovrflw", u64::from(self.ovrflw));
        r.set_flag("Fold", self.fold);
        r.set_flag("RplUnprt", self.rplunprt);
        r.set_text("RplChar", &self.rplchar.to_string());
        r.set_flag("Align", self.align);
        r.set_uint("PrtQlty", u64::from(self.prtqlty));
        r.set_flag("Hold", self.hold);
        r.set_flag("Save", self.save);
        r.set_uint("OutPty", u64::from(self.outpty));
        r.set_uint("Copies", u64::from(self.copies));
        r.set_uint(
            "Schedule",
            match self.schedule {
                Schedule::Immed => 0,
                Schedule::FileEnd => 1,
                Schedule::JobEnd => 2,
            },
        );
        r.set_flag("CtlChar", self.fcfc);
        r.set_uint("FileSep", u64::from(self.filesep));
        r.set_uint("MaxRcds", u64::from(self.maxrcds));
        r.set_uint("Drawer", u64::from(self.drawer));
        r.set_uint("Duplex", u64::from(self.duplex));
        r.set_uint("PagRtt", u64::from(self.pagrtt));
        r.set_flag("Share", self.share);
        r.set_flag("LvlChk", self.lvlchk);
        r.set_text("FormType", &self.formtype);
        let (ol, on) = split_q(&self.outq);
        r.set_text("OutqName", on);
        r.set_text("OutqLib", ol);
        r.set_text("Dev", &self.dev);
        r.set_text("UsrDta", &self.usrdta);
        r.set_text("SplfName", &self.splfname);
        r.set_text("Font", &self.font);
        r.set_text("DevType", &self.devtype);
        r.set_text("ChrId", &self.chrid);
        r.set_text("Text", &self.text);
        r.set_text("PrtTxt", &self.prttxt);
        r.set_uint("FmtLen", self.formats.len() as u64);
        let mut out = r.buf;
        out.extend_from_slice(&self.formats);
        out
    }

    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a printer file".into()));
        }
        let n = usize::try_from(r.uint("FmtLen")).unwrap_or(usize::MAX);
        let formats = r
            .tail()
            .get(..n)
            .ok_or_else(|| Error::Corrupt("printer file formats truncated".into()))?
            .to_vec();
        let small = |f: &str| u8::try_from(r.uint(f)).unwrap_or(0);
        let half = |f: &str| u16::try_from(r.uint(f)).unwrap_or(0);
        let ol = r.text("OutqLib");
        let on = r.text("OutqName");
        Ok(Self {
            page_len: half("PageLen"),
            page_width: half("PageWidth"),
            lpi: small("Lpi"),
            cpi: small("Cpi"),
            ovrflw: half("Ovrflw"),
            fold: r.flag("Fold"),
            rplunprt: r.flag("RplUnprt"),
            rplchar: r.text("RplChar").chars().next().unwrap_or(' '),
            align: r.flag("Align"),
            prtqlty: small("PrtQlty"),
            hold: r.flag("Hold"),
            save: r.flag("Save"),
            outpty: small("OutPty"),
            copies: half("Copies"),
            schedule: match r.uint("Schedule") {
                0 => Schedule::Immed,
                2 => Schedule::JobEnd,
                _ => Schedule::FileEnd,
            },
            fcfc: r.flag("CtlChar"),
            filesep: half("FileSep"),
            maxrcds: u32::try_from(r.uint("MaxRcds")).unwrap_or(0),
            drawer: small("Drawer"),
            duplex: small("Duplex"),
            pagrtt: half("PagRtt"),
            share: r.flag("Share"),
            lvlchk: r.flag("LvlChk"),
            formtype: r.text("FormType"),
            outq: if ol.is_empty() { on } else { format!("{ol}/{on}") },
            dev: r.text("Dev"),
            usrdta: r.text("UsrDta"),
            splfname: r.text("SplfName"),
            font: r.text("Font"),
            devtype: r.text("DevType"),
            chrid: r.text("ChrId"),
            text: r.text("Text"),
            prttxt: r.text("PrtTxt"),
            formats,
        })
    }

    /// `CRTPRTF`, `CHGPRTF` and `OVRPRTF` keywords over these attributes.
    /// `FILE`, `SRCFILE`, `SRCMBR` and the override's own keywords are
    /// the caller's.
    #[allow(clippy::too_many_lines)] // one arm per keyword
    pub fn apply(&mut self, p: &Params) -> Result<()> {
        let bad = |k: &str, v: &str| Error::value("CPD0084", format!("{k}({v}) is not valid"));
        for (k, raw) in &p.kw {
            let v = raw.trim();
            let up = v.to_ascii_uppercase();
            let words: Vec<&str> = up.split_whitespace().collect();
            let num = |s: &str, lo: u32, hi: u32| -> Result<u32> {
                s.parse::<u32>()
                    .ok()
                    .filter(|n| (lo..=hi).contains(n))
                    .ok_or_else(|| bad(k, v))
            };
            let yes = || match up.as_str() {
                "*YES" => Ok(true),
                "*NO" => Ok(false),
                _ => Err(bad(k, v)),
            };
            match k.as_str() {
                "FILE" | "SRCFILE" | "SRCMBR" | "TOFILE" | "OVRSCOPE" | "SECURE" | "OPTION"
                | "GENLVL" | "FLAG" | "REPLACE" | "AUT" => {}
                "DEVTYPE" => {
                    if up != "*SCS" {
                        return Err(Error::value(
                            "CPD0084",
                            format!("DEVTYPE({v}) is not supported; *SCS is"),
                        ));
                    }
                }
                "PAGESIZE" => {
                    let l = words.first().ok_or_else(|| bad(k, v))?;
                    let w = words.get(1).ok_or_else(|| bad(k, v))?;
                    self.page_len = u16::try_from(num(l, 1, 255)?).unwrap_or(66);
                    self.page_width = u16::try_from(num(w, 1, 378)?).unwrap_or(132);
                }
                "LPI" => {
                    let n = num(&up, 3, 12)?;
                    if ![3, 4, 6, 8, 9, 12].contains(&n) {
                        return Err(bad(k, v));
                    }
                    self.lpi = u8::try_from(n).unwrap_or(6);
                }
                "CPI" => {
                    let n = num(&up, 5, 20)?;
                    if ![5, 10, 12, 13, 15, 16, 18, 20].contains(&n) {
                        return Err(bad(k, v));
                    }
                    self.cpi = u8::try_from(n).unwrap_or(10);
                }
                "OVRFLW" => self.ovrflw = u16::try_from(num(&up, 1, 255)?).unwrap_or(60),
                "FOLD" => self.fold = yes()?,
                "RPLUNPRT" => {
                    self.rplunprt = words.first() == Some(&"*YES");
                    if let Some(c) = words.get(1) {
                        self.rplchar = c.trim_matches('\'').chars().next().unwrap_or(' ');
                    }
                }
                "ALIGN" => self.align = yes()?,
                "PRTQLTY" => {
                    let i = QUALITIES.iter().position(|q| *q == up).ok_or_else(|| bad(k, v))?;
                    self.prtqlty = u8::try_from(i).unwrap_or(3);
                }
                "HOLD" => self.hold = yes()?,
                "SAVE" => self.save = yes()?,
                "OUTPTY" => {
                    self.outpty = if up == "*JOB" {
                        0
                    } else {
                        u8::try_from(num(&up, 1, 9)?).unwrap_or(5)
                    };
                }
                "COPIES" => self.copies = u16::try_from(num(&up, 1, 255)?).unwrap_or(1),
                "SCHEDULE" => self.schedule = Schedule::parse(&up).ok_or_else(|| bad(k, v))?,
                "CTLCHAR" => {
                    self.fcfc = match up.as_str() {
                        "*FCFC" => true,
                        "*NONE" => false,
                        _ => return Err(bad(k, v)),
                    };
                }
                "FILESEP" => self.filesep = u16::try_from(num(&up, 0, 9)?).unwrap_or(0),
                "MAXRCDS" => {
                    self.maxrcds = if up == "*NOMAX" {
                        0
                    } else {
                        num(&up, 1, 999_999)?
                    };
                }
                "DRAWER" => self.drawer = u8::try_from(num(&up, 1, 255)?).unwrap_or(1),
                "DUPLEX" => {
                    self.duplex = match up.as_str() {
                        "*NO" => 0,
                        "*YES" => 1,
                        "*TUMBLE" => 2,
                        _ => return Err(bad(k, v)),
                    };
                }
                "PAGRTT" => {
                    self.pagrtt = match up.as_str() {
                        "*DEVD" | "*AUTO" | "*COR" => 0xFFFF,
                        d => {
                            let n = num(d, 0, 270)?;
                            if n % 90 != 0 {
                                return Err(bad(k, v));
                            }
                            u16::try_from(n).unwrap_or(0)
                        }
                    };
                }
                "SHARE" => self.share = yes()?,
                "LVLCHK" => self.lvlchk = yes()?,
                "FORMTYPE" => self.formtype = up,
                "OUTQ" => self.outq = up,
                "DEV" => self.dev = up,
                "USRDTA" => self.usrdta = v.trim_matches('\'').to_string(),
                "SPLFNAME" => self.splfname = up,
                "FONT" => self.font = words.first().map_or(up.clone(), |w| (*w).to_string()),
                "CHRID" => self.chrid = up,
                "TEXT" => self.text = v.trim_matches('\'').replace("''", "'"),
                "PRTTXT" => self.prttxt = v.trim_matches('\'').to_string(),
                _ => {
                    return Err(Error::value(
                        "CPD0043",
                        format!("keyword {k} not valid for this command"),
                    ));
                }
            }
        }
        if self.ovrflw > self.page_len {
            return Err(Error::value(
                "CPF7302",
                format!(
                    "OVRFLW({}) is past the page length {}",
                    self.ovrflw, self.page_len
                ),
            ));
        }
        Ok(())
    }

    /// `DSPFD` of a printer file, as text.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let yn = |b: bool| if b { "*YES" } else { "*NO" };
        let rows: [(&str, String); 16] = [
            ("File attribute", "PRTF".into()),
            (
                "Externally described",
                yn(!self.formats.is_empty()).into(),
            ),
            ("Device type", self.devtype.clone()),
            (
                "Page size",
                format!("{} by {}", self.page_len, self.page_width),
            ),
            ("Lines per inch", self.lpi.to_string()),
            ("Characters per inch", self.cpi.to_string()),
            ("Overflow line number", self.ovrflw.to_string()),
            (
                "Control character",
                if self.fcfc { "*FCFC" } else { "*NONE" }.into(),
            ),
            ("Output queue", self.outq.clone()),
            ("Form type", self.formtype.clone()),
            ("Copies", self.copies.to_string()),
            ("Hold spooled file", yn(self.hold).into()),
            ("Save spooled file", yn(self.save).into()),
            ("Schedule", self.schedule.name().into()),
            ("Spooled file name", self.splfname.clone()),
            ("Text", self.text.clone()),
        ];
        for (label, value) in rows {
            let pad = format!("{label} {}", ". ".repeat(16));
            let _ = writeln!(out, "  {} : {value}", &pad[..32]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_keywords() {
        let mut f = Prtf::default();
        f.apply(&Params::parse(
            "x FILE(Q/X) PAGESIZE(88 198) LPI(8) CPI(15) OVRFLW(80) CTLCHAR(*FCFC) \
             OUTQ(QGPL/QPRINT2) SCHEDULE(*JOBEND) TEXT('Report')",
        ))
        .unwrap();
        f.formats = vec![1, 2, 3];
        let back = Prtf::decode(&f.encode()).unwrap();
        assert_eq!(back, f);
        assert_eq!((back.page_len, back.page_width, back.lpi), (88, 198, 8));
        assert!(Prtf::default().apply(&Params::parse("x LPI(7)")).is_err());
        assert!(Prtf::default().apply(&Params::parse("x OVRFLW(90)")).is_err());
        assert!(f.describe().contains("*FCFC"));
    }
}
