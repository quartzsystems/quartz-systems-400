// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGFTPA`: the FTP server's attributes, with the oracle's keywords,
//! ranges and defaults (V7R5M0 `QCDRCMDD`). `PORT`, `TLSPORT` and
//! `DTACNNPORT` are ours, kept in the same bag and set by no command:
//! the harness moves the server off 21, and `DTACNNPORT` bounds the
//! passive data ports (`lo-hi`, empty for any).

use super::{get, is_same};

/// The default number of servers (`*DFT`).
pub const NBRSVR_DFT: u32 = 3;
/// The default inactivity timeout, seconds (`*DFT`).
pub const INACTTIMO_DFT: u32 = 300;
/// The default ASCII CCSID (`*DFT`).
pub const CCSID_DFT: u16 = 819;
/// The plain port.
pub const PORT: u16 = 21;
/// The implicit TLS port.
pub const TLS_PORT: u16 = 990;

/// `CRTCCSID`: the CCSID of a database file `STOR` creates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrtCcsid {
    /// `*CALC`: the EBCDIC CCSID that goes with the data's ASCII CCSID.
    Calc,
    /// `*USER`: the user profile's.
    User,
    /// `*SYSVAL`: `QCCSID`.
    Sysval,
    /// A CCSID.
    Of(u16),
}

impl CrtCcsid {
    /// As the prompt and `SITE CRTCCSID` show it.
    pub fn text(self) -> String {
        match self {
            CrtCcsid::Calc => "*CALC".into(),
            CrtCcsid::User => "*USER".into(),
            CrtCcsid::Sysval => "*SYSVAL".into(),
            CrtCcsid::Of(n) => n.to_string(),
        }
    }

    /// From a command or `SITE` value.
    pub fn parse(v: &str) -> Result<Self, String> {
        match v.trim().to_ascii_uppercase().as_str() {
            "*CALC" | "0" => Ok(CrtCcsid::Calc),
            "*USER" | "-3" => Ok(CrtCcsid::User),
            "*SYSVAL" | "-2" => Ok(CrtCcsid::Sysval),
            n => number(n, "CRTCCSID", 1, 65533).map(|n| CrtCcsid::Of(u16::try_from(n).unwrap_or(37))),
        }
    }
}

/// The FTP server's attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FtpAttrs {
    /// `NBRSVR`: prestarted servers (kept; a session job starts per
    /// connection).
    pub nbrsvr: u32,
    /// `INACTTIMO`, seconds; 0 is none.
    pub inacttimo: u32,
    /// `CCSID`: the ASCII CCSID of `TYPE A`.
    pub ccsid: u16,
    /// `TBLFTPOUT`: `*CCSID`, `*DFT` or `LIB/TABLE`.
    pub tblftpout: String,
    /// `TBLFTPIN`.
    pub tblftpin: String,
    /// `NAMEFMT`: `*LIB` (0) or `*PATH` (1).
    pub namefmt: String,
    /// `CURDIR`: `*CURLIB` or `*HOMEDIR`.
    pub curdir: String,
    /// `LISTFMT`: `*DFT` (0) or `*UNIX` (1).
    pub listfmt: String,
    /// `CRTCCSID`.
    pub crtccsid: CrtCcsid,
    /// `SBSD`: `*DFT` or `LIB/NAME`.
    pub sbsd: String,
    /// `ALWSSL`: `*YES`, `*NO` or `*ONLY`.
    pub alwssl: String,
    /// The plain port (ours).
    pub port: u16,
    /// The implicit TLS port (ours).
    pub tls_port: u16,
    /// Passive data ports, inclusive (ours); `None` is any.
    pub dtacnnport: Option<(u16, u16)>,
}

impl Default for FtpAttrs {
    fn default() -> Self {
        Self {
            nbrsvr: NBRSVR_DFT,
            inacttimo: INACTTIMO_DFT,
            ccsid: CCSID_DFT,
            tblftpout: "*CCSID".into(),
            tblftpin: "*CCSID".into(),
            namefmt: "*LIB".into(),
            curdir: "*CURLIB".into(),
            listfmt: "*DFT".into(),
            crtccsid: CrtCcsid::Calc,
            sbsd: "*DFT".into(),
            alwssl: "*YES".into(),
            port: PORT,
            tls_port: TLS_PORT,
            dtacnnport: None,
        }
    }
}

/// The keywords `CHGFTPA` changes, in its order (`AUTOSTART` is the
/// server table's own flag).
pub const KEYWORDS: &[&str] = &[
    "NBRSVR",
    "INACTTIMO",
    "CCSID",
    "TBLFTPOUT",
    "TBLFTPIN",
    "NAMEFMT",
    "CURDIR",
    "LISTFMT",
    "CRTCCSID",
    "SBSD",
    "ALWSSL",
];

fn number(v: &str, kwd: &str, lo: u32, hi: u32) -> Result<u32, String> {
    v.trim()
        .parse::<u32>()
        .ok()
        .filter(|n| (lo..=hi).contains(n))
        .ok_or_else(|| format!("{kwd}({v}): from {lo} to {hi}"))
}

fn one_of(v: &str, kwd: &str, allowed: &[&str]) -> Result<String, String> {
    let u = v.trim().to_ascii_uppercase();
    if allowed.contains(&u.as_str()) {
        Ok(u)
    } else {
        Err(format!("{kwd}({v}): one of {}", allowed.join(" ")))
    }
}

fn qualified(v: &str, kwd: &str, specials: &[&str]) -> Result<String, String> {
    let u = v.trim().to_ascii_uppercase();
    if specials.contains(&u.as_str()) {
        return Ok(u);
    }
    let ok = |s: &str| !s.is_empty() && s.len() <= 10 && !s.starts_with(|c: char| c.is_ascii_digit());
    let parts: Vec<&str> = u.split('/').collect();
    let name_ok = parts.last().is_some_and(|n| ok(n) && !n.starts_with('*'));
    if parts.len() <= 2 && name_ok && parts.iter().all(|p| ok(p)) {
        Ok(u)
    } else {
        Err(format!("{kwd}({v}): {} or a qualified name", specials.join(", ")))
    }
}

fn ports(v: &str) -> Result<Option<(u16, u16)>, String> {
    let v = v.trim();
    if v.is_empty() || v.eq_ignore_ascii_case("*ANY") {
        return Ok(None);
    }
    let (a, b) = v.split_once('-').unwrap_or((v, v));
    let lo = number(a, "DTACNNPORT", 1024, 65535)?;
    let hi = number(b, "DTACNNPORT", 1024, 65535)?;
    if lo > hi {
        return Err(format!("DTACNNPORT({v}): the low port first"));
    }
    Ok(Some((u16::try_from(lo).unwrap_or(1024), u16::try_from(hi).unwrap_or(65535))))
}

impl FtpAttrs {
    /// From the server table's bag; missing or unreadable values take
    /// their defaults.
    pub fn from_bag(bag: &[(String, String)]) -> Self {
        let mut a = Self::default();
        for k in KEYWORDS.iter().chain(&["PORT", "TLSPORT", "DTACNNPORT"]) {
            if let Some(v) = get(bag, k) {
                let _ = a.set(k, v);
            }
        }
        a
    }

    /// Every attribute as the bag keeps it.
    pub fn to_bag(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = KEYWORDS
            .iter()
            .map(|k| ((*k).to_string(), self.value(k)))
            .collect();
        if self.port != PORT {
            out.push(("PORT".into(), self.port.to_string()));
        }
        if self.tls_port != TLS_PORT {
            out.push(("TLSPORT".into(), self.tls_port.to_string()));
        }
        if self.dtacnnport.is_some() {
            out.push(("DTACNNPORT".into(), self.value("DTACNNPORT")));
        }
        out
    }

    /// One attribute as text (as the prompt shows it).
    pub fn value(&self, kwd: &str) -> String {
        match kwd.to_ascii_uppercase().as_str() {
            "NBRSVR" => self.nbrsvr.to_string(),
            "INACTTIMO" => self.inacttimo.to_string(),
            "CCSID" => format!("{:05}", self.ccsid),
            "TBLFTPOUT" => self.tblftpout.clone(),
            "TBLFTPIN" => self.tblftpin.clone(),
            "NAMEFMT" => self.namefmt.clone(),
            "CURDIR" => self.curdir.clone(),
            "LISTFMT" => self.listfmt.clone(),
            "CRTCCSID" => self.crtccsid.text(),
            "SBSD" => self.sbsd.clone(),
            "ALWSSL" => self.alwssl.clone(),
            "PORT" => self.port.to_string(),
            "TLSPORT" => self.tls_port.to_string(),
            "DTACNNPORT" => self
                .dtacnnport
                .map_or_else(String::new, |(a, b)| format!("{a}-{b}")),
            _ => String::new(),
        }
    }

    /// Set one attribute from a command value (`*SAME` leaves it);
    /// whether it changed, or why the value is not valid.
    pub fn set(&mut self, kwd: &str, v: &str) -> Result<bool, String> {
        if is_same(v) {
            return Ok(false);
        }
        let before = self.clone();
        let u = v.trim().to_ascii_uppercase();
        match kwd.to_ascii_uppercase().as_str() {
            "NBRSVR" => {
                self.nbrsvr = if u == "*DFT" { NBRSVR_DFT } else { number(&u, "NBRSVR", 1, 20)? };
            }
            "INACTTIMO" => {
                self.inacttimo = if u == "*DFT" {
                    INACTTIMO_DFT
                } else {
                    number(&u, "INACTTIMO", 0, 2_147_483)?
                };
            }
            "CCSID" => {
                self.ccsid = if u == "*DFT" {
                    CCSID_DFT
                } else {
                    u16::try_from(number(&u, "CCSID", 1, 65533)?).unwrap_or(CCSID_DFT)
                };
            }
            "TBLFTPOUT" => self.tblftpout = qualified(&u, "TBLFTPOUT", &["*CCSID", "*DFT"])?,
            "TBLFTPIN" => self.tblftpin = qualified(&u, "TBLFTPIN", &["*CCSID", "*DFT"])?,
            "NAMEFMT" => self.namefmt = one_of(&u, "NAMEFMT", &["*LIB", "*PATH"])?,
            "CURDIR" => self.curdir = one_of(&u, "CURDIR", &["*CURLIB", "*HOMEDIR"])?,
            "LISTFMT" => self.listfmt = one_of(&u, "LISTFMT", &["*DFT", "*UNIX"])?,
            "CRTCCSID" => self.crtccsid = CrtCcsid::parse(&u)?,
            "SBSD" => self.sbsd = qualified(&u, "SBSD", &["*DFT"])?,
            "ALWSSL" => self.alwssl = one_of(&u, "ALWSSL", &["*YES", "*NO", "*ONLY"])?,
            "PORT" => self.port = u16::try_from(number(&u, "PORT", 1, 65535)?).unwrap_or(PORT),
            "TLSPORT" => self.tls_port = u16::try_from(number(&u, "TLSPORT", 1, 65535)?).unwrap_or(TLS_PORT),
            "DTACNNPORT" => self.dtacnnport = ports(&u)?,
            other => return Err(format!("{other} is not an FTP attribute")),
        }
        Ok(*self != before)
    }

    /// `NAMEFMT` as the session's number.
    pub fn namefmt_number(&self) -> u8 {
        u8::from(self.namefmt == "*PATH")
    }

    /// `LISTFMT` as the session's number.
    pub fn listfmt_number(&self) -> u8 {
        u8::from(self.listfmt == "*UNIX")
    }

    /// Whether a session starts in the home directory.
    pub fn starts_home(&self) -> bool {
        self.curdir == "*HOMEDIR"
    }

    /// Whether the plain port listens.
    pub fn plain(&self) -> bool {
        self.alwssl != "*ONLY"
    }

    /// Whether the implicit TLS port listens and `AUTH TLS` works.
    pub fn secure(&self) -> bool {
        self.alwssl != "*NO"
    }

    /// The subsystem the jobs run in.
    pub fn subsystem(&self) -> String {
        if self.sbsd == "*DFT" {
            "QSYSWRK".into()
        } else {
            self.sbsd.rsplit('/').next().unwrap_or("QSYSWRK").to_string()
        }
    }

    /// The request data the listener job reads.
    pub fn rqsdta(&self) -> String {
        format!("PORT({}) TLSPORT({})", self.port, self.tls_port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_oracles() {
        let a = FtpAttrs::from_bag(&[]);
        let shown: Vec<String> = KEYWORDS.iter().map(|k| a.value(k)).collect();
        assert_eq!(
            shown,
            [
                "3", "300", "00819", "*CCSID", "*CCSID", "*LIB", "*CURLIB", "*DFT", "*CALC", "*DFT", "*YES"
            ]
        );
        assert!(a.plain() && a.secure());
        assert_eq!(a.namefmt_number(), 0);
        assert_eq!(a.subsystem(), "QSYSWRK");
    }

    #[test]
    fn set_and_round_trip() {
        let mut a = FtpAttrs::default();
        assert_eq!(a.set("NBRSVR", "5"), Ok(true));
        assert_eq!(a.set("NBRSVR", "*SAME"), Ok(false));
        assert_eq!(a.set("INACTTIMO", "0"), Ok(true));
        assert_eq!(a.set("CCSID", "1208"), Ok(true));
        assert_eq!(a.set("NAMEFMT", "*path"), Ok(true));
        assert_eq!(a.set("CURDIR", "*HOMEDIR"), Ok(true));
        assert_eq!(a.set("LISTFMT", "*UNIX"), Ok(true));
        assert_eq!(a.set("CRTCCSID", "-3"), Ok(true));
        assert_eq!(a.crtccsid, CrtCcsid::User);
        assert_eq!(a.set("SBSD", "QSYS/QSYSWRK2"), Ok(true));
        assert_eq!(a.subsystem(), "QSYSWRK2");
        assert_eq!(a.set("TBLFTPOUT", "QGPL/MYTBL"), Ok(true));
        assert_eq!(a.set("ALWSSL", "*ONLY"), Ok(true));
        assert_eq!(a.set("DTACNNPORT", "40000-40100"), Ok(true));
        assert_eq!(a.set("PORT", "2121"), Ok(true));
        assert!(a.set("NBRSVR", "21").is_err());
        assert!(a.set("NAMEFMT", "*BOTH").is_err());
        assert!(a.set("DTACNNPORT", "5-4").is_err());
        assert!(a.set("SSLCFG", "x").is_err());
        let back = FtpAttrs::from_bag(&a.to_bag());
        assert_eq!(back, a);
        assert!(!a.plain());
        assert_eq!(a.namefmt_number(), 1);
        assert!(a.starts_home());
    }
}
