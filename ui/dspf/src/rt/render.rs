// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! How one field looks: its conditioning, its 5250 attribute from
//! `DSPATR` and `COLOR`, its field format word from the checks, and
//! the text of its value or constant.

use std::collections::BTreeMap;

use tn5250::ds::attr;
use tn5250::ds::field::{Adjust, Ffw, Shift};

use super::edit;
use crate::cond::Cond;
use crate::model::{Constant, Field, TypeClass, Usage};

/// The order of the job date's parts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateOrder {
    /// `*MDY`.
    #[default]
    Mdy,
    /// `*DMY`.
    Dmy,
    /// `*YMD`.
    Ymd,
    /// `*JUL`.
    Jul,
}

impl DateOrder {
    /// From `QDATFMT`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim_start_matches('*').to_ascii_uppercase().as_str() {
            "DMY" => Self::Dmy,
            "YMD" => Self::Ymd,
            "JUL" => Self::Jul,
            _ => Self::Mdy,
        }
    }
}

/// What the constants `DATE`, `TIME`, `USER`, `SYSNAME` and `MSGCON`
/// show.
#[derive(Clone, Debug, Default)]
pub struct Env {
    /// The job date: year, month, day.
    pub date: (u16, u8, u8),
    /// The system date.
    pub sys_date: (u16, u8, u8),
    /// The time of day: hours, minutes, seconds.
    pub time: (u8, u8, u8),
    /// The job's date format.
    pub datfmt: DateOrder,
    /// The job's date separator.
    pub datsep: char,
    /// The user profile.
    pub user: String,
    /// The system name.
    pub sysname: String,
    /// Message texts by identifier, for `MSGCON`.
    pub messages: BTreeMap<String, String>,
}

impl Env {
    /// A date's digits in the job's order, two- or four-digit year.
    fn date_digits(&self, d: (u16, u8, u8), year4: bool) -> String {
        let (y, m, dd) = d;
        let year = if year4 {
            format!("{y:04}")
        } else {
            format!("{:02}", y % 100)
        };
        match self.datfmt {
            DateOrder::Mdy => format!("{m:02}{dd:02}{year}"),
            DateOrder::Dmy => format!("{dd:02}{m:02}{year}"),
            DateOrder::Ymd => format!("{year}{m:02}{dd:02}"),
            DateOrder::Jul => format!("{year}{:03}", day_of_year(y, m, dd)),
        }
    }

    /// The same with the job's separators, as `EDTCDE(Y)` shows it.
    fn date_edited(&self, d: (u16, u8, u8), year4: bool) -> String {
        let s = self.date_digits(d, year4);
        let sep = if self.datsep == '\0' { '/' } else { self.datsep };
        let parts: Vec<&str> = match (self.datfmt, year4) {
            (DateOrder::Jul, false) => vec![&s[..2], &s[2..]],
            (DateOrder::Jul, true) => vec![&s[..4], &s[4..]],
            (DateOrder::Ymd, true) => vec![&s[..4], &s[4..6], &s[6..]],
            _ => vec![&s[..2], &s[2..4], &s[4..]],
        };
        parts.join(&sep.to_string())
    }
}

fn day_of_year(y: u16, m: u8, d: u8) -> u16 {
    let leap = (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let days: [u16; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let before: u16 = days.iter().take(usize::from(m.saturating_sub(1))).sum();
    before + u16::from(d)
}

/// Whether a condition holds: the display size matches when named,
/// and some group of indicators is wholly satisfied.
#[must_use]
pub fn cond_on(c: &Cond, inds: &[bool; 100], size_name: &str) -> bool {
    if let Some(s) = c.size
        && format!("*DS{}", s + 3) != size_name
    {
        return false;
    }
    if c.any.is_empty() {
        return true;
    }
    c.any.iter().any(|group| {
        group
            .iter()
            .all(|i| inds[usize::from(i.number)] == i.on)
    })
}

/// How a field is shown and taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look {
    /// The 5250 attribute.
    pub attr: u8,
    /// `DSPATR(PC)`: the cursor goes here.
    pub cursor: bool,
    /// `DSPATR(PR)`: no typing.
    pub protect: bool,
    /// `DSPATR(MDT)`: counts as changed.
    pub mdt: bool,
}

/// The attribute of a field from its `DSPATR` and `COLOR` keywords
/// that are in effect. `chginpdft` is the `CHGINPDFT` in force, if any:
/// its words replace the underline input fields get by default.
#[must_use]
pub fn look_of(f: &Field, inds: &[bool; 100], size_name: &str, chginpdft: Option<&[String]>) -> Look {
    let input = f.usage.is_input();
    let mut words: Vec<String> = Vec::new();
    if input {
        match chginpdft {
            Some(list) => words.extend(list.iter().cloned()),
            None => words.push("UL".into()),
        }
    }
    for a in &f.dspatr {
        if cond_on(&a.cond, inds, size_name) {
            words.extend(a.values.iter().cloned());
        }
    }
    let color = f
        .color
        .iter()
        .filter(|c| cond_on(&c.cond, inds, size_name))
        .filter_map(|c| c.values.first().cloned())
        .next_back();
    let has = |w: &str| words.iter().any(|x| x == w);
    let (hi, ri, ul, bl, nd, cs) = (
        has("HI"),
        has("RI"),
        has("UL"),
        has("BL"),
        has("ND"),
        has("CS"),
    );
    let base = match color.as_deref() {
        Some("WHT") => attr::WHITE,
        Some("RED") => attr::RED,
        Some("TRQ") => attr::TURQUOISE_CS,
        Some("YLW") => attr::YELLOW_CS,
        Some("PNK") => attr::PINK,
        Some("BLU") => attr::BLUE,
        _ => {
            if hi {
                attr::WHITE
            } else if cs {
                attr::TURQUOISE_CS
            } else {
                attr::GREEN
            }
        }
    };
    let mut a = base;
    if bl && matches!(base, attr::GREEN | attr::RED) {
        a = attr::RED_BL;
    }
    if ul {
        a |= 0x04;
    }
    if ri {
        a |= 0x01;
        if attr::is_nondisplay(a) {
            // Underline and reverse image together have no code in
            // this color: reverse image wins.
            a &= !0x04;
        }
    }
    if nd {
        a = attr::NONDISPLAY;
    }
    Look {
        attr: a,
        cursor: has("PC"),
        protect: has("PR"),
        mdt: has("MDT"),
    }
}

/// The field format word of an input field from its checks.
#[must_use]
pub fn ffw_of(f: &Field, look: Look) -> Ffw {
    let check = |code: &str| f.check.iter().any(|c| c == code);
    let has_kw = |name: &str| f.keywords.iter().any(|k| k.name == name);
    let numeric = f.ty.is_numeric();
    // Numeric only (Y) leaves the digits where they were typed; the
    // host aligns them.
    let numeric_only = numeric && f.ty.letter == 'Y';
    let shift = if numeric_only {
        Shift::NumericOnly
    } else if numeric {
        Shift::SignedNumeric
    } else {
        Shift::AlphaShift
    };
    let adjust = if check("MF") {
        Adjust::MandatoryFill
    } else if check("RZ") {
        Adjust::RightZero
    } else if check("RB") || numeric && !numeric_only {
        Adjust::RightBlank
    } else {
        Adjust::None
    };
    Ffw {
        bypass: look.protect,
        dup: has_kw("DUP"),
        mdt: look.mdt,
        shift,
        auto_enter: f
            .keywords
            .iter()
            .any(|k| k.name == "AUTO" && k.parms.iter().any(|p| p.as_word() == Some("RA"))),
        field_exit_required: check("FE"),
        monocase: !numeric && !check("LC"),
        mandatory_entry: check("ME"),
        adjust,
    }
}

/// The text a constant shows.
#[must_use]
pub fn constant_text(c: &Constant, f: &Field, env: &Env) -> String {
    let edited_date = matches!(&f.edit, crate::model::Edit::Code('Y', _));
    match c {
        Constant::Text(t) => t.clone(),
        Constant::Date { sys, year4 } => {
            let d = if *sys { env.sys_date } else { env.date };
            if edited_date {
                env.date_edited(d, *year4)
            } else {
                env.date_digits(d, *year4)
            }
        }
        Constant::Time { .. } => {
            let (hh, mm, ss) = env.time;
            format!("{hh:02}:{mm:02}:{ss:02}")
        }
        Constant::User => format!("{:<10}", env.user),
        Constant::SysName => format!("{:<8}", env.sysname),
        Constant::Message { len, id, .. } => {
            let t = env.messages.get(id).cloned().unwrap_or_default();
            let mut t: String = t.chars().take(*len as usize).collect();
            while t.chars().count() < *len as usize {
                t.push(' ');
            }
            t
        }
    }
}

/// The text of a named field's value from its buffer bytes: numbers
/// edited, the rest decoded. `None` bytes (an input-only field) show
/// blank, or the field's `DFT`.
#[must_use]
pub fn value_text(f: &Field, bytes: Option<&[u8]>) -> String {
    let Some(b) = bytes else {
        return f.dft.clone().unwrap_or_default();
    };
    match f.ty.class {
        TypeClass::Zoned | TypeClass::Packed | TypeClass::Binary | TypeClass::Float => {
            let v = edit::decode_number(f, b).unwrap_or_else(qdecimal::Decimal::zero);
            edit::display_number(f, &v)
        }
        _ => ccsid::from_ebcdic(b),
    }
}

/// Pad or cut text to the field's screen length; numbers are already
/// right-adjusted, the rest is left-adjusted.
#[must_use]
pub fn fit(text: &str, width: usize) -> String {
    let n = text.chars().count();
    if n >= width {
        text.chars().take(width).collect()
    } else {
        format!("{text:<width$}")
    }
}

/// Whether a field's bytes are in the output buffer (its usage says
/// so and it is not a constant).
#[must_use]
pub fn has_output(f: &Field) -> bool {
    f.in_output() && !matches!(f.usage, Usage::Program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cond::Ind;
    use crate::model::{Attr, FieldType};

    fn env() -> Env {
        Env {
            date: (2026, 10, 23),
            sys_date: (2026, 10, 23),
            time: (9, 5, 7),
            datfmt: DateOrder::Mdy,
            datsep: '/',
            user: "QSECOFR".into(),
            sysname: "QS400".into(),
            messages: BTreeMap::new(),
        }
    }

    #[test]
    fn conditions() {
        let mut inds = [false; 100];
        inds[50] = true;
        let c = Cond {
            size: None,
            any: vec![
                vec![Ind {
                    number: 50,
                    on: true,
                }],
                vec![Ind {
                    number: 51,
                    on: true,
                }],
            ],
        };
        assert!(cond_on(&c, &inds, "*DS3"));
        inds[50] = false;
        assert!(!cond_on(&c, &inds, "*DS3"));
        let sized = Cond {
            size: Some(1),
            any: vec![],
        };
        assert!(cond_on(&sized, &inds, "*DS4"));
        assert!(!cond_on(&sized, &inds, "*DS3"));
    }

    #[test]
    fn attributes() {
        let mut f = Field::new(Some("A".into()), FieldType::char(), 5, 0, Usage::Both);
        let inds = [false; 100];
        assert_eq!(look_of(&f, &inds, "*DS3", None).attr, attr::GREEN_UL);
        f.dspatr.push(Attr {
            values: vec!["HI".into(), "RI".into()],
            cond: Cond::always(),
            line: 0,
        });
        // White, underline and reverse image: reverse image wins.
        assert_eq!(look_of(&f, &inds, "*DS3", None).attr, attr::WHITE_RI);
        f.dspatr.clear();
        f.color.push(Attr {
            values: vec!["BLU".into()],
            cond: Cond::always(),
            line: 0,
        });
        assert_eq!(look_of(&f, &inds, "*DS3", Some(&[])).attr, attr::BLUE);
        f.dspatr.push(Attr {
            values: vec!["ND".into(), "PC".into()],
            cond: Cond::always(),
            line: 0,
        });
        let l = look_of(&f, &inds, "*DS3", None);
        assert_eq!(l.attr, attr::NONDISPLAY);
        assert!(l.cursor);
    }

    #[test]
    fn constants() {
        let e = env();
        let mut f = Field::new(None, FieldType::char(), 6, 0, Usage::Output);
        assert_eq!(
            constant_text(
                &Constant::Date {
                    sys: false,
                    year4: false
                },
                &f,
                &e
            ),
            "102326"
        );
        f.edit = crate::model::Edit::Code('Y', None);
        assert_eq!(
            constant_text(
                &Constant::Date {
                    sys: false,
                    year4: true
                },
                &f,
                &e
            ),
            "10/23/2026"
        );
        assert_eq!(constant_text(&Constant::Time { sys: false }, &f, &e), "09:05:07");
        assert_eq!(constant_text(&Constant::SysName, &f, &e), "QS400   ");
        assert_eq!(day_of_year(2024, 3, 1), 61);
    }
}
