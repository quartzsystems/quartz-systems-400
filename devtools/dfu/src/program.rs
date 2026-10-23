// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A DFU program: the choices of the creation displays, kept as the
//! payload of a `*PGM` object of attribute `DFU`. The payload is text,
//! one `KEYWORD value` line each, so it reads back across versions.

use crate::DfuError;

/// The first line of every payload.
const MAGIC: &str = "QS400-DFU 1";

/// Display format (`Display format` on the general information).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    /// One field per line, one column.
    Single,
    /// Fields side by side as they fit.
    #[default]
    Multiple,
    /// As many as fit, names shortened.
    Maximum,
    /// Several records, one per row.
    Row,
}

impl Format {
    /// From the digit typed.
    #[must_use]
    pub fn from_code(c: &str) -> Option<Self> {
        Some(match c.trim() {
            "1" => Self::Single,
            "2" => Self::Multiple,
            "3" => Self::Maximum,
            "4" => Self::Row,
            _ => return None,
        })
    }

    /// The digit.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Single => "1",
            Self::Multiple => "2",
            Self::Maximum => "3",
            Self::Row => "4",
        }
    }
}

/// Everything a DFU program holds.
#[allow(clippy::struct_excessive_bools)] // the display's Y/N choices
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// The data file's library as named (`*LIBL` allowed).
    pub file_lib: String,
    /// The data file.
    pub file: String,
    /// Job title.
    pub title: String,
    /// Display format.
    pub format: Format,
    /// Audit report.
    pub audit: bool,
    /// S/36 style.
    pub s36: bool,
    /// Suppress errors.
    pub suppress: bool,
    /// Edit numerics.
    pub edit: bool,
    /// Allow updates on roll.
    pub roll_update: bool,
    /// Record numbers generated.
    pub recnbr: bool,
    /// Record numbers stored in a field.
    pub recnbr_field: bool,
    /// The record number's heading.
    pub recnbr_heading: String,
    /// Sequential processing (else direct), for a nonindexed file.
    pub sequential: bool,
    /// Generate keys, for an indexed file.
    pub gen_keys: bool,
    /// Record formats used; empty for all.
    pub formats: Vec<String>,
    /// Fields shown, by format; a format missing shows every field.
    pub fields: Vec<(String, Vec<String>)>,
}

impl Program {
    /// The defaults of a new program over a file.
    #[must_use]
    pub fn new(name: &str, file_lib: &str, file: &str) -> Self {
        Self {
            file_lib: file_lib.to_string(),
            file: file.to_string(),
            title: name.to_string(),
            format: Format::Multiple,
            audit: true,
            s36: false,
            suppress: false,
            edit: false,
            roll_update: true,
            recnbr: false,
            recnbr_field: false,
            recnbr_heading: "*RECNBR".into(),
            sequential: true,
            gen_keys: false,
            formats: Vec::new(),
            fields: Vec::new(),
        }
    }

    /// The fields to show of a format, from its fields in order.
    #[must_use]
    pub fn shown<'a>(&self, format: &str, all: &'a [String]) -> Vec<&'a String> {
        match self.fields.iter().find(|(f, _)| f == format) {
            Some((_, pick)) => all.iter().filter(|n| pick.contains(n)).collect(),
            None => all.iter().collect(),
        }
    }

    /// The payload.
    #[must_use]
    pub fn encode(&self) -> String {
        let yn = |b: bool| if b { "Y" } else { "N" };
        let mut out = vec![
            MAGIC.to_string(),
            format!("FILE {}/{}", self.file_lib, self.file),
            format!("TITLE {}", self.title),
            format!("DSPFMT {}", self.format.code()),
            format!("AUDIT {}", yn(self.audit)),
            format!("S36 {}", yn(self.s36)),
            format!("SUPPRESS {}", yn(self.suppress)),
            format!("EDIT {}", yn(self.edit)),
            format!("ROLLUPD {}", yn(self.roll_update)),
            format!("RECNBR {}", yn(self.recnbr)),
            format!("RECNBRFLD {}", yn(self.recnbr_field)),
            format!("RECNBRHDG {}", self.recnbr_heading),
            format!("SEQUENTIAL {}", yn(self.sequential)),
            format!("GENKEYS {}", yn(self.gen_keys)),
        ];
        for f in &self.formats {
            out.push(format!("FORMAT {f}"));
        }
        for (f, names) in &self.fields {
            out.push(format!("FIELDS {f} {}", names.join(" ")));
        }
        out.join("\n")
    }

    /// From a payload.
    ///
    /// # Errors
    /// A payload that is not a DFU program's.
    pub fn decode(text: &str) -> Result<Self, DfuError> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(MAGIC) {
            return Err(DfuError::NotDfu);
        }
        let mut prog = Self::new("", "*LIBL", "");
        for line in lines {
            let (k, v) = line.split_once(' ').unwrap_or((line, ""));
            let y = v.trim() == "Y";
            match k {
                "FILE" => {
                    let (lib, f) = v.split_once('/').unwrap_or(("*LIBL", v));
                    prog.file_lib = lib.to_string();
                    prog.file = f.to_string();
                }
                "TITLE" => prog.title = v.to_string(),
                "DSPFMT" => prog.format = Format::from_code(v).unwrap_or_default(),
                "AUDIT" => prog.audit = y,
                "S36" => prog.s36 = y,
                "SUPPRESS" => prog.suppress = y,
                "EDIT" => prog.edit = y,
                "ROLLUPD" => prog.roll_update = y,
                "RECNBR" => prog.recnbr = y,
                "RECNBRFLD" => prog.recnbr_field = y,
                "RECNBRHDG" => prog.recnbr_heading = v.to_string(),
                "SEQUENTIAL" => prog.sequential = y,
                "GENKEYS" => prog.gen_keys = y,
                "FORMAT" => prog.formats.push(v.to_string()),
                "FIELDS" => {
                    let mut w = v.split_whitespace();
                    if let Some(f) = w.next() {
                        prog.fields
                            .push((f.to_string(), w.map(str::to_string).collect()));
                    }
                }
                _ => {}
            }
        }
        if prog.file.is_empty() {
            return Err(DfuError::NotDfu);
        }
        Ok(prog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut p = Program::new("CUSTDFU", "QGPL", "CUST");
        p.format = Format::Single;
        p.formats.push("CUSTR".into());
        p.fields.push(("CUSTR".into(), vec!["ID".into(), "NAME".into()]));
        let q = Program::decode(&p.encode()).unwrap();
        assert_eq!(p, q);
        let all = ["ID".to_string(), "CITY".into(), "NAME".into()];
        assert_eq!(q.shown("CUSTR", &all), vec![&all[0], &all[2]]);
        assert_eq!(q.shown("OTHER", &all).len(), 3);
        assert_eq!(Program::decode("garbage"), Err(DfuError::NotDfu));
    }
}
