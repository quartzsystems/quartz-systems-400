// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IPL attributes `CHGIPLA` changes and `DSPIPLA` shows, IBM's set,
//! kept in the record (TLV [`TAG_IPL_ATTRS`]) because the IPL reads
//! them: the keylock before anything else, the rest when the operating
//! system starts.

use crate::error::LicError;
use crate::format::Record;

/// The record's TLV tag for the attributes.
pub const TAG_IPL_ATTRS: u16 = 0x0106;

/// One attribute: keyword, the values it takes, the default, and where
/// `DSPIPLA` shows it (section, label).
pub struct Attr {
    /// `CHGIPLA` keyword.
    pub kwd: &'static str,
    /// Values, special values as IBM spells them.
    pub values: &'static [&'static str],
    /// The shipped value.
    pub default: &'static str,
    /// `DSPIPLA` section.
    pub section: &'static str,
    /// `DSPIPLA` label.
    pub label: &'static str,
}

const YES_NO: &[&str] = &["*YES", "*NO"];
const NONE_TO_ALL: &[&str] = &["*NONE", "*NORMAL", "*ABNORMAL", "*ALL"];

/// Every attribute, in `DSPIPLA` order.
pub const ATTRS: &[Attr] = &[
    Attr { kwd: "RESTART", values: &["*SYS", "*FULL"], default: "*SYS", section: "System Features", label: "Restart type" },
    Attr { kwd: "KEYLCKPOS", values: &["*NORMAL", "*AUTO", "*MANUAL", "*SECURE"], default: "*NORMAL", section: "System Features", label: "Keylock position" },
    Attr { kwd: "HDWDIAG", values: &["*MIN", "*ALL"], default: "*MIN", section: "Hardware Functions", label: "Hardware diagnostics" },
    Attr { kwd: "CPRJOBTBL", values: NONE_TO_ALL, default: "*NONE", section: "Operating System Functions", label: "Compress job tables" },
    Attr { kwd: "CHKJOBTBL", values: &["*ABNORMAL", "*ALL", "*SYNC"], default: "*ABNORMAL", section: "Operating System Functions", label: "Check job tables" },
    Attr { kwd: "RBDPRDDIR", values: NONE_TO_ALL, default: "*NORMAL", section: "Operating System Functions", label: "Rebuild product directory" },
    Attr { kwd: "MSFRCY", values: &["*NONE", "*ALL"], default: "*NONE", section: "Operating System Functions", label: "Mail Server Framework recovery" },
    Attr { kwd: "DSPSTS", values: &["*ALL", "*NONE", "*SYS", "*USER"], default: "*ALL", section: "Operating System Functions", label: "Display status" },
    Attr { kwd: "STRTCP", values: YES_NO, default: "*YES", section: "Operating System Functions", label: "Start TCP/IP" },
    Attr { kwd: "SPLFRCY", values: &["*DETACH", "*REMOVE"], default: "*DETACH", section: "Operating System Functions", label: "Spooled file recovery" },
    Attr { kwd: "CLRJOBQ", values: YES_NO, default: "*NO", section: "Operating System Options", label: "Clear job queues" },
    Attr { kwd: "CLROUTQ", values: YES_NO, default: "*NO", section: "Operating System Options", label: "Clear output queues" },
    Attr { kwd: "CLRINCJOB", values: YES_NO, default: "*NO", section: "Operating System Options", label: "Clear incomplete joblogs" },
    Attr { kwd: "STRPRTWTR", values: YES_NO, default: "*YES", section: "Operating System Options", label: "Start print writers" },
    Attr { kwd: "STRRSTD", values: YES_NO, default: "*NO", section: "Operating System Options", label: "Start system to restricted state" },
];

/// The attributes' values, by keyword, in [`ATTRS`] order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IplAttrs {
    values: Vec<&'static str>,
}

impl Default for IplAttrs {
    fn default() -> Self {
        Self {
            values: ATTRS.iter().map(|a| a.default).collect(),
        }
    }
}

/// A label indented three, with the dot leader to the colon at column 51.
fn leader(label: &str) -> String {
    let mut s = format!("   {label}");
    let len = s.len();
    while s.len() < 51 {
        s.push(if s.len() % 2 == 1 && s.len() > len { '.' } else { ' ' });
    }
    s.push(':');
    s
}

fn index(kwd: &str) -> Option<usize> {
    ATTRS.iter().position(|a| a.kwd == kwd)
}

impl IplAttrs {
    /// A value by keyword; `None` for an unknown keyword.
    #[must_use]
    pub fn get(&self, kwd: &str) -> Option<&'static str> {
        index(kwd).map(|i| self.values[i])
    }

    /// Whether a `*YES`/`*NO` attribute is `*YES`.
    #[must_use]
    pub fn yes(&self, kwd: &str) -> bool {
        self.get(kwd) == Some("*YES")
    }

    /// Change one; `*SAME` changes nothing.
    pub fn set(&mut self, kwd: &str, value: &str) -> Result<(), LicError> {
        let i = index(kwd).ok_or(LicError::Malformed("IPL attribute"))?;
        if value == "*SAME" {
            return Ok(());
        }
        let v = ATTRS[i]
            .values
            .iter()
            .find(|v| v.eq_ignore_ascii_case(value))
            .ok_or(LicError::Malformed("IPL attribute value"))?;
        self.values[i] = v;
        Ok(())
    }

    /// The TLV's bytes: `KEYWORD=VALUE` lines.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = String::new();
        for (a, v) in ATTRS.iter().zip(&self.values) {
            out.push_str(a.kwd);
            out.push('=');
            out.push_str(v);
            out.push('\n');
        }
        out.into_bytes()
    }

    /// From the TLV's bytes; unknown lines are skipped, missing ones
    /// take the default.
    #[must_use]
    pub fn decode(b: &[u8]) -> Self {
        let mut a = Self::default();
        for line in String::from_utf8_lossy(b).lines() {
            if let Some((k, v)) = line.split_once('=') {
                let _ = a.set(k.trim(), v.trim());
            }
        }
        a
    }

    /// The text form `DSPIPLA` shows and prints: a heading, then each
    /// section and its attributes, labels dot-led to column 51.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::from("Display IPL Attributes\n");
        let mut section = "";
        for (a, v) in ATTRS.iter().zip(&self.values) {
            if a.section != section {
                section = a.section;
                out.push(' ');
                out.push_str(section);
                out.push_str(":\n");
            }
            out.push_str(&leader(a.label));
            out.push_str("   ");
            out.push_str(v);
            out.push('\n');
        }
        out
    }

    /// The record's attributes.
    #[must_use]
    pub fn of(record: &Record) -> Self {
        record.tlv(TAG_IPL_ATTRS).map(Self::decode).unwrap_or_default()
    }

    /// Keep them in the record.
    pub fn store(&self, record: &mut Record) -> Result<(), LicError> {
        record.set_tlv(TAG_IPL_ATTRS, self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_round_trip_in_the_record() {
        let mut r = Record::new(1);
        assert_eq!(IplAttrs::of(&r), IplAttrs::default());
        let mut a = IplAttrs::default();
        a.set("CLRJOBQ", "*yes").unwrap();
        a.set("KEYLCKPOS", "*MANUAL").unwrap();
        a.set("STRTCP", "*SAME").unwrap();
        assert!(a.set("STRTCP", "*MAYBE").is_err());
        assert!(a.set("NOSUCH", "*YES").is_err());
        a.store(&mut r).unwrap();
        let back = IplAttrs::of(&Record::decode(&r.encode().unwrap()).unwrap());
        assert_eq!(back, a);
        assert!(back.yes("CLRJOBQ"));
        assert!(back.yes("STRTCP"));
        assert_eq!(back.get("KEYLCKPOS"), Some("*MANUAL"));
        assert_eq!(back.get("HDWDIAG"), Some("*MIN"));
    }

    #[test]
    fn the_text_lines_up_as_the_oracle_prints_it() {
        // tools/conformance/oracle/save/primary/33-ipla.txt (V7R1M0).
        let t = IplAttrs::default().text();
        for line in [
            " System Features:",
            "   Restart type  . . . . . . . . . . . . . . . . . :   *SYS",
            "   Keylock position  . . . . . . . . . . . . . . . :   *NORMAL",
            "   Compress job tables . . . . . . . . . . . . . . :   *NONE",
            "   Mail Server Framework recovery  . . . . . . . . :   *NONE",
            "   Start TCP/IP  . . . . . . . . . . . . . . . . . :   *YES",
            "   Start system to restricted state  . . . . . . . :   *NO",
            " Operating System Options:",
        ] {
            assert!(t.lines().any(|l| l == line), "{line:?} not in\n{t}");
        }
    }
}
