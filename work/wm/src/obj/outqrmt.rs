// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The remote section of an `*OUTQ` (version 2): where a remote writer
//! sends the queue's files (`CRTOUTQ RMTSYS() RMTPRTQ() CNNTYPE(*IP)`)
//! and how (`DESTTYPE`, `TRANSFORM`, `MFRTYPMDL`, `DESTOPT`, `SEPPAGE`,
//! `USRDTATFM`). The remote system and printer queue themselves are in
//! the fixed part (`docs/WORKMGMT.md`, `RemoteOutputQueue`).

use std::fmt::Write as _;

use ipc::wire_struct;

use crate::cmd::Params;
use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The section.
pub const LAYOUT: Layout = Layout {
    name: "RemoteOutputQueue",
    size: Some(512),
    fields: &[
        field("DestType", 0, 1),
        field("Transform", 1, 1),
        field("SepPage", 2, 1),
        field("Reserved", 3, 1),
        field("MsgqName", 4, 10),
        field("MsgqLib", 14, 10),
        field("MfrTypMdl", 24, 17),
        field("Class", 41, 1),
        field("Reserved2", 42, 2),
        field("WscstName", 44, 10),
        field("WscstLib", 54, 10),
        field("UsrDtaTfmName", 64, 10),
        field("UsrDtaTfmLib", 74, 10),
        field("UsrDrvPgmName", 84, 10),
        field("UsrDrvPgmLib", 94, 10),
        field("IntNetAdr", 104, 45),
        field("Reserved3", 149, 1),
        field("Fcb", 150, 8),
        field("Reserved4", 158, 2),
        field("DestOpt", 160, 128),
        field("UsrDfnOpt", 288, 40),
        field("Reserved5", 328, 184),
    ],
};

/// `DESTTYPE` by code.
pub const DESTTYPE: [&str; 6] = ["*OTHER", "*OS400", "*OS400V2", "*PSF2", "*S390", "*NDS"];

/// The remote attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteQ {
    /// `DESTTYPE`, an index of [`DESTTYPE`].
    pub desttype: u8,
    /// `TRANSFORM(*YES)`.
    pub transform: bool,
    /// `SEPPAGE(*YES)`.
    pub seppage: bool,
    /// `MSGQ`: where the writer's messages go, `lib/name`.
    pub msgq: String,
    /// `MFRTYPMDL`.
    pub mfrtypmdl: String,
    /// `CLASS` (`VMMVSCLASS`).
    pub class: String,
    /// `WSCST`, `lib/name`; blank `*NONE`.
    pub wscst: String,
    /// `USRDTATFM`, `lib/name`; blank `*NONE`.
    pub usrdtatfm: String,
    /// `USRDRVPGM`, `lib/name`; blank `*NONE`.
    pub usrdrvpgm: String,
    /// `INTNETADR`.
    pub intnetadr: String,
    /// `FCB`; blank `*NONE`.
    pub fcb: String,
    /// `DESTOPT`; blank `*NONE`.
    pub destopt: String,
    /// `USRDFNOPT`, blank separated; blank `*NONE`.
    pub usrdfnopt: String,
}

wire_struct!(RemoteQ {
    desttype,
    transform,
    seppage,
    msgq,
    mfrtypmdl,
    class,
    wscst,
    usrdtatfm,
    usrdrvpgm,
    intnetadr,
    fcb,
    destopt,
    usrdfnopt
});

impl Default for RemoteQ {
    fn default() -> Self {
        Self {
            desttype: 0,
            transform: true,
            seppage: true,
            msgq: "*LIBL/QSYSOPR".into(),
            mfrtypmdl: "*IBM42011".into(),
            class: "A".into(),
            wscst: String::new(),
            usrdtatfm: String::new(),
            usrdrvpgm: String::new(),
            intnetadr: String::new(),
            fcb: String::new(),
            destopt: String::new(),
            usrdfnopt: String::new(),
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

fn qualified(v: &str) -> String {
    let v = v.trim().to_ascii_uppercase();
    if v.is_empty() || v == "*NONE" {
        String::new()
    } else if v.contains('/') {
        v
    } else {
        format!("*LIBL/{v}")
    }
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        v[1..v.len() - 1].replace("''", "'")
    } else {
        v.to_string()
    }
}

impl RemoteQ {
    /// The section's bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_uint("DestType", u64::from(self.desttype));
        r.set_flag("Transform", self.transform);
        r.set_flag("SepPage", self.seppage);
        r.set_text("MfrTypMdl", &self.mfrtypmdl);
        r.set_text("Class", &self.class);
        for (q, n, l) in [
            (&self.msgq, "MsgqName", "MsgqLib"),
            (&self.wscst, "WscstName", "WscstLib"),
            (&self.usrdtatfm, "UsrDtaTfmName", "UsrDtaTfmLib"),
            (&self.usrdrvpgm, "UsrDrvPgmName", "UsrDrvPgmLib"),
        ] {
            let (lib, name) = split(q);
            r.set_text(n, name);
            r.set_text(l, lib);
        }
        r.set_text("IntNetAdr", &self.intnetadr);
        r.set_text("Fcb", &self.fcb);
        r.set_text("DestOpt", &self.destopt);
        r.set_text("UsrDfnOpt", &self.usrdfnopt);
        r.buf
    }

    /// Read the section back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        Ok(Self {
            desttype: u8::try_from(r.uint("DestType")).unwrap_or(0),
            transform: r.flag("Transform"),
            seppage: r.flag("SepPage"),
            msgq: join(&r.text("MsgqLib"), &r.text("MsgqName")),
            mfrtypmdl: r.text("MfrTypMdl"),
            class: r.text("Class"),
            wscst: join(&r.text("WscstLib"), &r.text("WscstName")),
            usrdtatfm: join(&r.text("UsrDtaTfmLib"), &r.text("UsrDtaTfmName")),
            usrdrvpgm: join(&r.text("UsrDrvPgmLib"), &r.text("UsrDrvPgmName")),
            intnetadr: r.text("IntNetAdr"),
            fcb: r.text("Fcb"),
            destopt: r.text("DestOpt"),
            usrdfnopt: r.text("UsrDfnOpt"),
        })
    }

    /// The `DESTTYPE` special value.
    #[must_use]
    pub fn desttype_name(&self) -> &'static str {
        DESTTYPE.get(usize::from(self.desttype)).unwrap_or(&"*OTHER")
    }

    /// One `CRTOUTQ` or `CHGOUTQ` keyword of this section; `false`
    /// when it is not one of them.
    pub fn apply_one(&mut self, k: &str, v: &str) -> Result<bool> {
        let bad = || Error::value("CPD0084", format!("{k}({v}) is not valid"));
        let up = v.trim().to_ascii_uppercase();
        match k {
            "DESTTYPE" => {
                let up = if up == "*IBMI" { "*OS400".to_string() } else { up };
                self.desttype = u8::try_from(DESTTYPE.iter().position(|d| *d == up).ok_or_else(bad)?)
                    .unwrap_or(0);
            }
            "TRANSFORM" => {
                self.transform = match up.as_str() {
                    "*YES" => true,
                    "*NO" => false,
                    _ => return Err(bad()),
                };
            }
            "SEPPAGE" => {
                self.seppage = match up.as_str() {
                    "*YES" => true,
                    "*NO" => false,
                    _ => return Err(bad()),
                };
            }
            "MSGQ" => {
                self.msgq = if up.is_empty() {
                    "*LIBL/QSYSOPR".into()
                } else {
                    qualified(v)
                };
            }
            "MFRTYPMDL" => self.mfrtypmdl = up,
            "CLASS" | "VMMVSCLASS" => {
                if up.len() != 1 || !up.chars().all(|c| c.is_ascii_alphanumeric()) {
                    return Err(bad());
                }
                self.class = up;
            }
            "WSCST" => self.wscst = qualified(v),
            "USRDTATFM" => self.usrdtatfm = qualified(v),
            "USRDRVPGM" => self.usrdrvpgm = qualified(v),
            "INTNETADR" => {
                let a = unquote(v);
                self.intnetadr = if a.eq_ignore_ascii_case("*NONE") {
                    String::new()
                } else {
                    a
                };
                if self.intnetadr.len() > 45 {
                    return Err(bad());
                }
            }
            "FCB" => self.fcb = if up == "*NONE" { String::new() } else { up },
            "DESTOPT" => {
                let d = unquote(v);
                self.destopt = if d.eq_ignore_ascii_case("*NONE") {
                    String::new()
                } else {
                    d
                };
            }
            "USRDFNOPT" => {
                let items: Vec<String> = Params::list(v)
                    .into_iter()
                    .map(|s| s.to_ascii_uppercase())
                    .collect();
                let items = if items.is_empty() {
                    vec![up]
                } else {
                    items
                };
                self.usrdfnopt = if items.iter().any(|s| s == "*NONE") {
                    String::new()
                } else {
                    items.join(" ")
                };
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The `WRKOUTQD` lines of the section.
    #[must_use]
    pub fn describe(&self) -> String {
        let none = |s: &str| {
            if s.is_empty() {
                "*NONE".to_string()
            } else {
                s.to_string()
            }
        };
        let yn = |b: bool| if b { "*YES" } else { "*NO" };
        let mut out = String::new();
        let _ = writeln!(out, "Destination type . . : {}", self.desttype_name());
        let _ = writeln!(out, "Host print transform : {}", yn(self.transform));
        let _ = writeln!(out, "Manufacturer type  . : {}", none(&self.mfrtypmdl));
        let _ = writeln!(out, "Internet address . . : {}", none(&self.intnetadr));
        let _ = writeln!(out, "Destination options  : {}", none(&self.destopt));
        let _ = writeln!(out, "Print separator page : {}", yn(self.seppage));
        let _ = writeln!(out, "User data transform  : {}", none(&self.usrdtatfm));
        let _ = writeln!(out, "Writer messages  . . : {}", none(&self.msgq));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_values() {
        let mut r = RemoteQ::default();
        for (k, v) in [
            ("DESTTYPE", "*os400"),
            ("TRANSFORM", "*NO"),
            ("SEPPAGE", "*NO"),
            ("MFRTYPMDL", "*hp4"),
            ("INTNETADR", "'10.1.2.3'"),
            ("DESTOPT", "'XAIX XAUTOQ'"),
            ("USRDTATFM", "MYLIB/MYTFM"),
            ("USRDFNOPT", "(A B)"),
            ("MSGQ", "QGPL/WTRMSGS"),
        ] {
            assert!(r.apply_one(k, v).unwrap(), "{k}");
        }
        assert!(!r.apply_one("TEXT", "x").unwrap());
        assert_eq!(r.desttype_name(), "*OS400");
        assert_eq!(r.destopt, "XAIX XAUTOQ");
        assert_eq!(r.usrdfnopt, "A B");
        assert_eq!(RemoteQ::decode(&r.encode()).unwrap(), r);
        assert!(r.apply_one("DESTTYPE", "*FAX").is_err());
        assert!(r.describe().contains("10.1.2.3"));
    }
}
