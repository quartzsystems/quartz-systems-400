// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGQSIA`: the QSI server's attributes (docs/QSID.md section 12):
//! `AUTOSTART` (the server table's flag, on as shipped) and `PORT`.

use super::{get, is_same};

/// `PORT(*DFT)`: QSI's port, as a QS/VS host's.
pub const PORT_DFT: u16 = 2300;

/// The keywords `CHGQSIA` changes besides `AUTOSTART`.
pub const KEYWORDS: &[&str] = &["PORT"];

/// The QSI server's attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QsiAttrs {
    /// The port.
    pub port: u16,
}

impl Default for QsiAttrs {
    fn default() -> Self {
        Self { port: PORT_DFT }
    }
}

impl QsiAttrs {
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
        vec![("PORT".to_string(), self.port.to_string())]
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
            other => return Err(format!("{other} is not a QSI attribute")),
        }
        Ok(*self != before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qsi_attributes() {
        let mut a = QsiAttrs::from_bag(&[]);
        assert_eq!(a.port, 2300);
        assert_eq!(a.set("PORT", "2400"), Ok(true));
        assert_eq!(a.set("PORT", "*SAME"), Ok(false));
        assert!(a.set("PORT", "0").is_err());
        assert!(a.set("NBRSVR", "1").is_err());
        assert_eq!(QsiAttrs::from_bag(&a.to_bag()), a);
        a.set("PORT", "*dft").unwrap();
        assert_eq!(a, QsiAttrs::default());
    }
}
