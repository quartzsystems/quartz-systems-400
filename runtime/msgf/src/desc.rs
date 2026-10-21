// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A message description and the substitution of message data into
//! its text.

use std::fmt::Write as _;

use crate::error::{MsgfError, Result};

/// A substitution variable's type, as `ADDMSGD FMT` declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VarType {
    /// `*CHAR n`: CP037 text, trailing blanks dropped.
    Char(u32),
    /// `*CHAR *VARY n`: a 2- or 4-byte length then the text.
    VaryChar(u8),
    /// `*DEC d s`: packed decimal.
    Dec(u8, u8),
    /// `*HEX n`: bytes shown as hexadecimal.
    Hex(u32),
    /// `*QTDCHAR n`: text shown in quotes.
    QtdChar(u32),
    /// `*BIN n`: a 2-, 4- or 8-byte binary integer.
    Bin(u8),
    /// `*DTS`: an 8-byte timestamp, microseconds since the epoch.
    Dts,
    /// `*CCHAR n`: convertible text; shown as text.
    CChar(u32),
    /// `*ITYPE n`: an integer whose type the data names; shown as text.
    IType(u32),
    /// `*UTC`: like `*DTS`, in UTC.
    Utc,
    /// `*SPP`: a 16-byte space pointer, shown in hexadecimal.
    Spp,
    /// `*SYP`: a 16-byte system pointer, shown in hexadecimal.
    Syp,
}

impl VarType {
    /// From `*CHAR 10`, `*DEC 7 2`, `*CHAR *VARY 2` and the rest.
    pub fn parse(s: &str) -> Result<Self> {
        let words: Vec<&str> = s.split_whitespace().collect();
        let bad = || MsgfError::Var(s.to_string());
        let num = |i: usize| -> Result<u32> {
            words
                .get(i)
                .ok_or_else(bad)?
                .parse::<u32>()
                .map_err(|_| bad())
        };
        Ok(
            match words.first().map(|w| w.to_ascii_uppercase()).as_deref() {
                Some("*CHAR")
                    if words
                        .get(1)
                        .is_some_and(|w| w.eq_ignore_ascii_case("*VARY")) =>
                {
                    let n = num(2)?;
                    if n != 2 && n != 4 {
                        return Err(bad());
                    }
                    Self::VaryChar(u8::try_from(n).expect("2 or 4"))
                }
                Some("*CHAR") => Self::Char(num(1)?),
                Some("*DEC") => {
                    let d = u8::try_from(num(1)?).map_err(|_| bad())?;
                    let sc = u8::try_from(num(2).unwrap_or(0)).map_err(|_| bad())?;
                    Self::Dec(d, sc)
                }
                Some("*HEX") => Self::Hex(num(1)?),
                Some("*QTDCHAR") => Self::QtdChar(num(1)?),
                Some("*BIN") => {
                    let n = num(1).unwrap_or(2);
                    if !matches!(n, 2 | 4 | 8) {
                        return Err(bad());
                    }
                    Self::Bin(u8::try_from(n).expect("small"))
                }
                Some("*DTS") => Self::Dts,
                Some("*CCHAR") => Self::CChar(num(1)?),
                Some("*ITYPE") => Self::IType(num(1)?),
                Some("*UTC") => Self::Utc,
                Some("*SPP") => Self::Spp,
                Some("*SYP") => Self::Syp,
                _ => return Err(bad()),
            },
        )
    }

    /// The declaration as `ADDMSGD` writes it.
    #[must_use]
    pub fn declaration(&self) -> String {
        match self {
            Self::Char(n) => format!("*CHAR {n}"),
            Self::VaryChar(n) => format!("*CHAR *VARY {n}"),
            Self::Dec(d, s) => format!("*DEC {d} {s}"),
            Self::Hex(n) => format!("*HEX {n}"),
            Self::QtdChar(n) => format!("*QTDCHAR {n}"),
            Self::Bin(n) => format!("*BIN {n}"),
            Self::Dts => "*DTS".into(),
            Self::CChar(n) => format!("*CCHAR {n}"),
            Self::IType(n) => format!("*ITYPE {n}"),
            Self::Utc => "*UTC".into(),
            Self::Spp => "*SPP".into(),
            Self::Syp => "*SYP".into(),
        }
    }

    /// Bytes of message data the variable takes; for `*CHAR *VARY`,
    /// the prefix alone.
    #[must_use]
    pub fn fixed_size(&self) -> usize {
        match self {
            Self::Char(n) | Self::Hex(n) | Self::QtdChar(n) | Self::CChar(n) | Self::IType(n) => {
                *n as usize
            }
            Self::VaryChar(p) | Self::Bin(p) => usize::from(*p),
            Self::Dec(d, _) => usize::from(*d) / 2 + 1,
            Self::Dts | Self::Utc => 8,
            Self::Spp | Self::Syp => 16,
        }
    }

    /// Render the variable from message data; returns the text and the
    /// bytes consumed. Short data renders what is there.
    #[must_use]
    pub fn render(&self, data: &[u8]) -> (String, usize) {
        let take = |n: usize| &data[..n.min(data.len())];
        match self {
            Self::Char(n) | Self::CChar(n) | Self::IType(n) => {
                let b = take(*n as usize);
                (ccsid::decode_trimmed(b), b.len())
            }
            Self::QtdChar(n) => {
                let b = take(*n as usize);
                (format!("'{}'", ccsid::decode_trimmed(b)), b.len())
            }
            Self::VaryChar(p) => {
                let p = usize::from(*p);
                if data.len() < p {
                    return (String::new(), data.len());
                }
                let len = if p == 2 {
                    usize::from(u16::from_be_bytes([data[0], data[1]]))
                } else {
                    usize::try_from(u32::from_be_bytes(data[..4].try_into().expect("4")))
                        .unwrap_or(0)
                };
                let b = &data[p..(p + len).min(data.len())];
                (ccsid::decode_trimmed(b), p + b.len())
            }
            Self::Dec(d, s) => {
                let n = usize::from(*d) / 2 + 1;
                let b = take(n);
                let text = qdecimal::Packed::new(*d, *s)
                    .and_then(|p| p.decode(b))
                    .map_or_else(|_| "?".to_string(), |v| v.to_string());
                (text, b.len())
            }
            Self::Hex(n) => {
                let b = take(*n as usize);
                (hex(b), b.len())
            }
            Self::Bin(p) => {
                let p = usize::from(*p);
                let b = take(p);
                let v = match b.len() {
                    2 => i64::from(i16::from_be_bytes([b[0], b[1]])),
                    4 => i64::from(i32::from_be_bytes(b.try_into().expect("4"))),
                    8 => i64::from_be_bytes(b.try_into().expect("8")),
                    _ => 0,
                };
                (v.to_string(), b.len())
            }
            Self::Dts | Self::Utc => {
                let b = take(8);
                if b.len() < 8 {
                    return (String::new(), b.len());
                }
                let micros = u64::from_be_bytes(b.try_into().expect("8"));
                (timestamp_text(micros), 8)
            }
            Self::Spp | Self::Syp => {
                let b = take(16);
                (hex(b), b.len())
            }
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02X}");
        s
    })
}

/// `yyyy-mm-dd-hh.mm.ss` of microseconds since 1970-01-01 UTC.
fn timestamp_text(micros: u64) -> String {
    let secs = micros / 1_000_000;
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let sod = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}-{:02}.{:02}.{:02}",
        sod / 3600,
        (sod % 3600) / 60,
        sod % 60
    )
}

/// A message description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsgDesc {
    /// `AAAnnnn`.
    pub id: String,
    /// Severity 0 to 99.
    pub severity: u16,
    /// First-level text with `&n` substitution variables.
    pub text: String,
    /// Second-level text.
    pub help: String,
    /// The substitution variables, `&1` first.
    pub vars: Vec<VarType>,
    /// Default reply, for inquiry messages.
    pub dft_reply: Option<String>,
    /// Valid replies; empty for any.
    pub valid_replies: Vec<String>,
    /// Variables to dump with the job (`DMPLST`), by number.
    pub dump: Vec<u16>,
}

impl MsgDesc {
    /// Whether an identifier is three letters and four hexadecimal
    /// digits, as `ADDMSGD` requires.
    #[must_use]
    pub fn valid_id(id: &str) -> bool {
        let b = id.as_bytes();
        b.len() == 7
            && b[..3].iter().all(u8::is_ascii_alphabetic)
            && b[3..].iter().all(u8::is_ascii_hexdigit)
    }

    /// The message data as rendered variables, in order.
    #[must_use]
    pub fn render_vars(&self, data: &[u8]) -> Vec<String> {
        let mut at = 0;
        let mut out = Vec::with_capacity(self.vars.len());
        for v in &self.vars {
            let rest = data.get(at..).unwrap_or(&[]);
            let (text, used) = v.render(rest);
            out.push(text);
            at += used;
        }
        out
    }

    /// The first-level text with the data substituted.
    #[must_use]
    pub fn format(&self, data: &[u8]) -> String {
        substitute(&self.text, &self.render_vars(data))
    }

    /// The second-level text with the data substituted.
    #[must_use]
    pub fn format_help(&self, data: &[u8]) -> String {
        substitute(&self.help, &self.render_vars(data))
    }

    /// Whether a reply is one the description allows.
    #[must_use]
    pub fn reply_ok(&self, reply: &str) -> bool {
        self.valid_replies.is_empty()
            || self
                .valid_replies
                .iter()
                .any(|r| r.eq_ignore_ascii_case(reply))
    }
}

/// Replace `&n` with the n-th value; an `&` that starts no variable,
/// or a number past the list, stays as written.
#[must_use]
pub fn substitute(template: &str, values: &[String]) -> String {
    let mut out = String::with_capacity(template.len());
    let b = template.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'&' {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_digit() && j - i <= 2 {
                j += 1;
            }
            if j > i + 1 {
                let n: usize = template[i + 1..j].parse().unwrap_or(0);
                if n >= 1 && n <= values.len() {
                    out.push_str(&values[n - 1]);
                    i = j;
                    continue;
                }
            }
        }
        let ch = template[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_parse_and_render() {
        assert_eq!(VarType::parse("*CHAR 10").unwrap(), VarType::Char(10));
        assert_eq!(
            VarType::parse("*char *vary 2").unwrap(),
            VarType::VaryChar(2)
        );
        assert_eq!(VarType::parse("*DEC 7 2").unwrap(), VarType::Dec(7, 2));
        assert!(VarType::parse("*BIN 3").is_err());
        let d = MsgDesc {
            id: "CPF1234".into(),
            severity: 10,
            text: "File &1 has &2 rows, hex &3, &&4 stays".into(),
            help: String::new(),
            vars: vec![VarType::Char(10), VarType::Dec(5, 0), VarType::Hex(2)],
            dft_reply: None,
            valid_replies: Vec::new(),
            dump: Vec::new(),
        };
        let mut data = ccsid::to_ebcdic("CUST      ").unwrap();
        data.extend_from_slice(&[0x00, 0x12, 0x3F]);
        data.extend_from_slice(&[0xAB, 0xCD]);
        assert_eq!(
            d.format(&data),
            "File CUST has 123 rows, hex ABCD, &&4 stays"
        );
        assert_eq!(
            d.format(&data[..4]),
            "File CUST has ? rows, hex , &&4 stays"
        );
        assert!(MsgDesc::valid_id("MCH1211"));
        assert!(!MsgDesc::valid_id("MCH12"));
    }

    #[test]
    fn timestamps_render() {
        assert_eq!(timestamp_text(0), "1970-01-01-00.00.00");
        assert_eq!(
            timestamp_text(1_767_225_600 * 1_000_000),
            "2026-01-01-00.00.00"
        );
        let (t, n) =
            VarType::Dts.render(&(1_767_225_600u64 * 1_000_000 + 61_000_000).to_be_bytes());
        assert_eq!((t.as_str(), n), ("2026-01-01-00.01.01", 8));
    }
}
