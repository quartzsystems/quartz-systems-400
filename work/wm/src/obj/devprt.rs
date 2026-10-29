// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The printer section of a `*DEVD`: what `CRTDEVPRT DEVCLS(*LAN)`
//! keeps for a LAN printer (`LANATTACH(*IP)`, the port, the remote
//! location, the host print transform, the system driver program).
//! It follows the device description's 192 bytes when `PrtLen` says
//! so (`docs/WORKMGMT.md`, `PrinterDevice`).

use std::fmt::Write as _;

use crate::cmd::Params;
use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The section.
pub const LAYOUT: Layout = Layout {
    name: "PrinterDevice",
    size: Some(512),
    fields: &[
        field("LanAttach", 0, 1),
        field("Transform", 1, 1),
        field("SecureCnn", 2, 1),
        field("PrtQlty", 3, 1),
        field("Port", 4, 4),
        field("ChridCs", 8, 2),
        field("ChridCp", 10, 2),
        field("FontId", 12, 10),
        field("FontPt", 22, 2),
        field("FormFeed", 24, 1),
        field("SepDrawer", 25, 1),
        field("PprSrc1", 26, 1),
        field("PprSrc2", 27, 1),
        field("Envelope", 28, 1),
        field("PrtErrMsg", 29, 1),
        field("MaxPndRqs", 30, 1),
        field("Reserved", 31, 1),
        field("PrtRqsTmr", 32, 2),
        field("Reserved2", 34, 6),
        field("SysDrvPgm", 40, 10),
        field("MfrTypMdl", 50, 17),
        field("Reserved3", 67, 1),
        field("SepPgmName", 68, 10),
        field("SepPgmLib", 78, 10),
        field("WscstName", 88, 10),
        field("WscstLib", 98, 10),
        field("UsrDtaTfmName", 108, 10),
        field("UsrDtaTfmLib", 118, 10),
        field("UsrDrvPgmName", 128, 10),
        field("UsrDrvPgmLib", 138, 10),
        field("Reserved4", 148, 4),
        field("RmtLocName", 152, 255),
        field("Reserved5", 407, 105),
    ],
};

/// `LANATTACH`.
pub const LANATTACH: [&str; 3] = ["*LEXLINK", "*IP", "*USRDFN"];

/// `FORMFEED`, by code.
pub const FORMFEED: [&str; 5] = ["*TYPE", "*CONT", "*CUT", "*AUTOCUT", "*CONT2"];

/// `PPRSRC1`, `PPRSRC2` and `ENVELOPE`: IBM's codes for the paper
/// sizes (`*MFRTYPMDL` is 0).
pub const PAPER: [(&str, u8); 18] = [
    ("*MFRTYPMDL", 0x00),
    ("*LETTER", 0x01),
    ("*LEGAL", 0x02),
    ("*EXECUTIVE", 0x03),
    ("*A4", 0x04),
    ("*A5", 0x05),
    ("*B5", 0x06),
    ("*CONT80", 0x07),
    ("*CONT132", 0x08),
    ("*MONARCH", 0x09),
    ("*NUMBER9", 0x0A),
    ("*NUMBER10", 0x0B),
    ("*C5", 0x0C),
    ("*DL", 0x0D),
    ("*A3", 0x0E),
    ("*B4", 0x0F),
    ("*LEDGER", 0x10),
    ("*NONE", 0xFF),
];

/// The system driver programs.
pub const SYSDRVPGM: [&str; 5] = [
    "*HPPJLDRV",
    "*IBMPJLDRV",
    "*IBMSNMPDRV",
    "*NETSTNDRV",
    "*IBMIPPDRV",
];

/// A LAN printer.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct LanPrinter {
    /// `LANATTACH`: 0 `*LEXLINK`, 1 `*IP`, 2 `*USRDFN`.
    pub lanattach: u8,
    /// `PORT`; 0 when not given.
    pub port: u32,
    /// `RMTLOCNAME`: host name, address, or an IPP printer URI.
    pub rmtlocname: String,
    /// `TRANSFORM(*YES)`.
    pub transform: bool,
    /// `MFRTYPMDL`.
    pub mfrtypmdl: String,
    /// `SYSDRVPGM`; blank for none.
    pub sysdrvpgm: String,
    /// `SECURECNN(*YES)`.
    pub securecnn: bool,
    /// `PRTQLTY`: 0 `*STD`, 1 `*DRAFT`, 2 `*NLQ`.
    pub prtqlty: u8,
    /// `CHRID`: graphic character set and code page; `(0, 0)` `*SYSVAL`.
    pub chrid: (u16, u16),
    /// `FONT` identifier; blank for none.
    pub font: String,
    /// `FONT` point size in tenths; 0 `*NONE`.
    pub font_pt: u16,
    /// `FORMFEED`, an index of [`FORMFEED`].
    pub formfeed: u8,
    /// `SEPDRAWER`; 0 `*FILE`.
    pub sepdrawer: u8,
    /// `PPRSRC1`, IBM's code.
    pub pprsrc1: u8,
    /// `PPRSRC2`, IBM's code.
    pub pprsrc2: u8,
    /// `ENVELOPE`, IBM's code.
    pub envelope: u8,
    /// `PRTERRMSG(*INFO)`; `*INQ` otherwise.
    pub prterrmsg_info: bool,
    /// `MAXPNDRQS`.
    pub maxpndrqs: u8,
    /// `PRTRQSTMR`; 0 `*NOMAX`.
    pub prtrqstmr: u16,
    /// `SEPPGM`, `lib/name`; blank `*NONE`.
    pub seppgm: String,
    /// `WSCST`, `lib/name`; blank `*NONE`.
    pub wscst: String,
    /// `USRDTATFM`, `lib/name`; blank `*NONE`.
    pub usrdtatfm: String,
    /// `USRDRVPGM`, `lib/name`; blank `*NONE`.
    pub usrdrvpgm: String,
}

impl Default for LanPrinter {
    fn default() -> Self {
        Self {
            lanattach: 0,
            port: 0,
            rmtlocname: String::new(),
            transform: false,
            mfrtypmdl: String::new(),
            sysdrvpgm: String::new(),
            securecnn: false,
            prtqlty: 0,
            chrid: (0, 0),
            font: String::new(),
            font_pt: 0,
            formfeed: 0,
            sepdrawer: 0,
            pprsrc1: 0,
            pprsrc2: 0,
            envelope: 0,
            prterrmsg_info: false,
            maxpndrqs: 6,
            prtrqstmr: 0,
            seppgm: String::new(),
            wscst: String::new(),
            usrdtatfm: String::new(),
            usrdrvpgm: String::new(),
        }
    }
}

fn split(q: &str) -> (&str, &str) {
    q.split_once('/').unwrap_or(("", q))
}

fn join(lib: &str, name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        format!("{lib}/{name}")
    }
}

fn bad(k: &str, v: &str) -> Error {
    Error::value("CPD0084", format!("{k}({v}) is not valid"))
}

/// A qualified name in a keyword value: `LIB/NAME` or `NAME` (then
/// `*LIBL`), `*NONE` none.
fn qualified(v: &str) -> String {
    let v = v.trim().to_ascii_uppercase();
    if v.is_empty() || v == "*NONE" {
        return String::new();
    }
    if v.contains('/') {
        v
    } else {
        format!("*LIBL/{v}")
    }
}

/// The paper code of a special value.
#[must_use]
pub fn paper_code(v: &str) -> Option<u8> {
    PAPER.iter().find(|(n, _)| *n == v).map(|(_, c)| *c)
}

/// The special value of a paper code.
#[must_use]
pub fn paper_name(c: u8) -> &'static str {
    PAPER.iter().find(|(_, k)| *k == c).map_or("*MFRTYPMDL", |(n, _)| n)
}

impl LanPrinter {
    /// The section's bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_uint("LanAttach", u64::from(self.lanattach));
        r.set_flag("Transform", self.transform);
        r.set_flag("SecureCnn", self.securecnn);
        r.set_uint("PrtQlty", u64::from(self.prtqlty));
        r.set_uint("Port", u64::from(self.port));
        r.set_uint("ChridCs", u64::from(self.chrid.0));
        r.set_uint("ChridCp", u64::from(self.chrid.1));
        r.set_text("FontId", &self.font);
        r.set_uint("FontPt", u64::from(self.font_pt));
        r.set_uint("FormFeed", u64::from(self.formfeed));
        r.set_uint("SepDrawer", u64::from(self.sepdrawer));
        r.set_uint("PprSrc1", u64::from(self.pprsrc1));
        r.set_uint("PprSrc2", u64::from(self.pprsrc2));
        r.set_uint("Envelope", u64::from(self.envelope));
        r.set_flag("PrtErrMsg", self.prterrmsg_info);
        r.set_uint("MaxPndRqs", u64::from(self.maxpndrqs));
        r.set_uint("PrtRqsTmr", u64::from(self.prtrqstmr));
        r.set_text("SysDrvPgm", &self.sysdrvpgm);
        r.set_text("MfrTypMdl", &self.mfrtypmdl);
        for (q, n, l) in [
            (&self.seppgm, "SepPgmName", "SepPgmLib"),
            (&self.wscst, "WscstName", "WscstLib"),
            (&self.usrdtatfm, "UsrDtaTfmName", "UsrDtaTfmLib"),
            (&self.usrdrvpgm, "UsrDrvPgmName", "UsrDrvPgmLib"),
        ] {
            let (lib, name) = split(q);
            r.set_text(n, name);
            r.set_text(l, lib);
        }
        r.set_text("RmtLocName", &self.rmtlocname);
        r.buf
    }

    /// Read the section back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        let small = |n: &str| u8::try_from(r.uint(n)).unwrap_or(0);
        let half = |n: &str| u16::try_from(r.uint(n)).unwrap_or(0);
        Ok(Self {
            lanattach: small("LanAttach"),
            port: u32::try_from(r.uint("Port")).unwrap_or(0),
            rmtlocname: r.text("RmtLocName"),
            transform: r.flag("Transform"),
            mfrtypmdl: r.text("MfrTypMdl"),
            sysdrvpgm: r.text("SysDrvPgm"),
            securecnn: r.flag("SecureCnn"),
            prtqlty: small("PrtQlty"),
            chrid: (half("ChridCs"), half("ChridCp")),
            font: r.text("FontId"),
            font_pt: half("FontPt"),
            formfeed: small("FormFeed"),
            sepdrawer: small("SepDrawer"),
            pprsrc1: small("PprSrc1"),
            pprsrc2: small("PprSrc2"),
            envelope: small("Envelope"),
            prterrmsg_info: r.flag("PrtErrMsg"),
            maxpndrqs: small("MaxPndRqs"),
            prtrqstmr: half("PrtRqsTmr"),
            seppgm: join(&r.text("SepPgmLib"), &r.text("SepPgmName")),
            wscst: join(&r.text("WscstLib"), &r.text("WscstName")),
            usrdtatfm: join(&r.text("UsrDtaTfmLib"), &r.text("UsrDtaTfmName")),
            usrdrvpgm: join(&r.text("UsrDrvPgmLib"), &r.text("UsrDrvPgmName")),
        })
    }

    /// Whether the writer reaches it over TCP/IP.
    #[must_use]
    pub fn is_ip(&self) -> bool {
        self.lanattach == 1
    }

    /// `CHRID` as the transform takes it: `gcgid cpgid`, or `*SYSVAL`.
    #[must_use]
    pub fn chrid_text(&self) -> String {
        if self.chrid == (0, 0) {
            "*SYSVAL".into()
        } else {
            format!("{} {}", self.chrid.0, self.chrid.1)
        }
    }

    /// The `CRTDEVPRT` and `CHGDEVPRT` keywords this section keeps;
    /// the rest are the caller's (`DEVD`, `DEVCLS`, `TYPE`, `MODEL`,
    /// `ONLINE`, `TEXT`, `MSGQ`, ...), or accepted and not kept.
    #[allow(clippy::many_single_char_names, clippy::too_many_lines)]
    pub fn apply(&mut self, p: &Params) -> Result<()> {
        for (k, raw) in &p.kw {
            let v = raw.trim();
            let up = v.to_ascii_uppercase();
            if up == "*SAME" {
                continue;
            }
            match k.as_str() {
                "LANATTACH" => {
                    self.lanattach = u8::try_from(
                        LANATTACH.iter().position(|n| *n == up).ok_or_else(|| bad(k, v))?,
                    )
                    .unwrap_or(0);
                }
                "PORT" => {
                    self.port = v.parse().ok().filter(|n| *n <= 65535).ok_or_else(|| bad(k, v))?;
                }
                "RMTLOCNAME" => {
                    self.rmtlocname = v.trim_matches('\'').trim().to_string();
                }
                "TRANSFORM" => self.transform = up == "*YES",
                "SECURECNN" => self.securecnn = up == "*YES",
                "MFRTYPMDL" => self.mfrtypmdl = up,
                "SYSDRVPGM" => {
                    if !SYSDRVPGM.contains(&up.as_str()) && up != "*IPPDRV" && up != "*NONE" {
                        return Err(bad(k, v));
                    }
                    self.sysdrvpgm = if up == "*IPPDRV" {
                        "*IBMIPPDRV".into()
                    } else if up == "*NONE" {
                        String::new()
                    } else {
                        up
                    };
                }
                "PRTQLTY" => {
                    self.prtqlty = match up.as_str() {
                        "*STD" => 0,
                        "*DRAFT" => 1,
                        "*NLQ" => 2,
                        _ => return Err(bad(k, v)),
                    };
                }
                "CHRID" => {
                    self.chrid = if up == "*SYSVAL" {
                        (0, 0)
                    } else {
                        let n: Vec<u16> = up
                            .trim_matches(['(', ')'])
                            .split_whitespace()
                            .filter_map(|x| x.parse().ok())
                            .collect();
                        match n.as_slice() {
                            [a, b] => (*a, *b),
                            _ => return Err(bad(k, v)),
                        }
                    };
                }
                "FONT" => {
                    let mut w = up.trim_matches(['(', ')']).split_whitespace();
                    self.font = w.next().unwrap_or("").to_string();
                    self.font_pt = match w.next() {
                        None | Some("*NONE") => 0,
                        Some(pt) => {
                            let f: f64 = pt.parse().map_err(|_| bad(k, v))?;
                            // Tenths of a point, 000.1 to 999.9.
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            let t = (f * 10.0).round().clamp(0.0, 9999.0) as u16;
                            t
                        }
                    };
                }
                "FORMFEED" => {
                    self.formfeed = u8::try_from(
                        FORMFEED.iter().position(|n| *n == up).ok_or_else(|| bad(k, v))?,
                    )
                    .unwrap_or(0);
                }
                "SEPDRAWER" => {
                    self.sepdrawer = if up == "*FILE" {
                        0
                    } else {
                        v.parse().map_err(|_| bad(k, v))?
                    };
                }
                "PPRSRC1" => self.pprsrc1 = paper_code(&up).ok_or_else(|| bad(k, v))?,
                "PPRSRC2" => self.pprsrc2 = paper_code(&up).ok_or_else(|| bad(k, v))?,
                "ENVELOPE" => self.envelope = paper_code(&up).ok_or_else(|| bad(k, v))?,
                "PRTERRMSG" => self.prterrmsg_info = up == "*INFO",
                "MAXPNDRQS" => {
                    self.maxpndrqs = v.parse().ok().filter(|n| (1..=31).contains(n)).ok_or_else(|| bad(k, v))?;
                }
                "PRTRQSTMR" => {
                    self.prtrqstmr = if up == "*NOMAX" {
                        0
                    } else {
                        v.parse().map_err(|_| bad(k, v))?
                    };
                }
                "SEPPGM" => self.seppgm = qualified(v),
                "WSCST" => self.wscst = qualified(v),
                "USRDTATFM" => self.usrdtatfm = qualified(v),
                "USRDRVPGM" => self.usrdrvpgm = qualified(v),
                _ => {}
            }
        }
        Ok(())
    }

    /// The `DSPDEVD` lines of the section.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let none = |s: &str| {
            if s.is_empty() {
                "*NONE".to_string()
            } else {
                s.to_string()
            }
        };
        let yn = |b: bool| if b { "*YES" } else { "*NO" };
        let _ = writeln!(
            out,
            "  LAN attachment . . : {}",
            LANATTACH.get(usize::from(self.lanattach)).unwrap_or(&"*LEXLINK")
        );
        let _ = writeln!(out, "  Port number  . . . : {}", self.port);
        let _ = writeln!(out, "  Remote location  . : {}", none(&self.rmtlocname));
        let _ = writeln!(out, "  Host print transform : {}", yn(self.transform));
        let _ = writeln!(out, "  Manufacturer type  : {}", none(&self.mfrtypmdl));
        let _ = writeln!(out, "  Paper source 1 . . : {}", paper_name(self.pprsrc1));
        let _ = writeln!(out, "  Paper source 2 . . : {}", paper_name(self.pprsrc2));
        let _ = writeln!(out, "  Envelope source  . : {}", paper_name(self.envelope));
        let _ = writeln!(out, "  Character identifier : {}", self.chrid_text());
        let _ = writeln!(out, "  Workstation customizing : {}", none(&self.wscst));
        let _ = writeln!(out, "  User data transform  : {}", none(&self.usrdtatfm));
        let _ = writeln!(out, "  System driver program : {}", none(&self.sysdrvpgm));
        let _ = writeln!(out, "  Secure connection  : {}", yn(self.securecnn));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_apply() {
        let mut p = LanPrinter::default();
        p.apply(&Params::parse(
            "x LANATTACH(*IP) PORT(9100) RMTLOCNAME('lj4.example.com') TRANSFORM(*YES) \
             MFRTYPMDL(*hp4) SYSDRVPGM(*HPPJLDRV) PPRSRC1(*LETTER) PPRSRC2(*A4) \
             ENVELOPE(*NUMBER10) CHRID(697 850) FONT(011 10.5) WSCST(MYLIB/MYWS) \
             FORMFEED(*AUTOCUT) PRTERRMSG(*INFO)",
        ))
        .unwrap();
        assert!(p.is_ip());
        assert_eq!(p.mfrtypmdl, "*HP4");
        assert_eq!(p.chrid_text(), "697 850");
        assert_eq!((p.font.as_str(), p.font_pt), ("011", 105));
        assert_eq!(p.pprsrc2, 0x04);
        assert_eq!(p.wscst, "MYLIB/MYWS");
        let back = LanPrinter::decode(&p.encode()).unwrap();
        assert_eq!(back, p);
        assert!(p.describe().contains("lj4.example.com"));
        assert!(p.apply(&Params::parse("x SYSDRVPGM(*FOO)")).is_err());
        assert!(p.apply(&Params::parse("x PORT(70000)")).is_err());
        p.apply(&Params::parse("x SYSDRVPGM(*IPPDRV)")).unwrap();
        assert_eq!(p.sysdrvpgm, "*IBMIPPDRV");
    }
}
