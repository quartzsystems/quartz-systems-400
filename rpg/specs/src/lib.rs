// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The column layouts of the ILE RPG specifications, as data
//! (`specs/rpgle.toml`). The front end cuts fixed-form records into
//! fields with them; SEU draws its format lines and prompts from them.
//! Columns are 1-based and inclusive, as IBM numbers them.

#![deny(missing_docs)]

use std::sync::OnceLock;

use serde::Deserialize;

/// One field of a form.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Field {
    /// The field's name in the table.
    pub name: String,
    /// First column.
    pub from: usize,
    /// Last column.
    pub to: usize,
    /// What SEU's format line shows over it; empty for a field that
    /// overlaps others and has no ruler of its own.
    pub ruler: String,
}

/// One form: a specification type, or one of its line kinds.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Form {
    /// `H`, `F`, `D`, `I`, `IX`, `IR`, `IF`, `C`, `CX`, `O`, `OF`, `P`.
    pub id: String,
    /// The letter in column 6.
    pub letter: String,
    /// What the form describes.
    pub title: String,
    /// Its fields, in column order.
    pub fields: Vec<Field>,
}

#[derive(Deserialize)]
struct Table {
    form: Vec<Form>,
}

/// Every form.
///
/// # Panics
///
/// When the embedded table does not parse, which a test rules out.
pub fn forms() -> &'static [Form] {
    static FORMS: OnceLock<Vec<Form>> = OnceLock::new();
    FORMS.get_or_init(|| {
        let t: Table = toml::from_str(include_str!("../specs/rpgle.toml")).expect("the spec table");
        t.form
    })
}

/// A form by identifier.
#[must_use]
pub fn form(id: &str) -> Option<&'static Form> {
    forms().iter().find(|f| f.id == id)
}

impl Form {
    /// A field by name.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// The text of a field in a record, blank padded to its width.
    /// A field past the end of the record is blanks.
    #[must_use]
    pub fn cut(&self, record: &str, name: &str) -> String {
        self.field(name).map_or_else(String::new, |f| cut(record, f.from, f.to))
    }
}

/// Columns `from..=to` of `record`, blank padded.
#[must_use]
pub fn cut(record: &str, from: usize, to: usize) -> String {
    let mut out = String::with_capacity(to + 1 - from);
    let mut chars = record.chars().skip(from.saturating_sub(1));
    for _ in from..=to {
        out.push(chars.next().unwrap_or(' '));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_parses_and_fields_fit() {
        assert!(forms().len() >= 12);
        for f in forms() {
            for x in &f.fields {
                assert!(x.from >= 6 && x.from <= x.to && x.to <= 100, "{} {}", f.id, x.name);
            }
        }
        let c = form("C").unwrap();
        let mut r = vec![' '; 80];
        for (col, text) in [(6, "C"), (12, "KEY"), (26, "CHAIN"), (36, "CUSTFILE"), (73, "50")] {
            for (i, ch) in text.chars().enumerate() {
                r[col - 1 + i] = ch;
            }
        }
        let rec: String = r.into_iter().collect();
        let rec = rec.as_str();
        assert_eq!(c.cut(rec, "factor1").trim(), "KEY");
        assert_eq!(c.cut(rec, "opcode").trim(), "CHAIN");
        assert_eq!(c.cut(rec, "factor2").trim(), "CUSTFILE");
        assert_eq!(c.cut(rec, "hi").trim(), "");
        assert_eq!(c.cut(rec, "lo"), "50");
    }
}
