// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGNTPA`: the SNTP client's and server's attributes, with the
//! oracle's keywords and ranges (V7R5M0 `QCDRCMDD`). The oracle could
//! not show its current values, so the defaults are IBM's documented
//! ones. `AUTOSTART` (the client's) and `SVRAUTOSTR` (the server's) are
//! kept in the bag; the server table's own autostart flag is set when
//! either is on, so `STRTCP` starts `QTOTNTP`, which runs the parts
//! that are.

use super::{get, is_same, keyword};

/// The most remote systems.
pub const MAX_RMTSYS: usize = 3;

/// The SNTP attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NtpAttrs {
    /// `RMTSYS`: time servers, tried in order; empty for `*NONE`.
    pub rmtsys: Vec<String>,
    /// `AUTOSTART`: the client starts with TCP/IP.
    pub autostart: bool,
    /// `POLLITV`, minutes.
    pub pollitv: u32,
    /// `MINADJ`, milliseconds; `None` for `*NONE`.
    pub minadj: Option<u32>,
    /// `MAXADJ`, minutes.
    pub maxadj: u32,
    /// `ADJTHLD`, seconds; `None` for `*MAXADJ`.
    pub adjthld: Option<u32>,
    /// `ACTLOG`: `*NONE`, `*CHANGE` or `*POLL`.
    pub actlog: String,
    /// `SVRAUTOSTR`: the server starts with TCP/IP.
    pub svrautostr: bool,
    /// `SVRACTLOG`: `*NONE`, `*ALL` or `*ERROR`.
    pub svractlog: String,
    /// `SYNCRQD`: the server answers only once the client has
    /// synchronized.
    pub syncrqd: bool,
    /// The UDP port (ours; no command sets it).
    pub port: u16,
}

impl Default for NtpAttrs {
    fn default() -> Self {
        Self {
            rmtsys: Vec::new(),
            autostart: false,
            pollitv: 60,
            minadj: Some(20),
            maxadj: 20,
            adjthld: None,
            actlog: "*NONE".into(),
            svrautostr: false,
            svractlog: "*NONE".into(),
            syncrqd: true,
            port: 123,
        }
    }
}

/// The keywords `CHGNTPA` changes, in its order.
pub const KEYWORDS: &[&str] = &[
    "RMTSYS",
    "AUTOSTART",
    "POLLITV",
    "MINADJ",
    "MAXADJ",
    "ADJTHLD",
    "ACTLOG",
    "SVRAUTOSTR",
    "SVRACTLOG",
    "SYNCRQD",
];

fn number(v: &str, kwd: &str, lo: u32, hi: u32) -> Result<u32, String> {
    v.trim()
        .parse::<u32>()
        .ok()
        .filter(|n| (lo..=hi).contains(n))
        .ok_or_else(|| format!("{kwd}({v}): from {lo} to {hi}"))
}

fn yes_no(v: &str, kwd: &str) -> Result<bool, String> {
    match v.trim().to_ascii_uppercase().as_str() {
        "*YES" => Ok(true),
        "*NO" => Ok(false),
        _ => Err(format!("{kwd}({v}): *YES or *NO")),
    }
}

fn yn(b: bool) -> String {
    if b { "*YES" } else { "*NO" }.into()
}

/// A host name or address as `RMTSYS` takes it.
fn host(v: &str) -> Result<String, String> {
    let h = v.trim().trim_matches('\'');
    let ok = !h.is_empty()
        && h.len() <= 255
        && h.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'));
    if ok {
        Ok(h.to_string())
    } else {
        Err(format!("RMTSYS({v}): a host name or an address"))
    }
}

impl NtpAttrs {
    /// From the server table's bag; unreadable values take defaults.
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
        let mut out: Vec<(String, String)> = KEYWORDS.iter().map(|k| ((*k).to_string(), self.value(k))).collect();
        if self.port != 123 {
            out.push(("PORT".into(), self.port.to_string()));
        }
        out
    }

    /// Whether `QTOTNTP` starts with TCP/IP.
    pub fn autostarts(&self) -> bool {
        self.autostart || self.svrautostr
    }

    /// One attribute as text.
    pub fn value(&self, kwd: &str) -> String {
        match kwd.to_ascii_uppercase().as_str() {
            "RMTSYS" if self.rmtsys.is_empty() => "*NONE".into(),
            "RMTSYS" => self.rmtsys.join(" "),
            "AUTOSTART" => yn(self.autostart),
            "POLLITV" => self.pollitv.to_string(),
            "MINADJ" => self.minadj.map_or_else(|| "*NONE".into(), |m| m.to_string()),
            "MAXADJ" => self.maxadj.to_string(),
            "ADJTHLD" => self.adjthld.map_or_else(|| "*MAXADJ".into(), |s| s.to_string()),
            "ACTLOG" => self.actlog.clone(),
            "SVRAUTOSTR" => yn(self.svrautostr),
            "SVRACTLOG" => self.svractlog.clone(),
            "SYNCRQD" => yn(self.syncrqd),
            "PORT" => self.port.to_string(),
            _ => String::new(),
        }
    }

    /// Set one attribute from a command value (`*SAME` leaves it);
    /// whether it changed, or why the value is not valid. `RMTSYS`
    /// takes its list blank-separated.
    pub fn set(&mut self, kwd: &str, v: &str) -> Result<bool, String> {
        if is_same(v) {
            return Ok(false);
        }
        let before = self.clone();
        let up = v.trim().to_ascii_uppercase();
        match kwd.to_ascii_uppercase().as_str() {
            "RMTSYS" => {
                if up == "*NONE" {
                    self.rmtsys.clear();
                } else {
                    let list: Vec<String> = v.split_whitespace().map(host).collect::<Result<_, _>>()?;
                    if list.len() > MAX_RMTSYS {
                        return Err(format!("RMTSYS: at most {MAX_RMTSYS} systems"));
                    }
                    self.rmtsys = list;
                }
            }
            "AUTOSTART" => self.autostart = yes_no(v, "AUTOSTART")?,
            "POLLITV" => self.pollitv = number(&up, "POLLITV", 1, 1440)?,
            "MINADJ" => {
                self.minadj = match up.as_str() {
                    "*NONE" | "-2" => None,
                    n => Some(number(n, "MINADJ", 0, 300_000)?),
                }
            }
            "MAXADJ" => self.maxadj = number(&up, "MAXADJ", 1, 120)?,
            "ADJTHLD" => {
                self.adjthld = match up.as_str() {
                    "*MAXADJ" | "-2" => None,
                    n => Some(number(n, "ADJTHLD", 1, 7200)?),
                }
            }
            "ACTLOG" => {
                if !matches!(up.as_str(), "*NONE" | "*CHANGE" | "*POLL") {
                    return Err(format!("ACTLOG({v}): *NONE, *CHANGE or *POLL"));
                }
                self.actlog = up;
            }
            "SVRAUTOSTR" => self.svrautostr = yes_no(v, "SVRAUTOSTR")?,
            "SVRACTLOG" => {
                if !matches!(up.as_str(), "*NONE" | "*ALL" | "*ERROR") {
                    return Err(format!("SVRACTLOG({v}): *NONE, *ALL or *ERROR"));
                }
                self.svractlog = up;
            }
            "SYNCRQD" => self.syncrqd = yes_no(v, "SYNCRQD")?,
            "PORT" => self.port = u16::try_from(number(&up, "PORT", 1, 65535)?).unwrap_or(123),
            other => return Err(format!("{other} is not an SNTP attribute")),
        }
        Ok(*self != before)
    }

    /// The client and server parts `QTOTNTP` runs, as request data.
    pub fn rqsdta(client: bool, server: bool) -> String {
        format!("CLIENT({}) SERVER({})", yn(client), yn(server))
    }

    /// The parts asked for in request data: (client, server).
    pub fn parts(rqsdta: &str) -> (bool, bool) {
        let on = |k| keyword(rqsdta, k).is_none_or(|v| v.eq_ignore_ascii_case("*YES"));
        (on("CLIENT"), on("SERVER"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let a = NtpAttrs::from_bag(&[]);
        let shown: Vec<String> = KEYWORDS.iter().map(|k| a.value(k)).collect();
        assert_eq!(
            shown,
            ["*NONE", "*NO", "60", "20", "20", "*MAXADJ", "*NONE", "*NO", "*NONE", "*YES"]
        );
        assert!(!a.autostarts());
    }

    #[test]
    fn set_and_round_trip() {
        let mut a = NtpAttrs::default();
        assert_eq!(a.set("RMTSYS", "time1.example.com 192.0.2.7"), Ok(true));
        assert_eq!(a.rmtsys, ["time1.example.com", "192.0.2.7"]);
        assert!(a.set("RMTSYS", "a b c d").is_err());
        assert!(a.set("RMTSYS", "bad host!").is_err());
        assert_eq!(a.set("MINADJ", "-2"), Ok(true));
        assert_eq!(a.minadj, None);
        assert_eq!(a.set("ADJTHLD", "300"), Ok(true));
        assert_eq!(a.set("ADJTHLD", "-1"), Ok(false));
        assert!(a.set("POLLITV", "0").is_err());
        assert!(a.set("MAXADJ", "121").is_err());
        assert_eq!(a.set("SVRAUTOSTR", "*yes"), Ok(true));
        assert!(a.autostarts());
        assert_eq!(a.set("ACTLOG", "*POLL"), Ok(true));
        assert!(a.set("SVRACTLOG", "*POLL").is_err());
        assert_eq!(a.set("SYNCRQD", "*NO"), Ok(true));
        assert_eq!(NtpAttrs::from_bag(&a.to_bag()), a);
        assert_eq!(a.set("RMTSYS", "*NONE"), Ok(true));
        assert_eq!(a.value("RMTSYS"), "*NONE");
    }

    #[test]
    fn parts() {
        assert_eq!(NtpAttrs::parts(&NtpAttrs::rqsdta(true, false)), (true, false));
        assert_eq!(NtpAttrs::parts(""), (true, true));
    }
}
