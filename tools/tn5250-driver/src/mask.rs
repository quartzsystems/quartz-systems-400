// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Masks: what differs between systems and runs without meaning
//! anything (dates, times, the system name, the user, job numbers) is
//! replaced by a token of the same width before a screen is saved or
//! compared, on every target alike.

use anyhow::{Result, bail};
use regex::Regex;

/// One mask.
#[derive(Clone, Debug)]
pub struct Mask {
    /// Its name, for `+name`/`-name` on `snap`.
    pub name: String,
    re: Regex,
    /// The capture group replaced (0: the whole match).
    group: usize,
    token: String,
    /// In force unless named with `-`.
    pub enabled: bool,
}

impl Mask {
    /// A mask from a regex, the group to replace and the token.
    pub fn new(
        name: &str,
        pattern: &str,
        group: usize,
        token: &str,
        enabled: bool,
    ) -> Result<Self> {
        Ok(Self {
            name: name.into(),
            re: Regex::new(pattern)?,
            group,
            token: token.into(),
            enabled,
        })
    }

    /// `NAME REGEX => TOKEN [GROUP]`.
    pub fn parse(spec: &str) -> Result<Self> {
        let (name, rest) = spec
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((spec, ""));
        let Some((pattern, token)) = rest.split_once("=>") else {
            bail!("mask wants NAME REGEX => TOKEN");
        };
        let mut t = token.split_whitespace();
        let token = t.next().unwrap_or("");
        let group = t.next().map_or(0, |g| g.parse().unwrap_or(0));
        Self::new(name, pattern.trim(), group, token, true)
    }

    /// Replace in one line, keeping its width.
    pub fn apply(&self, line: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for cap in self.re.captures_iter(line) {
            let Some(m) = cap.get(self.group) else {
                continue;
            };
            if m.start() < last {
                continue;
            }
            out.push_str(&line[last..m.start()]);
            let width = m.as_str().chars().count();
            let mut tok: String = self.token.chars().take(width).collect();
            while tok.chars().count() < width {
                tok.push(' ');
            }
            out.push_str(&tok);
            last = m.end();
        }
        out.push_str(&line[last..]);
        out
    }
}

/// The masks in force.
#[derive(Clone, Debug, Default)]
pub struct Masks {
    list: Vec<Mask>,
}

impl Masks {
    /// The defaults: dates, times, the system name; job numbers, device
    /// names and serial numbers off until named.
    pub fn defaults() -> Self {
        let mut m = Self::default();
        let defaults = [
            ("date", r"\b\d\d[/.\-]\d\d[/.\-]\d\d\b", 0, "MM/DD/YY", true),
            ("time", r"\b\d\d:\d\d:\d\d\b", 0, "HH:MM:SS", true),
            ("hhmm", r"\b\d\d:\d\d\b", 0, "HH:MM", false),
            (
                "sysname",
                r"(System[ .]*:\s+)([A-Z0-9]+ *)",
                2,
                "SYSNAME",
                true,
            ),
            ("jobnum", r"\b\d{6}\b", 0, "NNNNNN", false),
            ("device", r"\bQPADEV[0-9A-Z]{4}\b", 0, "QPADEVNNNN", false),
            (
                "serial",
                r"(Serial number[ .]*:\s+)([A-Z0-9]+)",
                2,
                "SERIAL",
                false,
            ),
            ("cpu", r"\b\d{1,3}[.,]\d\b", 0, "N.N", false),
            ("number", r"\b\d+\b", 0, "N", false),
        ];
        for (name, pat, group, token, enabled) in defaults {
            m.list
                .push(Mask::new(name, pat, group, token, enabled).expect("a default mask"));
        }
        m
    }

    /// The user mask: the user's name as a word.
    pub fn set_user(&mut self, user: &str) {
        self.list.retain(|m| m.name != "user");
        if !user.is_empty() {
            let pat = format!(r"\b{}\b", regex::escape(&user.to_ascii_uppercase()));
            self.list
                .push(Mask::new("user", &pat, 0, "USERNAME", true).expect("a user mask"));
        }
    }

    /// Add or replace a mask by name.
    pub fn add(&mut self, m: Mask) {
        self.list.retain(|x| x.name != m.name);
        self.list.push(m);
    }

    /// Turn a mask on.
    pub fn enable(&mut self, name: &str) -> Result<()> {
        match self.list.iter_mut().find(|m| m.name == name) {
            Some(m) => {
                m.enabled = true;
                Ok(())
            }
            None => bail!("no mask named {name}"),
        }
    }

    /// Turn a mask off.
    pub fn disable(&mut self, name: &str) {
        if let Some(m) = self.list.iter_mut().find(|m| m.name == name) {
            m.enabled = false;
        }
    }

    /// Mask every row.
    pub fn apply(&self, rows: &[String]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                let mut line = r.clone();
                for m in self.list.iter().filter(|m| m.enabled) {
                    line = m.apply(&line);
                }
                line
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_width() {
        let m = Masks::defaults();
        let rows = vec![
            "                                                              System:  PUB400    "
                .to_string(),
            " 23.10.26  14:05:33   10/23/26".to_string(),
        ];
        let out = m.apply(&rows);
        assert_eq!(out[0].len(), rows[0].len());
        assert!(out[0].contains("System:  SYSNAME"));
        assert_eq!(out[1], " MM/DD/YY  HH:MM:SS   MM/DD/YY");
    }

    #[test]
    fn user_mask() {
        let mut m = Masks::defaults();
        m.set_user("someuser");
        let out = m.apply(&["User SOMEUSER here".to_string()]);
        assert_eq!(out[0], "User USERNAME here");
    }

    #[test]
    fn parse_spec() {
        let m = Mask::parse(r"job  QPADEV\d+ => DEV").unwrap();
        assert_eq!(m.apply("x QPADEV0003 y"), "x DEV        y");
    }
}
