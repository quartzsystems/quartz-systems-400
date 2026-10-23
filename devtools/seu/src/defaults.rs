// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The session defaults (`F13`): two pages of choices, checked as the
//! display checks them, and what they mean to the session.

/// `Amount to roll`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Roll {
    /// `H`: half the lines shown.
    Half,
    /// `F`: all the lines shown.
    Full,
    /// `C`: the line under the cursor goes to the top (or bottom).
    Cursor,
    /// `D`: all the lines shown but one.
    Data,
    /// A number of lines.
    Lines(u32),
}

impl Roll {
    /// From the display's value.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "H" => Some(Self::Half),
            "F" => Some(Self::Full),
            "C" => Some(Self::Cursor),
            "D" => Some(Self::Data),
            n => n
                .parse::<u32>()
                .ok()
                .filter(|v| (1..=999).contains(v))
                .map(Self::Lines),
        }
    }

    /// The display's value.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Half => "H".into(),
            Self::Full => "F".into(),
            Self::Cursor => "C".into(),
            Self::Data => "D".into(),
            Self::Lines(n) => n.to_string(),
        }
    }

    /// Lines to move for `shown` lines on the display, the cursor's
    /// line among them (0-based) when it is on one.
    #[must_use]
    pub fn lines(self, shown: usize, cursor: Option<usize>) -> usize {
        match self {
            Self::Half => shown.div_ceil(2).max(1),
            Self::Full => shown.max(1),
            Self::Data => shown.saturating_sub(1).max(1),
            Self::Cursor => cursor.filter(|c| *c > 0).unwrap_or(shown).max(1),
            Self::Lines(n) => n as usize,
        }
    }
}

/// `Resequence member default`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reseq {
    /// `Y`.
    Yes,
    /// `N`.
    No,
    /// `P`: as the previous session left it.
    Previous,
}

/// The session defaults.
#[allow(clippy::struct_excessive_bools)] // the display's Y/N choices
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Defaults {
    /// Amount to roll.
    pub roll: Roll,
    /// Uppercase input only.
    pub upper: bool,
    /// Tabs on.
    pub tabs: bool,
    /// Increment of insert record, in hundredths.
    pub increment: u32,
    /// Full screen mode: no function key lines, more records.
    pub full: bool,
    /// The source type for prompts and syntax checking.
    pub srctype: String,
    /// Syntax checking when added or modified.
    pub syntax: bool,
    /// Syntax checking from sequence number (hundredths).
    pub syntax_from: Option<u32>,
    /// To sequence number.
    pub syntax_to: Option<u32>,
    /// Set records to date `yymmdd`.
    pub set_date: Option<u32>,
    /// Resequence member default.
    pub reseq: Reseq,
    /// Default to uppercase input for unknown source type.
    pub upper_unknown: bool,
    /// User exit program: `*REGFAC`, `*NONE` or a name.
    pub exit_pgm: String,
    /// Program selection filter: `*ALL`, `*USRPRF` or a string.
    pub filter: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            roll: Roll::Half,
            upper: true,
            tabs: false,
            increment: 1,
            full: false,
            srctype: String::new(),
            syntax: true,
            syntax_from: None,
            syntax_to: None,
            set_date: None,
            reseq: Reseq::Previous,
            upper_unknown: true,
            exit_pgm: "*REGFAC".into(),
            filter: "*ALL".into(),
        }
    }
}

/// The first page's values as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page1 {
    /// Amount to roll.
    pub roll: String,
    /// Uppercase input only.
    pub upper: String,
    /// Tabs on.
    pub tabs: String,
    /// Increment.
    pub increment: String,
    /// Full screen mode.
    pub full: String,
    /// Source type.
    pub srctype: String,
    /// Syntax checking when added/modified.
    pub syntax: String,
    /// From sequence number.
    pub from: String,
    /// To sequence number.
    pub to: String,
    /// Set records to date (`YY/MM/DD` or `YYMMDD`).
    pub date: String,
}

/// The second page's values as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page2 {
    /// Resequence member default.
    pub reseq: String,
    /// Default to uppercase input for unknown source type.
    pub upper_unknown: String,
    /// User exit program.
    pub exit_pgm: String,
    /// Program selection filter.
    pub filter: String,
}

/// A value the display refuses: which field (by its order on the
/// page, 0-based) and why.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{text}")]
pub struct Invalid {
    /// The field's index on its page.
    pub field: usize,
    /// What is wrong.
    pub text: String,
}

fn yes_no(field: usize, s: &str) -> Result<bool, Invalid> {
    match s.trim().to_ascii_uppercase().as_str() {
        "Y" => Ok(true),
        "N" => Ok(false),
        _ => Err(Invalid {
            field,
            text: "Type Y or N.".into(),
        }),
    }
}

/// An increment `0.01` to `999.99` in hundredths.
fn increment(field: usize, s: &str) -> Result<u32, Invalid> {
    let bad = || Invalid {
        field,
        text: "The increment is a number from 0.01 to 999.99.".into(),
    };
    let t = s.trim();
    let (w, f) = t.split_once('.').unwrap_or((t, ""));
    if w.len() > 3 || f.len() > 2 || !w.chars().chain(f.chars()).all(|c| c.is_ascii_digit()) {
        return Err(bad());
    }
    let whole: u32 = if w.is_empty() { 0 } else { w.parse().map_err(|_| bad())? };
    let mut frac: u32 = if f.is_empty() { 0 } else { f.parse().map_err(|_| bad())? };
    if f.len() == 1 {
        frac *= 10;
    }
    let v = whole * 100 + frac;
    if v == 0 {
        return Err(bad());
    }
    Ok(v)
}

/// A date `YY/MM/DD` or `YYMMDD`; blank (or just the separators) for
/// none.
#[must_use]
pub fn parse_date(s: &str) -> Option<Option<u32>> {
    let digits: String = s.chars().filter(char::is_ascii_digit).collect();
    let rest = s.chars().all(|c| c.is_ascii_digit() || c == '/' || c == ' ');
    if !rest {
        return None;
    }
    if digits.is_empty() {
        return Some(None);
    }
    if digits.len() != 6 {
        return None;
    }
    let v: u32 = digits.parse().ok()?;
    let (m, d) = (v / 100 % 100, v % 100);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(Some(v))
}

/// A date for the display: `YY/MM/DD`, or the separators alone.
#[must_use]
pub fn date_text(d: Option<u32>) -> String {
    match d {
        Some(v) => format!("{:02}/{:02}/{:02}", v / 10_000, v / 100 % 100, v % 100),
        None => "  /  /".into(),
    }
}

impl Defaults {
    /// The first page as the display shows it.
    #[must_use]
    pub fn page1(&self) -> Page1 {
        Page1 {
            roll: self.roll.text(),
            upper: yn(self.upper),
            tabs: yn(self.tabs),
            increment: format!("{}.{:02}", self.increment / 100, self.increment % 100),
            full: yn(self.full),
            srctype: self.srctype.clone(),
            syntax: yn(self.syntax),
            from: self.syntax_from.map(crate::seq_text).unwrap_or_default(),
            to: self.syntax_to.map(crate::seq_text).unwrap_or_default(),
            date: date_text(self.set_date),
        }
    }

    /// The second page as the display shows it.
    #[must_use]
    pub fn page2(&self) -> Page2 {
        Page2 {
            reseq: match self.reseq {
                Reseq::Yes => "Y",
                Reseq::No => "N",
                Reseq::Previous => "P",
            }
            .into(),
            upper_unknown: yn(self.upper_unknown),
            exit_pgm: self.exit_pgm.clone(),
            filter: self.filter.clone(),
        }
    }

    /// Take the first page's values: all or none.
    pub fn set_page1(&mut self, p: &Page1) -> Result<(), Invalid> {
        let roll = Roll::parse(&p.roll).ok_or(Invalid {
            field: 0,
            text: "Type H, F, C, D or a number of lines from 1 to 999.".into(),
        })?;
        let upper = yes_no(1, &p.upper)?;
        let tabs = yes_no(2, &p.tabs)?;
        let inc = increment(3, &p.increment)?;
        let full = yes_no(4, &p.full)?;
        let srctype = p.srctype.trim().to_ascii_uppercase();
        if srctype.len() > 10 {
            return Err(Invalid {
                field: 5,
                text: "A source type has at most 10 characters.".into(),
            });
        }
        let syntax = yes_no(6, &p.syntax)?;
        let seq = |field: usize, s: &str| -> Result<Option<u32>, Invalid> {
            if s.trim().is_empty() {
                return Ok(None);
            }
            crate::parse_seq(s).map(Some).ok_or(Invalid {
                field,
                text: "Type a sequence number from 0000.00 to 9999.99.".into(),
            })
        };
        let from = seq(7, &p.from)?;
        let to = seq(8, &p.to)?;
        if let (Some(f), Some(t)) = (from, to)
            && f > t
        {
            return Err(Invalid {
                field: 8,
                text: "The last sequence number is before the first.".into(),
            });
        }
        let date = parse_date(&p.date).ok_or(Invalid {
            field: 9,
            text: "Type the date as YY/MM/DD or YYMMDD.".into(),
        })?;
        self.roll = roll;
        self.upper = upper;
        self.tabs = tabs;
        self.increment = inc;
        self.full = full;
        self.srctype = srctype;
        self.syntax = syntax;
        self.syntax_from = from;
        self.syntax_to = to;
        self.set_date = date;
        Ok(())
    }

    /// Take the second page's values: all or none.
    pub fn set_page2(&mut self, p: &Page2) -> Result<(), Invalid> {
        let reseq = match p.reseq.trim().to_ascii_uppercase().as_str() {
            "Y" => Reseq::Yes,
            "N" => Reseq::No,
            "P" => Reseq::Previous,
            _ => {
                return Err(Invalid {
                    field: 0,
                    text: "Type Y, N or P.".into(),
                });
            }
        };
        let upper_unknown = yes_no(1, &p.upper_unknown)?;
        let exit_pgm = p.exit_pgm.trim().to_ascii_uppercase();
        if exit_pgm.is_empty() {
            return Err(Invalid {
                field: 2,
                text: "Type *REGFAC, *NONE or a program name.".into(),
            });
        }
        let filter = p.filter.trim().to_string();
        self.reseq = reseq;
        self.upper_unknown = upper_unknown;
        self.exit_pgm = exit_pgm;
        self.filter = if filter.is_empty() { "*ALL".into() } else { filter };
        Ok(())
    }
}

fn yn(b: bool) -> String {
    if b { "Y" } else { "N" }.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolls() {
        assert_eq!(Roll::parse("h"), Some(Roll::Half));
        assert_eq!(Roll::parse("12"), Some(Roll::Lines(12)));
        assert_eq!(Roll::parse("0"), None);
        assert_eq!(Roll::parse("1000"), None);
        assert_eq!(Roll::Half.lines(18, None), 9);
        assert_eq!(Roll::Full.lines(18, None), 18);
        assert_eq!(Roll::Data.lines(18, None), 17);
        assert_eq!(Roll::Cursor.lines(18, Some(5)), 5);
        assert_eq!(Roll::Cursor.lines(18, None), 18);
    }

    #[test]
    fn first_page_round_trip() {
        let mut d = Defaults::default();
        let mut p = d.page1();
        assert_eq!(p.increment, "0.01");
        assert_eq!(p.date, "  /  /");
        p.roll = "F".into();
        p.increment = "1".into();
        p.srctype = "clle".into();
        p.date = "26/10/23".into();
        d.set_page1(&p).unwrap();
        assert_eq!((d.roll, d.increment, d.srctype.as_str()), (Roll::Full, 100, "CLLE"));
        assert_eq!(d.set_date, Some(261_023));
        assert_eq!(d.page1().date, "26/10/23");
    }

    #[test]
    fn first_page_refusals() {
        let mut d = Defaults::default();
        let before = d.clone();
        let mut p = d.page1();
        p.tabs = "X".into();
        assert_eq!(d.set_page1(&p).unwrap_err().field, 2);
        let mut p = d.page1();
        p.increment = "0".into();
        assert_eq!(d.set_page1(&p).unwrap_err().field, 3);
        let mut p = d.page1();
        p.from = "20".into();
        p.to = "10".into();
        assert_eq!(d.set_page1(&p).unwrap_err().field, 8);
        let mut p = d.page1();
        p.date = "261341".into();
        assert_eq!(d.set_page1(&p).unwrap_err().field, 9);
        assert_eq!(d, before);
    }

    #[test]
    fn second_page() {
        let mut d = Defaults::default();
        let mut p = d.page2();
        assert_eq!((p.reseq.as_str(), p.exit_pgm.as_str()), ("P", "*REGFAC"));
        p.reseq = "n".into();
        p.exit_pgm = "*none".into();
        d.set_page2(&p).unwrap();
        assert_eq!((d.reseq, d.exit_pgm.as_str()), (Reseq::No, "*NONE"));
        p.reseq = "Q".into();
        assert!(d.set_page2(&p).is_err());
    }
}
