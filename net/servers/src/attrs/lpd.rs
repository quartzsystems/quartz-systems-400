// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGLPDA`: the LPD server's attributes, the oracle's keywords
//! (V7R5M0 `QCDRCMDD`): `AUTOSTART` (the server table's flag) and
//! `NBRSVR`, 1 to 200, `*DFT` 2. `PORT` is ours, kept in the same bag
//! and set by no command.

use super::{get, is_same};

/// `NBRSVR(*DFT)`.
pub const NBRSVR_DFT: u16 = 2;
/// The LPD port.
pub const PORT: u16 = 515;

/// The LPD attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LpdAttrs {
    /// `NBRSVR`: initial server jobs. Kept: connections are served on
    /// threads of the one `QTLPD00001` job.
    pub nbrsvr: u16,
    /// The port.
    pub port: u16,
}

impl Default for LpdAttrs {
    fn default() -> Self {
        Self {
            nbrsvr: NBRSVR_DFT,
            port: PORT,
        }
    }
}

/// The keywords `CHGLPDA` changes besides `AUTOSTART`.
pub const KEYWORDS: &[&str] = &["NBRSVR"];

impl LpdAttrs {
    /// From the server table's bag.
    pub fn from_bag(bag: &[(String, String)]) -> Self {
        let mut a = Self::default();
        if let Some(v) = get(bag, "NBRSVR") {
            let _ = a.set("NBRSVR", v);
        }
        if let Some(p) = get(bag, "PORT").and_then(|p| p.trim().parse().ok()) {
            a.port = p;
        }
        a
    }

    /// As the bag keeps it.
    pub fn to_bag(&self) -> Vec<(String, String)> {
        let mut out = vec![("NBRSVR".to_string(), self.nbrsvr.to_string())];
        if self.port != PORT {
            out.push(("PORT".into(), self.port.to_string()));
        }
        out
    }

    /// Set one attribute; whether it changed, or why not.
    pub fn set(&mut self, kwd: &str, v: &str) -> Result<bool, String> {
        if is_same(v) {
            return Ok(false);
        }
        let before = *self;
        let up = v.trim().to_ascii_uppercase();
        match kwd.to_ascii_uppercase().as_str() {
            "NBRSVR" => {
                self.nbrsvr = match up.as_str() {
                    "*DFT" => NBRSVR_DFT,
                    n => n
                        .parse::<u16>()
                        .ok()
                        .filter(|n| (1..=200).contains(n))
                        .ok_or_else(|| format!("NBRSVR({v}): from 1 to 200"))?,
                }
            }
            "PORT" => {
                self.port = up
                    .parse::<u16>()
                    .ok()
                    .filter(|p| *p != 0)
                    .ok_or_else(|| format!("PORT({v}): from 1 to 65535"))?;
            }
            other => return Err(format!("{other} is not an LPD attribute")),
        }
        Ok(*self != before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lpd_attributes() {
        let mut a = LpdAttrs::from_bag(&[]);
        assert_eq!(a, LpdAttrs::default());
        assert_eq!(a.set("NBRSVR", "5"), Ok(true));
        assert_eq!(a.set("NBRSVR", "*DFT"), Ok(true));
        assert_eq!(a.set("NBRSVR", "-1"), Ok(false));
        assert!(a.set("NBRSVR", "201").is_err());
        a.set("PORT", "1515").unwrap();
        assert_eq!(LpdAttrs::from_bag(&a.to_bag()), a);
    }
}
