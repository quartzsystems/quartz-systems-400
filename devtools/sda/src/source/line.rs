// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One DDS source line and its columns (1-based, as the DDS manual
//! counts them): 6 the form type, 7 and/or or `*` for a comment, 8-16
//! the conditions, 17 the name type, 19-28 the name, 29 reference,
//! 30-34 length, 35 data type, 36-37 decimals, 38 usage, 39-41 row,
//! 42-44 column, 45-80 the keywords.

/// The length of a DDS source line.
pub const WIDTH: usize = 80;

/// A source line as a fixed 80-column buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line(Vec<char>);

impl Line {
    /// A line from text, blank padded or cut to 80 columns.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut c: Vec<char> = text.chars().take(WIDTH).collect();
        c.resize(WIDTH, ' ');
        Self(c)
    }

    /// A blank `A` specification.
    #[must_use]
    pub fn spec() -> Self {
        let mut l = Self::new("");
        l.set(6, 6, "A");
        l
    }

    /// Columns `from..=to`.
    #[must_use]
    pub fn get(&self, from: usize, to: usize) -> String {
        self.0[from - 1..to].iter().collect()
    }

    /// Columns `from..=to` trimmed.
    #[must_use]
    pub fn word(&self, from: usize, to: usize) -> String {
        self.get(from, to).trim().to_string()
    }

    /// One column.
    #[must_use]
    pub fn ch(&self, col: usize) -> char {
        self.0[col - 1]
    }

    /// Columns `from..=to` set to `text`, left aligned and blank padded.
    pub fn set(&mut self, from: usize, to: usize, text: &str) {
        let mut chars = text.chars();
        for c in from..=to {
            self.0[c - 1] = chars.next().unwrap_or(' ');
        }
    }

    /// Columns `from..=to` set to `text`, right aligned.
    pub fn set_right(&mut self, from: usize, to: usize, text: &str) {
        let n = text.chars().count().min(to + 1 - from);
        self.set(from, to, "");
        self.set(to + 1 - n, to, text);
    }

    /// Whether `from..=to` is blank.
    #[must_use]
    pub fn blank(&self, from: usize, to: usize) -> bool {
        self.0[from - 1..to].iter().all(|c| *c == ' ')
    }

    /// A comment (`*` in column 7).
    #[must_use]
    pub fn is_comment(&self) -> bool {
        self.ch(7) == '*'
    }

    /// The name type in column 17.
    #[must_use]
    pub fn name_type(&self) -> char {
        self.ch(17)
    }

    /// The name in columns 19-28.
    #[must_use]
    pub fn name(&self) -> String {
        self.word(19, 28)
    }

    /// The row and column words (39-41, 42-44).
    #[must_use]
    pub fn location(&self) -> (String, String) {
        (self.word(39, 41), self.word(42, 44))
    }

    /// The keyword area, columns 45-80, without trailing blanks.
    #[must_use]
    pub fn functions(&self) -> String {
        self.get(45, 80).trim_end().to_string()
    }

    /// The conditioning columns 7-16 without trailing blanks.
    #[must_use]
    pub fn conditioning(&self) -> String {
        self.get(7, 16).trim_end().to_string()
    }

    /// The text without trailing blanks.
    #[must_use]
    pub fn text(&self) -> String {
        self.0.iter().collect::<String>().trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns() {
        let mut l = Line::spec();
        l.set(19, 28, "FLD001");
        l.set_right(30, 34, "10");
        l.set(38, 38, "I");
        l.set_right(39, 41, "3");
        l.set_right(42, 44, "21");
        assert_eq!(
            l.text(),
            format!("{}A{}FLD001{}10   I  3 21", " ".repeat(5), " ".repeat(12), " ".repeat(8))
        );
        assert_eq!(l.name(), "FLD001");
        assert_eq!(l.location(), ("3".into(), "21".into()));
        assert!(!l.is_comment());
    }
}
