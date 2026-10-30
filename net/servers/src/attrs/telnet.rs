// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGTELNA`: the telnet server's attributes, with the oracle's
//! keywords, ranges and defaults (V7R5M0 `QCDRCMDD` and prompt). `PORT`
//! is ours, kept in the same bag and set by no command: the test
//! harness moves the server off 23 with it.

use super::{get, is_same};

/// A count IBM lets the system choose (`*CALC`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Count {
    /// `*CALC`.
    Calc,
    /// A number.
    Of(u32),
}

impl Count {
    fn text(self) -> String {
        match self {
            Count::Calc => "*CALC".into(),
            Count::Of(n) => n.to_string(),
        }
    }
}

/// `NBRFLDS`: 3270 input fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fields {
    /// `*DFT`.
    Dft,
    /// `*RPY`: as the client's query reply says.
    Rpy,
    /// A number, 126 to 9999.
    Of(u32),
}

/// The CCSID special values and the numbers the command maps them to.
pub const CCSID_NAMES: &[(&str, i64)] = &[
    ("*MULTINAT", 84_542_540),
    ("*BRITISH", 84_608_077),
    ("*DUTCH", 84_673_614),
    ("*FINNISH", 84_739_151),
    ("*FRENCH", 18_941_008),
    ("*FRENCHCAN", 78_119_932),
    ("*GERMAN", 17_368_051),
    ("*ITALIAN", 19_203_060),
    ("*NORDAN", 84_870_225),
    ("*SPANISH", 78_316_543),
    ("*SWEDISH", 84_935_762),
    ("*SWISS", 78_185_469),
];

/// The default timemark timeout, seconds.
pub const TIMMRKTIMO_DFT: u32 = 300;
/// The default telnet port.
pub const PORT: u16 = 23;

/// The telnet server's attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelnetAttrs {
    /// `NBRSVR`.
    pub nbrsvr: Count,
    /// `NBRTASK`.
    pub nbrtask: Count,
    /// `TIMMRKTIMO`, seconds.
    pub timmrktimo: u32,
    /// `ENBWIDE`.
    pub enbwide: bool,
    /// `NBRFLDS`.
    pub nbrflds: Fields,
    /// `DFTNVTTYPE`: `*VT100` or `*NVT`.
    pub dftnvttype: String,
    /// `CCSID`: a special value or a number.
    pub ccsid: String,
    /// `TBLVTOUT`: `*CCSID`, `*DFT` or `LIB/TABLE`.
    pub tblvtout: String,
    /// `TBLVTIN`.
    pub tblvtin: String,
    /// `ALWSSL`: `*YES`, `*NO` or `*ONLY`.
    pub alwssl: String,
    /// The plain port (ours).
    pub port: u16,
}

impl Default for TelnetAttrs {
    fn default() -> Self {
        Self {
            nbrsvr: Count::Calc,
            nbrtask: Count::Calc,
            timmrktimo: TIMMRKTIMO_DFT,
            enbwide: true,
            nbrflds: Fields::Dft,
            dftnvttype: "*VT100".into(),
            ccsid: "*MULTINAT".into(),
            tblvtout: "*CCSID".into(),
            tblvtin: "*CCSID".into(),
            alwssl: "*YES".into(),
            port: PORT,
        }
    }
}

/// The keywords `CHGTELNA` changes, in its order (`AUTOSTART` is the
/// server table's own flag).
pub const KEYWORDS: &[&str] = &[
    "NBRSVR",
    "NBRTASK",
    "TIMMRKTIMO",
    "ENBWIDE",
    "NBRFLDS",
    "DFTNVTTYPE",
    "CCSID",
    "TBLVTOUT",
    "TBLVTIN",
    "ALWSSL",
];

fn number(v: &str, kwd: &str, lo: u32, hi: u32) -> Result<u32, String> {
    v.trim()
        .parse::<u32>()
        .ok()
        .filter(|n| (lo..=hi).contains(n))
        .ok_or_else(|| format!("{kwd}({v}): from {lo} to {hi}"))
}

fn count(v: &str, kwd: &str) -> Result<Count, String> {
    match v.trim().to_ascii_uppercase().as_str() {
        "*CALC" | "-2" => Ok(Count::Calc),
        n => number(n, kwd, 1, 200).map(Count::Of),
    }
}

fn one_of(v: &str, kwd: &str, allowed: &[&str]) -> Result<String, String> {
    let u = v.trim().to_ascii_uppercase();
    if allowed.contains(&u.as_str()) {
        Ok(u)
    } else {
        Err(format!("{kwd}({v}): one of {}", allowed.join(" ")))
    }
}

fn ccsid(v: &str) -> Result<String, String> {
    let u = v.trim().to_ascii_uppercase();
    if let Some((name, _)) = CCSID_NAMES.iter().find(|(n, m)| *n == u || m.to_string() == u) {
        return Ok((*name).to_string());
    }
    number(&u, "CCSID", 1, 65533).map(|n| n.to_string())
}

fn table(v: &str, kwd: &str) -> Result<String, String> {
    let u = v.trim().to_ascii_uppercase();
    if matches!(u.as_str(), "*CCSID" | "*DFT") {
        return Ok(u);
    }
    // LIB/TABLE or TABLE; a table name is a system name.
    let name = u.rsplit('/').next().unwrap_or("");
    let ok = |s: &str| !s.is_empty() && s.len() <= 10 && !s.starts_with(|c: char| c.is_ascii_digit() || c == '*');
    if ok(name) && u.split('/').count() <= 2 {
        Ok(u)
    } else {
        Err(format!("{kwd}({v}): *CCSID, *DFT or a table"))
    }
}

impl TelnetAttrs {
    /// From the server table's bag; missing or unreadable values take
    /// their defaults.
    pub fn from_bag(bag: &[(String, String)]) -> Self {
        let mut a = Self::default();
        for k in KEYWORDS {
            if let Some(v) = get(bag, k) {
                let _ = a.set(k, v);
            }
        }
        if let Some(p) = get(bag, "PORT").and_then(|p| p.trim().parse().ok()) {
            a.port = p;
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
        out
    }

    /// One attribute as text (as the prompt shows it).
    pub fn value(&self, kwd: &str) -> String {
        match kwd.to_ascii_uppercase().as_str() {
            "NBRSVR" => self.nbrsvr.text(),
            "NBRTASK" => self.nbrtask.text(),
            "TIMMRKTIMO" => self.timmrktimo.to_string(),
            "ENBWIDE" => if self.enbwide { "*YES" } else { "*NO" }.into(),
            "NBRFLDS" => match self.nbrflds {
                Fields::Dft => "*DFT".into(),
                Fields::Rpy => "*RPY".into(),
                Fields::Of(n) => n.to_string(),
            },
            "DFTNVTTYPE" => self.dftnvttype.clone(),
            "CCSID" => self.ccsid.clone(),
            "TBLVTOUT" => self.tblvtout.clone(),
            "TBLVTIN" => self.tblvtin.clone(),
            "ALWSSL" => self.alwssl.clone(),
            "PORT" => self.port.to_string(),
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
        match kwd.to_ascii_uppercase().as_str() {
            "NBRSVR" => self.nbrsvr = count(v, "NBRSVR")?,
            "NBRTASK" => self.nbrtask = count(v, "NBRTASK")?,
            "TIMMRKTIMO" => {
                self.timmrktimo = match v.trim().to_ascii_uppercase().as_str() {
                    "*CALC" | "*DFT" | "-2" => TIMMRKTIMO_DFT,
                    n => number(n, "TIMMRKTIMO", 1, 2_419_200)?,
                }
            }
            "ENBWIDE" => self.enbwide = one_of(v, "ENBWIDE", &["*YES", "*NO"])? == "*YES",
            "NBRFLDS" => {
                self.nbrflds = match v.trim().to_ascii_uppercase().as_str() {
                    "*DFT" | "-2" => Fields::Dft,
                    "*RPY" | "-3" => Fields::Rpy,
                    n => Fields::Of(number(n, "NBRFLDS", 126, 9999)?),
                }
            }
            "DFTNVTTYPE" => self.dftnvttype = one_of(v, "DFTNVTTYPE", &["*VT100", "*NVT"])?,
            "CCSID" => self.ccsid = ccsid(v)?,
            "TBLVTOUT" => self.tblvtout = table(v, "TBLVTOUT")?,
            "TBLVTIN" => self.tblvtin = table(v, "TBLVTIN")?,
            "ALWSSL" => self.alwssl = one_of(v, "ALWSSL", &["*YES", "*NO", "*ONLY"])?,
            "PORT" => {
                self.port = u16::try_from(number(v, "PORT", 1, 65535)?).unwrap_or(PORT);
            }
            other => return Err(format!("{other} is not a TELNET attribute")),
        }
        Ok(*self != before)
    }

    /// Whether the plain port listens.
    pub fn plain(&self) -> bool {
        self.alwssl != "*ONLY"
    }

    /// Whether the TLS port listens.
    pub fn secure(&self) -> bool {
        self.alwssl != "*NO"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_oracles() {
        let a = TelnetAttrs::from_bag(&[]);
        let shown: Vec<String> = KEYWORDS.iter().map(|k| a.value(k)).collect();
        assert_eq!(
            shown,
            ["*CALC", "*CALC", "300", "*YES", "*DFT", "*VT100", "*MULTINAT", "*CCSID", "*CCSID", "*YES"]
        );
        assert!(a.plain() && a.secure());
    }

    #[test]
    fn set_and_round_trip() {
        let mut a = TelnetAttrs::default();
        assert_eq!(a.set("NBRSVR", "4"), Ok(true));
        assert_eq!(a.set("NBRSVR", "*SAME"), Ok(false));
        assert_eq!(a.set("NBRSVR", "-1"), Ok(false));
        assert_eq!(a.set("NBRTASK", "-2"), Ok(false), "*CALC already");
        assert_eq!(a.set("TIMMRKTIMO", "600"), Ok(true));
        assert_eq!(a.set("NBRFLDS", "-3"), Ok(true));
        assert_eq!(a.set("CCSID", "17368051"), Ok(true));
        assert_eq!(a.ccsid, "*GERMAN");
        assert_eq!(a.set("TBLVTOUT", "qgpl/mytbl"), Ok(true));
        assert_eq!(a.set("ALWSSL", "*only"), Ok(true));
        assert!(!a.plain() && a.secure());
        assert_eq!(a.set("PORT", "2323"), Ok(true));
        assert!(a.set("NBRSVR", "201").is_err());
        assert!(a.set("NBRFLDS", "100").is_err());
        assert!(a.set("ALWSSL", "*MAYBE").is_err());
        assert!(a.set("TBLVTIN", "A/B/C").is_err());
        assert!(a.set("TBLVTIN", "*LIBL/*SAME").is_err());
        assert_eq!(a.set("TBLVTIN", "*LIBL/MYTBL"), Ok(true));
        assert!(a.set("SSLCFG", "x").is_err());
        let back = TelnetAttrs::from_bag(&a.to_bag());
        assert_eq!(back, a);
        assert_eq!(get(&a.to_bag(), "PORT"), Some("2323"));
        assert_eq!(get(&TelnetAttrs::default().to_bag(), "PORT"), None);
    }
}
