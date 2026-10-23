// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Exit display (`F3`): save or not, where to, resequencing,
//! printing, back to editing, the member list.

/// The Exit display's values as shown and typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitForm {
    /// Change/create member.
    pub change: String,
    /// Member.
    pub member: String,
    /// File.
    pub file: String,
    /// Library.
    pub lib: String,
    /// Text.
    pub text: String,
    /// Resequence member.
    pub reseq: String,
    /// Start.
    pub start: String,
    /// Increment.
    pub increment: String,
    /// Print member.
    pub print: String,
    /// Return to editing.
    pub ret: String,
    /// Go to member list.
    pub list: String,
}

/// What the operator chose.
#[allow(clippy::struct_excessive_bools)] // the display's Y/N choices
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitChoice {
    /// Write the member (to `lib/file(member)`).
    pub save: bool,
    /// Member.
    pub member: String,
    /// File.
    pub file: String,
    /// Library.
    pub lib: String,
    /// Member text.
    pub text: String,
    /// Renumber first: start and increment in hundredths.
    pub reseq: Option<(u32, u32)>,
    /// Print the member.
    pub print: bool,
    /// Go back to editing.
    pub ret: bool,
    /// Show the member list (PDM's) after.
    pub list: bool,
}

/// A refused value: the field's index (0 = Change/create member) and
/// why.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{text}")]
pub struct ExitError {
    /// The field.
    pub field: usize,
    /// What is wrong.
    pub text: String,
}

/// `nnnn.nn`-style numbers of the start and increment fields.
fn number(field: usize, s: &str, max_whole: u32, what: &str) -> Result<u32, ExitError> {
    let bad = || ExitError {
        field,
        text: format!("Type the {what} as a number with up to two decimal places."),
    };
    let t = s.trim();
    let (w, f) = t.split_once('.').unwrap_or((t, ""));
    if f.len() > 2 || !w.chars().chain(f.chars()).all(|c| c.is_ascii_digit()) || t.is_empty() {
        return Err(bad());
    }
    let whole: u32 = if w.is_empty() { 0 } else { w.parse().map_err(|_| bad())? };
    let mut frac: u32 = if f.is_empty() { 0 } else { f.parse().map_err(|_| bad())? };
    if f.len() == 1 {
        frac *= 10;
    }
    if whole > max_whole {
        return Err(bad());
    }
    let v = whole * 100 + frac;
    if v == 0 {
        return Err(bad());
    }
    Ok(v)
}

fn yes_no(field: usize, s: &str) -> Result<bool, ExitError> {
    match s.trim().to_ascii_uppercase().as_str() {
        "Y" => Ok(true),
        "N" => Ok(false),
        _ => Err(ExitError {
            field,
            text: "Type Y or N.".into(),
        }),
    }
}

impl ExitForm {
    /// The display for a member: save when it changed (or is new),
    /// resequence by the session default, back to editing when the
    /// member has syntax errors.
    #[must_use]
    pub fn new(member: &str, file: &str, lib: &str, text: &str, changed: bool, reseq: bool, errors: bool) -> Self {
        let yn = |b: bool| if b { "Y" } else { "N" }.to_string();
        Self {
            change: yn(changed),
            member: member.into(),
            file: file.into(),
            lib: lib.into(),
            text: text.into(),
            reseq: yn(reseq),
            start: "0001.00".into(),
            increment: "01.00".into(),
            print: "N".into(),
            ret: yn(errors),
            list: "N".into(),
        }
    }

    /// Check the values.
    pub fn choice(&self) -> Result<ExitChoice, ExitError> {
        let save = yes_no(0, &self.change)?;
        let name = |field: usize, s: &str, what: &str| -> Result<String, ExitError> {
            let n = s.trim().to_ascii_uppercase();
            let ok = !n.is_empty()
                && n.len() <= 10
                && n.chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || "$#@".contains(c))
                && n.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "$#@_.".contains(c));
            if ok {
                Ok(n)
            } else {
                Err(ExitError {
                    field,
                    text: format!("{what} name is not valid."),
                })
            }
        };
        let member = name(1, &self.member, "Member")?;
        let file = name(2, &self.file, "File")?;
        let lib = name(3, &self.lib, "Library")?;
        let reseq = if yes_no(5, &self.reseq)? {
            Some((
                number(6, &self.start, 9999, "start")?,
                number(7, &self.increment, 99, "increment")?,
            ))
        } else {
            None
        };
        Ok(ExitChoice {
            save,
            member,
            file,
            lib,
            text: self.text.trim_end().to_string(),
            reseq,
            print: yes_no(8, &self.print)?,
            ret: yes_no(9, &self.ret)?,
            list: yes_no(10, &self.list)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_change() {
        let f = ExitForm::new("QDFUZZIN", "QDDSSRC", "LIB", "", false, true, false);
        assert_eq!(f.change, "N");
        let c = f.choice().unwrap();
        assert!(!c.save);
        assert_eq!(c.reseq, Some((100, 100)));
        assert!(ExitForm::new("A", "F", "L", "", true, true, false).choice().unwrap().save);
    }

    #[test]
    fn refusals() {
        let mut f = ExitForm::new("M", "F", "L", "", true, true, false);
        f.change = "x".into();
        assert_eq!(f.choice().unwrap_err().field, 0);
        let mut f = ExitForm::new("M", "F", "L", "", true, true, false);
        f.member = "1BAD".into();
        assert_eq!(f.choice().unwrap_err().field, 1);
        let mut f = ExitForm::new("M", "F", "L", "", true, true, false);
        f.increment = "100".into();
        assert_eq!(f.choice().unwrap_err().field, 7);
        f.reseq = "N".into();
        assert_eq!(f.choice().unwrap().reseq, None);
    }

    #[test]
    fn numbers() {
        let mut f = ExitForm::new("M", "F", "L", "", true, true, false);
        f.start = "10".into();
        f.increment = ".5".into();
        assert_eq!(f.choice().unwrap().reseq, Some((1000, 50)));
    }
}
