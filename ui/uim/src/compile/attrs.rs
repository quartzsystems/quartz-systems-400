// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Typed attribute readers over an element: each reports a value it
//! cannot use and answers `None`, so the builders fill a default.

use super::Build;
use crate::diag::ids;
use crate::model::{
    Action, BaseType, Case, CmdLineSize, Depth, Display, Layout, Scroll, Separator, Shift, Usage,
    VarUpd,
};
use crate::tree::Element;

/// A UIM name: up to 10 characters, a letter first, then letters,
/// digits and `_ @ # $`.
#[must_use]
pub fn valid_name(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    s.len() <= 10
        && first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '#' | '$'))
}

impl Build {
    /// The raw value; a flag written without a value is a diagnostic.
    pub(super) fn value<'a>(&mut self, el: &'a Element, attr: &str) -> Option<&'a str> {
        let a = el.attr(attr)?;
        if let Some(v) = &a.value {
            return Some(v.as_str());
        }
        self.diags
            .push(ids::attr_needs_value(a.line, &el.name, attr));
        None
    }

    /// Text as written (quoted values keep their case).
    pub(super) fn text(&mut self, el: &Element, attr: &str) -> Option<String> {
        self.value(el, attr).map(str::to_string)
    }

    /// Text folded to upper case and trimmed.
    pub(super) fn upper(&mut self, el: &Element, attr: &str) -> Option<String> {
        self.value(el, attr).map(|v| v.trim().to_ascii_uppercase())
    }

    /// A UIM name.
    pub(super) fn name(&mut self, el: &Element, attr: &str, what: &str) -> Option<String> {
        let line = el.line;
        let v = self.upper(el, attr)?;
        if valid_name(&v) {
            Some(v)
        } else {
            self.diags.push(ids::bad_name(line, what, &v));
            None
        }
    }

    /// A blank-separated list of upper-case names.
    pub(super) fn names(&mut self, el: &Element, attr: &str) -> Vec<String> {
        self.upper(el, attr)
            .map(|v| v.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default()
    }

    /// A number.
    pub(super) fn u32(&mut self, el: &Element, attr: &str) -> Option<u32> {
        let line = el.line;
        let v = self.upper(el, attr)?;
        if let Ok(n) = v.parse() {
            return Some(n);
        }
        self.diags.push(ids::bad_attr(
            line,
            &el.name,
            attr,
            &v,
            "a number is needed",
        ));
        None
    }

    /// A flag: present without a value or `YES` is true, `NO` false.
    pub(super) fn flag(&mut self, el: &Element, attr: &str) -> bool {
        let Some(a) = el.attr(attr) else {
            return false;
        };
        match a.value.as_deref().map(str::to_ascii_uppercase).as_deref() {
            None | Some("YES") => true,
            Some("NO") => false,
            Some(other) => {
                self.diags
                    .push(ids::bad_attr(a.line, &el.name, attr, other, "YES or NO"));
                false
            }
        }
    }

    /// One of a fixed set of words.
    pub(super) fn choice(&mut self, el: &Element, attr: &str, words: &[&str]) -> Option<String> {
        let line = el.line;
        let v = self.upper(el, attr)?;
        if words.contains(&v.as_str()) {
            Some(v)
        } else {
            self.diags.push(ids::bad_attr(
                line,
                &el.name,
                attr,
                &v,
                format!("one of {}", words.join(", ")),
            ));
            None
        }
    }

    /// `DEPTH`: a number or `*`.
    pub(super) fn depth(&mut self, el: &Element, attr: &str) -> Option<Depth> {
        let line = el.line;
        let v = self.upper(el, attr)?;
        if v == "*" {
            return Some(Depth::Fill);
        }
        match v.parse::<u32>() {
            Ok(n) if n > 0 => Some(Depth::Lines(n)),
            _ => {
                self.diags.push(ids::bad_attr(
                    line,
                    &el.name,
                    attr,
                    &v,
                    "a count of lines or *",
                ));
                None
            }
        }
    }

    pub(super) fn usage(&mut self, el: &Element, attr: &str) -> Option<Usage> {
        Some(
            match self
                .choice(el, attr, &["INPUT", "OUTPUT", "INOUT"])?
                .as_str()
            {
                "INPUT" => Usage::Input,
                "OUTPUT" => Usage::Output,
                _ => Usage::Inout,
            },
        )
    }

    pub(super) fn separator(&mut self, el: &Element, attr: &str) -> Separator {
        match self
            .choice(el, attr, &["SPACE", "RULE", "NONE", "SYSNAME"])
            .as_deref()
        {
            Some("RULE") => Separator::Rule,
            Some("NONE") => Separator::None,
            Some("SYSNAME") => Separator::SysName,
            _ => Separator::Space,
        }
    }

    pub(super) fn layout(&mut self, el: &Element, attr: &str) -> Layout {
        match self.choice(el, attr, &["1", "2", "3"]).as_deref() {
            Some("2") => Layout::Two,
            Some("3") => Layout::Three,
            _ => Layout::One,
        }
    }

    pub(super) fn scroll(&mut self, el: &Element, attr: &str) -> Scroll {
        if el.attr(attr).is_some() && self.flag(el, attr) {
            Scroll::Yes
        } else {
            Scroll::No
        }
    }

    pub(super) fn display(&mut self, el: &Element, attr: &str) -> Display {
        if el.attr(attr).is_some() && !self.flag(el, attr) {
            Display::No
        } else {
            Display::Yes
        }
    }

    pub(super) fn varupd(&mut self, el: &Element, attr: &str) -> Option<VarUpd> {
        el.attr(attr)?;
        Some(if self.flag(el, attr) {
            VarUpd::Yes
        } else {
            VarUpd::No
        })
    }

    pub(super) fn shift(&mut self, el: &Element, attr: &str) -> Option<Shift> {
        Some(
            match self
                .choice(
                    el,
                    attr,
                    &[
                        "NONE", "ALPHA", "NUMERIC", "DIGITS", "NUMONLY", "SIGNED", "KATAKANA",
                    ],
                )?
                .as_str()
            {
                "ALPHA" => Shift::Alpha,
                "NUMERIC" => Shift::Numeric,
                "DIGITS" => Shift::DigitsOnly,
                "NUMONLY" => Shift::NumericOnly,
                "SIGNED" => Shift::Signed,
                "KATAKANA" => Shift::Katakana,
                _ => Shift::None,
            },
        )
    }

    pub(super) fn case(&mut self, el: &Element, attr: &str) -> Case {
        match self.choice(el, attr, &["MIXED", "UPPER"]).as_deref() {
            Some("UPPER") => Case::Upper,
            _ => Case::Mixed,
        }
    }

    pub(super) fn cmdline_size(&mut self, el: &Element, attr: &str) -> CmdLineSize {
        match self.choice(el, attr, &["SHORT", "LONG"]).as_deref() {
            Some("SHORT") => CmdLineSize::Short,
            _ => CmdLineSize::Long,
        }
    }

    /// An action; its name references are recorded.
    pub(super) fn action(&mut self, el: &Element, attr: &str) -> Option<Action> {
        let line = el.line;
        let v = self.text(el, attr)?;
        match Action::parse(&v) {
            Ok(a) => {
                self.action_refs(&a, line);
                Some(a)
            }
            Err(why) => {
                self.diags.push(ids::bad_action(line, &v, why));
                None
            }
        }
    }

    /// The names an action refers to.
    pub(super) fn action_refs(&mut self, a: &Action, line: usize) {
        match a {
            Action::Panel(p) => self.refer("Panel", p, line),
            Action::HelpModule(h) => self.refer("Help module", h, line),
            Action::Call(v) | Action::Msg(v) => self.refer("Variable", v, line),
            _ => {}
        }
    }

    /// `BASETYPE`.
    pub(super) fn basetype(&mut self, el: &Element, attr: &str) -> Option<BaseType> {
        let line = el.line;
        let v = self.upper(el, attr)?;
        match parse_basetype(&v) {
            Ok(b) => Some(b),
            Err(why) => {
                self.diags.push(ids::bad_basetype(line, &v, why));
                None
            }
        }
    }
}

/// Parse `CHAR 10`, `PACKED 7 2`, `DATE`, ...
pub fn parse_basetype(text: &str) -> Result<BaseType, String> {
    let mut parts = text.split_whitespace();
    let kind = parts.next().ok_or_else(|| "empty".to_string())?;
    let nums: Vec<u32> = parts
        .map(|p| p.parse::<u32>().map_err(|_| format!("{p} is not a number")))
        .collect::<Result<_, _>>()?;
    let one = |range: std::ops::RangeInclusive<u32>| -> Result<u32, String> {
        match nums.as_slice() {
            [n] if range.contains(n) => Ok(*n),
            [n] => Err(format!(
                "{n} is outside {} to {}",
                range.start(),
                range.end()
            )),
            _ => Err(format!("{kind} needs one number")),
        }
    };
    let two = || -> Result<(u32, u32), String> {
        match nums.as_slice() {
            [d, p] if (1..=63).contains(d) && p <= d => Ok((*d, *p)),
            [_, _] => Err("digits 1 to 63 and no more decimals than digits".to_string()),
            _ => Err(format!("{kind} needs digits and decimals")),
        }
    };
    let none = |b: BaseType| -> Result<BaseType, String> {
        if nums.is_empty() {
            Ok(b)
        } else {
            Err(format!("{kind} takes no number"))
        }
    };
    match kind {
        "CHAR" => one(1..=32_767).map(BaseType::Char),
        "IGC" => one(1..=32_767).map(BaseType::Igc),
        "BIN" => match one(15..=31)? {
            n @ (15 | 31) => Ok(BaseType::Bin(n)),
            _ => Err("BIN is 15 or 31".to_string()),
        },
        "PACKED" => two().map(|(d, p)| BaseType::Packed(d, p)),
        "ZONED" => two().map(|(d, p)| BaseType::Zoned(d, p)),
        "FLOAT" => match one(4..=8)? {
            n @ (4 | 8) => Ok(BaseType::Float(n)),
            _ => Err("FLOAT is 4 or 8".to_string()),
        },
        "DATE" => none(BaseType::Date),
        "TIME" => none(BaseType::Time),
        "PTR" => none(BaseType::Ptr),
        "OBJNAME" => none(BaseType::ObjName),
        "OBJNAMEQ" => none(BaseType::ObjNameQ),
        "NAME" => none(BaseType::Name),
        "SNAME" => none(BaseType::SName),
        "DEC" => none(BaseType::Dec),
        "ACTION" => none(BaseType::Action),
        "CMDLINE" => none(BaseType::CmdLine),
        other => Err(format!("{other} is not a base type")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basetypes() {
        assert_eq!(parse_basetype("CHAR 10"), Ok(BaseType::Char(10)));
        assert_eq!(parse_basetype("PACKED 7 2"), Ok(BaseType::Packed(7, 2)));
        assert_eq!(parse_basetype("BIN 31"), Ok(BaseType::Bin(31)));
        assert_eq!(parse_basetype("DATE"), Ok(BaseType::Date));
        assert!(parse_basetype("CHAR").is_err());
        assert!(parse_basetype("CHAR 0").is_err());
        assert!(parse_basetype("BIN 16").is_err());
        assert!(parse_basetype("PACKED 2 5").is_err());
        assert!(parse_basetype("DATE 1").is_err());
        assert!(parse_basetype("BLOB").is_err());
        assert!(valid_name("A_1$"));
        assert!(!valid_name("1A"));
        assert!(!valid_name("ABCDEFGHIJK"));
    }
}
