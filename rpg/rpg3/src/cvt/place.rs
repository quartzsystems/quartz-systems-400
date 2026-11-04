// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Cutting RPG III records and building ILE RPG ones; the renamed
//! operation codes and the array-element notation.

use rpgspecs::Form;

/// A field of an RPG III record, trimmed.
pub(super) fn cut3(form: &str, r: &str, name: &str) -> String {
    rpgspecs::rpg3_form(form).cut(r, name).trim().to_string()
}

/// A field of an RPG III record, untrimmed.
pub(super) fn raw3(form: &str, r: &str, name: &str) -> String {
    rpgspecs::rpg3_form(form).cut(r, name)
}

/// An ILE RPG record being built.
pub(super) struct Rec {
    form: &'static Form,
    chars: Vec<char>,
}

impl Rec {
    /// A record of a form, its letter in column 6 and columns 1-5 from
    /// the RPG III record (the sequence area).
    pub(super) fn new(form: &str, from: &str) -> Self {
        let form = rpgspecs::form(form).unwrap_or_else(|| panic!("no form {form}"));
        let mut chars: Vec<char> = from.chars().take(5).collect();
        chars.resize(5, ' ');
        chars.push(form.letter.chars().next().unwrap_or(' '));
        Self { form, chars }
    }

    /// Left-adjusted text.
    pub(super) fn put(&mut self, name: &str, text: &str) -> &mut Self {
        if !text.is_empty() {
            self.form.put(&mut self.chars, name, text, false);
        }
        self
    }

    /// Right-adjusted text (a numeric entry).
    pub(super) fn right(&mut self, name: &str, text: &str) -> &mut Self {
        if !text.is_empty() {
            self.form.put(&mut self.chars, name, text, true);
        }
        self
    }

    /// Text at a column.
    pub(super) fn at(&mut self, col: usize, text: &str) -> &mut Self {
        for (i, ch) in text.chars().enumerate() {
            let at = col - 1 + i;
            if self.chars.len() <= at {
                self.chars.resize(at + 1, ' ');
            }
            self.chars[at] = ch;
        }
        self
    }

    /// The record, trimmed.
    pub(super) fn text(&self) -> String {
        self.chars.iter().collect::<String>().trim_end().to_string()
    }
}

/// Every RPG III operation that ILE RPG spells otherwise.
const RENAMED: &[(&str, &str)] = &[
    ("BITOF", "BITOFF"),
    ("CHEKR", "CHECKR"),
    ("COMIT", "COMMIT"),
    ("DEFN", "DEFINE"),
    ("DELET", "DELETE"),
    ("EXCPT", "EXCEPT"),
    ("LOKUP", "LOOKUP"),
    ("OCUR", "OCCUR"),
    ("REDPE", "READPE"),
    ("RETRN", "RETURN"),
    ("SELEC", "SELECT"),
    ("SETOF", "SETOFF"),
    ("UNLCK", "UNLOCK"),
    ("UPDAT", "UPDATE"),
];

/// The ILE RPG spelling of an RPG III operation (`WHEQ` is `WHENEQ`).
#[must_use]
pub fn opcode(op: &str) -> String {
    let up = op.trim().to_ascii_uppercase();
    if let Some((_, new)) = RENAMED.iter().find(|(old, _)| *old == up) {
        return (*new).to_string();
    }
    if let Some(cmp) = up.strip_prefix("WH")
        && cmp.len() == 2
    {
        return format!("WHEN{cmp}");
    }
    up
}

/// A factor or result field in ILE RPG notation: an array element
/// `ARR,I` is `ARR(I)`, `*IN,nn` is `*IN(nn)`; literals stay as they
/// are.
#[must_use]
pub fn factor(f: &str) -> String {
    let f = f.trim();
    if f.starts_with('\'') || f.starts_with("X'") || f.starts_with("x'") {
        return f.to_string();
    }
    // A colon form (`B:1`, `'LO':'01'`) converts each part.
    if let Some((a, b)) = split_colon(f) {
        return format!("{}:{}", factor(a), factor(b));
    }
    match f.split_once(',') {
        Some((name, idx)) if !name.is_empty() && !idx.is_empty() => format!("{name}({idx})"),
        _ => f.to_string(),
    }
}

/// `a:b` outside literals.
fn split_colon(f: &str) -> Option<(&str, &str)> {
    let mut quote = false;
    for (i, ch) in f.char_indices() {
        match ch {
            '\'' => quote = !quote,
            ':' if !quote => return Some((&f[..i], &f[i + 1..])),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_and_factors() {
        assert_eq!(opcode("LOKUP"), "LOOKUP");
        assert_eq!(opcode("WHGT"), "WHENGT");
        assert_eq!(opcode("Z-ADD"), "Z-ADD");
        assert_eq!(factor("ARR,I"), "ARR(I)");
        assert_eq!(factor("*IN,10"), "*IN(10)");
        assert_eq!(factor("'A,B'"), "'A,B'");
        assert_eq!(factor("A:2"), "A:2");
        assert_eq!(factor("RUN,K"), "RUN(K)");
    }
}
