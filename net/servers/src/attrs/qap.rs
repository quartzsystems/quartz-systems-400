// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGQAPA`: the QAP server's attributes (docs/QAP.md): `AUTOSTART`
//! (the server table's flag), `PORT` and `IDLETIMO`, the seconds a
//! session lives with no connection attached.

use super::{get, is_same};

/// `PORT(*DFT)`.
pub const PORT_DFT: u16 = 2320;
/// `IDLETIMO(*DFT)`: an hour.
pub const IDLETIMO_DFT: u32 = 3600;
/// The longest `IDLETIMO`: a day.
pub const IDLETIMO_MAX: u32 = 86_400;

/// The keywords `CHGQAPA` changes besides `AUTOSTART`.
pub const KEYWORDS: &[&str] = &["PORT", "IDLETIMO"];

/// The QAP attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QapAttrs {
    /// The port.
    pub port: u16,
    /// Seconds a detached session may be resumed.
    pub idletimo: u32,
}

impl Default for QapAttrs {
    fn default() -> Self {
        Self {
            port: PORT_DFT,
            idletimo: IDLETIMO_DFT,
        }
    }
}

impl QapAttrs {
    /// From the server table's bag.
    pub fn from_bag(bag: &[(String, String)]) -> Self {
        let mut a = Self::default();
        for kwd in KEYWORDS {
            if let Some(v) = get(bag, kwd) {
                let _ = a.set(kwd, v);
            }
        }
        a
    }

    /// As the bag keeps it.
    pub fn to_bag(&self) -> Vec<(String, String)> {
        vec![
            ("PORT".to_string(), self.port.to_string()),
            ("IDLETIMO".to_string(), self.idletimo.to_string()),
        ]
    }

    /// Set one attribute; whether it changed, or why not.
    pub fn set(&mut self, kwd: &str, v: &str) -> Result<bool, String> {
        if is_same(v) {
            return Ok(false);
        }
        let before = *self;
        let up = v.trim().to_ascii_uppercase();
        match kwd.to_ascii_uppercase().as_str() {
            "PORT" => {
                self.port = match up.as_str() {
                    "*DFT" => PORT_DFT,
                    n => n
                        .parse::<u16>()
                        .ok()
                        .filter(|p| *p != 0)
                        .ok_or_else(|| format!("PORT({v}): from 1 to 65535"))?,
                }
            }
            "IDLETIMO" => {
                self.idletimo = match up.as_str() {
                    "*DFT" => IDLETIMO_DFT,
                    n => n
                        .parse::<u32>()
                        .ok()
                        .filter(|n| (60..=IDLETIMO_MAX).contains(n))
                        .ok_or_else(|| format!("IDLETIMO({v}): from 60 to {IDLETIMO_MAX}"))?,
                }
            }
            other => return Err(format!("{other} is not a QAP attribute")),
        }
        Ok(*self != before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qap_attributes() {
        let mut a = QapAttrs::from_bag(&[]);
        assert_eq!(a, QapAttrs::default());
        assert_eq!(a.set("PORT", "2400"), Ok(true));
        assert_eq!(a.set("PORT", "*SAME"), Ok(false));
        assert!(a.set("PORT", "0").is_err());
        assert_eq!(a.set("IDLETIMO", "600"), Ok(true));
        assert!(a.set("IDLETIMO", "59").is_err());
        assert!(a.set("IDLETIMO", "86401").is_err());
        assert_eq!(QapAttrs::from_bag(&a.to_bag()), a);
        a.set("PORT", "*DFT").unwrap();
        a.set("IDLETIMO", "*dft").unwrap();
        assert_eq!(a, QapAttrs::default());
    }
}
