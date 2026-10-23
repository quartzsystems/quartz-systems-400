// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*MENU` object's payload: `CRTMNU`'s attributes in one fixed
//! record (`docs/MENUS.md`, "Payload").

pub mod rec;

use rec::{Layout, Rec, field};

use crate::{Error, Qualified, Result};

/// The payload.
pub const LAYOUT: Layout = Layout {
    name: "Menu",
    size: Some(168),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Kind", 10, 1),
        field("CmdLin", 11, 1),
        field("CurLib", 12, 1),
        field("PrdLib", 13, 1),
        field("Reserved", 14, 2),
        field("Dspf", 16, 10),
        field("DspfLib", 26, 10),
        field("Msgf", 36, 10),
        field("MsgfLib", 46, 10),
        field("Pgm", 56, 10),
        field("PgmLib", 66, 10),
        field("Pnlgrp", 76, 10),
        field("PnlgrpLib", 86, 10),
        field("CurLibName", 96, 10),
        field("PrdLibName", 106, 10),
        field("Text", 116, 50),
        field("Reserved2", 166, 2),
    ],
};

const MAGIC: &[u8; 8] = b"QSUIMENU";
const VERSION: u64 = 1;

/// `TYPE`: how the menu is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuKind {
    /// `*DSPF`: a display file and a message file of commands.
    Dspf,
    /// `*PGM`: a program shows it.
    Pgm,
    /// `*UIM`: a panel group with a `:MENU.` panel named like the menu.
    #[default]
    Uim,
}

impl MenuKind {
    /// The keyword.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::Dspf => "*DSPF",
            Self::Pgm => "*PGM",
            Self::Uim => "*UIM",
        }
    }

    /// Parse the keyword.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "*DSPF" => Some(Self::Dspf),
            "*PGM" => Some(Self::Pgm),
            "*UIM" => Some(Self::Uim),
            _ => None,
        }
    }

    fn code(self) -> u64 {
        match self {
            Self::Dspf => 1,
            Self::Pgm => 2,
            Self::Uim => 3,
        }
    }

    fn from_code(c: u64) -> Option<Self> {
        match c {
            1 => Some(Self::Dspf),
            2 => Some(Self::Pgm),
            3 => Some(Self::Uim),
            _ => None,
        }
    }
}

/// `CMDLIN`: the command line under the options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CmdLin {
    /// `*LONG`: two lines.
    #[default]
    Long,
    /// `*SHORT`: one line.
    Short,
    /// `*NONE`: options only.
    None,
}

impl CmdLin {
    /// The keyword.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::Long => "*LONG",
            Self::Short => "*SHORT",
            Self::None => "*NONE",
        }
    }

    /// Parse the keyword.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "*LONG" => Some(Self::Long),
            "*SHORT" => Some(Self::Short),
            "*NONE" => Some(Self::None),
            _ => None,
        }
    }

    fn code(self) -> u64 {
        match self {
            Self::None => 0,
            Self::Short => 1,
            Self::Long => 2,
        }
    }

    fn from_code(c: u64) -> Option<Self> {
        match c {
            0 => Some(Self::None),
            1 => Some(Self::Short),
            2 => Some(Self::Long),
            _ => None,
        }
    }
}

/// `CURLIB`: the current library while the menu is shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CurLib {
    /// `*NOCHG`.
    #[default]
    NoChg,
    /// `*MNULIB`: the menu's library.
    MnuLib,
    /// `*CRTDFT`: no current library.
    CrtDft,
    /// A library.
    Name(String),
}

impl CurLib {
    /// The keyword or the name.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::NoChg => "*NOCHG".into(),
            Self::MnuLib => "*MNULIB".into(),
            Self::CrtDft => "*CRTDFT".into(),
            Self::Name(n) => n.clone(),
        }
    }

    /// Parse the keyword or the name.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_uppercase().as_str() {
            "*NOCHG" | "" => Self::NoChg,
            "*MNULIB" => Self::MnuLib,
            "*CRTDFT" => Self::CrtDft,
            n => Self::Name(n.to_string()),
        }
    }
}

/// `PRDLIB`: the product library while the menu is shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PrdLib {
    /// `*NOCHG`.
    #[default]
    NoChg,
    /// `*NONE`.
    None,
    /// A library.
    Name(String),
}

impl PrdLib {
    /// The keyword or the name.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::NoChg => "*NOCHG".into(),
            Self::None => "*NONE".into(),
            Self::Name(n) => n.clone(),
        }
    }

    /// Parse the keyword or the name.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_uppercase().as_str() {
            "*NOCHG" | "" => Self::NoChg,
            "*NONE" => Self::None,
            n => Self::Name(n.to_string()),
        }
    }
}

/// A menu's attributes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuAttrs {
    /// `TYPE`.
    pub kind: MenuKind,
    /// `DSPF` (`*DSPF` menus).
    pub dspf: Option<Qualified>,
    /// `MSGF` (`*DSPF` menus).
    pub msgf: Option<Qualified>,
    /// `PGM` (`*PGM` menus).
    pub pgm: Option<Qualified>,
    /// `PNLGRP` (`*UIM` menus).
    pub pnlgrp: Option<Qualified>,
    /// `CURLIB`.
    pub curlib: CurLib,
    /// `PRDLIB`.
    pub prdlib: PrdLib,
    /// `CMDLIN`.
    pub cmdlin: CmdLin,
    /// `TEXT`.
    pub text: String,
}

fn put_qual(r: &mut Rec<'_>, name: &str, lib: &str, q: Option<&Qualified>) {
    let (n, l) = q.map_or(("", ""), |q| (q.name.as_str(), q.lib.as_str()));
    r.set_text(name, n);
    r.set_text(lib, l);
}

fn get_qual(r: &Rec<'_>, name: &str, lib: &str) -> Option<Qualified> {
    let n = r.text(name);
    (!n.is_empty()).then(|| Qualified {
        name: n,
        lib: r.text(lib),
    })
}

impl MenuAttrs {
    /// A `*UIM` menu over a panel group, `CRTMNU`'s other defaults.
    #[must_use]
    pub fn uim(pnlgrp: Qualified, text: &str) -> Self {
        Self {
            kind: MenuKind::Uim,
            pnlgrp: Some(pnlgrp),
            text: text.to_string(),
            ..Self::default()
        }
    }

    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_uint("Kind", self.kind.code());
        r.set_uint("CmdLin", self.cmdlin.code());
        let (cur, cur_name) = match &self.curlib {
            CurLib::NoChg => (0, ""),
            CurLib::MnuLib => (1, ""),
            CurLib::CrtDft => (2, ""),
            CurLib::Name(n) => (3, n.as_str()),
        };
        r.set_uint("CurLib", cur);
        r.set_text("CurLibName", cur_name);
        let (prd, prd_name) = match &self.prdlib {
            PrdLib::NoChg => (0, ""),
            PrdLib::None => (1, ""),
            PrdLib::Name(n) => (2, n.as_str()),
        };
        r.set_uint("PrdLib", prd);
        r.set_text("PrdLibName", prd_name);
        put_qual(&mut r, "Dspf", "DspfLib", self.dspf.as_ref());
        put_qual(&mut r, "Msgf", "MsgfLib", self.msgf.as_ref());
        put_qual(&mut r, "Pgm", "PgmLib", self.pgm.as_ref());
        put_qual(&mut r, "Pnlgrp", "PnlgrpLib", self.pnlgrp.as_ref());
        r.set_text("Text", &self.text);
        r.buf
    }

    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a menu".into()));
        }
        let kind = MenuKind::from_code(r.uint("Kind"))
            .ok_or_else(|| Error::Corrupt("menu type code".into()))?;
        let cmdlin = CmdLin::from_code(r.uint("CmdLin"))
            .ok_or_else(|| Error::Corrupt("menu command line code".into()))?;
        let curlib = match r.uint("CurLib") {
            0 => CurLib::NoChg,
            1 => CurLib::MnuLib,
            2 => CurLib::CrtDft,
            3 => CurLib::Name(r.text("CurLibName")),
            _ => return Err(Error::Corrupt("menu current library code".into())),
        };
        let prdlib = match r.uint("PrdLib") {
            0 => PrdLib::NoChg,
            1 => PrdLib::None,
            2 => PrdLib::Name(r.text("PrdLibName")),
            _ => return Err(Error::Corrupt("menu product library code".into())),
        };
        Ok(Self {
            kind,
            dspf: get_qual(&r, "Dspf", "DspfLib"),
            msgf: get_qual(&r, "Msgf", "MsgfLib"),
            pgm: get_qual(&r, "Pgm", "PgmLib"),
            pnlgrp: get_qual(&r, "Pnlgrp", "PnlgrpLib"),
            curlib,
            prdlib,
            cmdlin,
            text: r.text("Text"),
        })
    }

    /// The attributes a `TYPE` needs are present.
    pub fn validate(&self) -> Result<()> {
        let need = |what: &str, present: bool| {
            if present {
                Ok(())
            } else {
                Err(Error::value(
                    "CPD0085",
                    format!("{what} is needed for TYPE({})", self.kind.text()),
                ))
            }
        };
        match self.kind {
            MenuKind::Dspf => {
                need("DSPF", self.dspf.is_some())?;
                need("MSGF", self.msgf.is_some())
            }
            MenuKind::Pgm => need("PGM", self.pgm.is_some()),
            MenuKind::Uim => need("PNLGRP", self.pnlgrp.is_some()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let m = MenuAttrs::uim(Qualified::qsys("CMDJOB"), "Job Commands");
        assert_eq!(m.encode().len(), 168);
        assert_eq!(MenuAttrs::decode(&m.encode()).unwrap(), m);
        let d = MenuAttrs {
            kind: MenuKind::Dspf,
            dspf: Some(Qualified::new("*LIBL", "MYMENU")),
            msgf: Some(Qualified::new("MYLIB", "MYMENU")),
            pgm: None,
            pnlgrp: None,
            curlib: CurLib::Name("MYLIB".into()),
            prdlib: PrdLib::None,
            cmdlin: CmdLin::Short,
            text: "A display-file menu".into(),
        };
        assert_eq!(MenuAttrs::decode(&d.encode()).unwrap(), d);
        let p = MenuAttrs {
            kind: MenuKind::Pgm,
            pgm: Some(Qualified::qsys("MENUPGM")),
            curlib: CurLib::CrtDft,
            prdlib: PrdLib::Name("PRDLIB".into()),
            cmdlin: CmdLin::None,
            ..MenuAttrs::default()
        };
        assert_eq!(MenuAttrs::decode(&p.encode()).unwrap(), p);
    }

    #[test]
    fn validation_and_corruption() {
        assert!(MenuAttrs::default().validate().is_err());
        assert!(MenuAttrs::uim(Qualified::qsys("X"), "").validate().is_ok());
        let dspf = MenuAttrs {
            kind: MenuKind::Dspf,
            dspf: Some(Qualified::qsys("X")),
            ..MenuAttrs::default()
        };
        assert!(dspf.validate().is_err());
        let mut b = MenuAttrs::default().encode();
        b[0] = b'X';
        assert!(MenuAttrs::decode(&b).is_err());
        assert!(MenuAttrs::decode(&b[..20]).is_err());
    }

    #[test]
    fn keywords() {
        assert_eq!(MenuKind::parse("*uim"), Some(MenuKind::Uim));
        assert_eq!(CmdLin::parse("*SHORT").map(CmdLin::text), Some("*SHORT"));
        assert_eq!(CurLib::parse("mylib"), CurLib::Name("MYLIB".into()));
        assert_eq!(CurLib::parse("*MNULIB").text(), "*MNULIB");
        assert_eq!(PrdLib::parse("*none"), PrdLib::None);
        assert_eq!(PrdLib::parse("").text(), "*NOCHG");
    }
}
