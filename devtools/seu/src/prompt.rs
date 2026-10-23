// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fixed-form prompts (`F4`, `P`, `IP`): a record cut into the
//! fields of its format's layout, and the fields put back. CL and
//! command definitions are prompted by the command prompter instead.

use crate::srctype::{FREE, Layout, layout};

/// A prompt over one record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    /// The format (`DP`, `C`, `**`, ...).
    pub format: String,
    /// Its layout.
    pub layout: Layout,
    /// Each field's text, as the record holds it (blanks kept).
    pub values: Vec<String>,
    /// The record's width.
    pub width: usize,
}

/// The characters of `data` in source columns `from..=to`, blank padded.
fn cut(data: &str, from: usize, to: usize) -> String {
    let chars: Vec<char> = data.chars().collect();
    (from..=to)
        .map(|c| chars.get(c - 1).copied().unwrap_or(' '))
        .collect()
}

impl Prompt {
    /// The prompt of a record in a format.
    #[must_use]
    pub fn new(format: &str, data: &str, width: usize) -> Self {
        let layout = layout(format);
        let values = layout
            .fields
            .iter()
            .map(|f| {
                let to = if layout == FREE { width } else { f.to };
                cut(data, f.from, to.min(width.max(f.from)))
            })
            .collect();
        Self {
            format: format.to_string(),
            layout,
            values,
            width,
        }
    }

    /// The screen length of field `i`: its columns, cut to the record.
    #[must_use]
    pub fn field_len(&self, i: usize) -> usize {
        let f = self.layout.fields[i];
        let to = if self.layout == FREE { self.width } else { f.to };
        if f.from > self.width {
            return 0;
        }
        to.min(self.width) - f.from + 1
    }

    /// The record built from the fields over the original record: each
    /// field's columns replaced, the columns no field holds kept, the
    /// specification letter put in column 6.
    #[must_use]
    pub fn record(&self, original: &str, fields: &[String]) -> String {
        let mut chars: Vec<char> = original.chars().collect();
        chars.resize(self.width, ' ');
        if let Some(letter) = crate::srctype::spec_letter(&self.format)
            && self.width >= 6
        {
            chars[5] = letter;
        }
        for (i, f) in self.layout.fields.iter().enumerate() {
            let len = self.field_len(i);
            if len == 0 {
                continue;
            }
            let v: Vec<char> = fields.get(i).map(|s| s.chars().collect()).unwrap_or_default();
            for k in 0..len {
                chars[f.from - 1 + k] = v.get(k).copied().unwrap_or(' ');
            }
        }
        let s: String = chars.into_iter().collect();
        s.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dds_fields() {
        let p = Prompt::new("DP", "     A            OP             2A", 80);
        assert_eq!(p.values[3].trim(), "OP");
        assert_eq!(p.values[5], "    2");
        assert_eq!(p.values[6], "A");
        let mut v = p.values.clone();
        v[3] = "NAME".into();
        assert_eq!(p.record("     A            OP             2A", &v), "     A            NAME           2A");
    }

    #[test]
    fn spec_letter_and_width() {
        let p = Prompt::new("C", "", 80);
        assert_eq!(p.field_len(11), 0);
        let mut v = p.values.clone();
        v[3] = "EVAL".into();
        let r = p.record("", &v);
        assert_eq!(r.chars().nth(5), Some('C'));
        assert_eq!(&r[25..29], "EVAL");
    }

    #[test]
    fn free_format() {
        let p = Prompt::new("**", "hello", 80);
        assert_eq!(p.values[0].trim_end(), "hello");
        assert_eq!(p.field_len(0), 80);
        assert_eq!(p.record("hello", &["bye".into()]), "bye");
    }
}
