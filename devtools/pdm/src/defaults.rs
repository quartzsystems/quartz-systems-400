// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The session defaults F18 changes: where compiled objects go, batch
//! or interactive, the option file. Kept per user as `KEY=VALUE`
//! lines (the driver stores them in a source member named for the
//! user).

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::PdmError;

/// One user's PDM defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the panel's Y/N fields
pub struct Defaults {
    /// Where options 14 and 15 put what they create (`&O`); `*CURLIB`
    /// and `*SRCLIB` resolve when used.
    pub object_library: String,
    /// Replace an existing object (`&R`).
    pub replace: bool,
    /// Compile with `SBMJOB` (`&E` is the run flag, not this).
    pub batch_compile: bool,
    /// Run option 16 in batch (`&E`).
    pub batch_run: bool,
    /// The batch job's description (`&H`, `&J`).
    pub jobd: String,
    /// Its library.
    pub jobd_lib: String,
    /// Ask for type and text when creating or copying a member.
    pub change_type_text: bool,
    /// The user-defined options file.
    pub option_file: String,
    /// Its library.
    pub option_lib: String,
    /// Its member.
    pub option_member: String,
    /// SEU starts in full-screen mode.
    pub full_screen: bool,
    /// Log the commands options run to the job log.
    pub log_commands: bool,
    /// Enter with nothing typed leaves the list.
    pub exit_on_enter: bool,
    /// Keep the defaults for the next session.
    pub save_session: bool,
    /// What options 9 and 10 save and restore: `1` the entry, `2` all
    /// entries selected.
    pub save_restore: String,
    /// The values `*PRV` stands for, by name (`LIB`, `OBJLIB`, ...).
    pub prev: BTreeMap<String, String>,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            object_library: "*CURLIB".into(),
            replace: false,
            batch_compile: false,
            batch_run: false,
            jobd: "*USRPRF".into(),
            jobd_lib: String::new(),
            change_type_text: true,
            option_file: "QAUOOPT".into(),
            option_lib: "QGPL".into(),
            option_member: "QAUOOPT".into(),
            full_screen: false,
            log_commands: true,
            exit_on_enter: false,
            save_session: true,
            save_restore: "1".into(),
            prev: BTreeMap::new(),
        }
    }
}

/// `Y`/`N` as a flag.
fn flag(field: &'static str, v: &str) -> Result<bool, PdmError> {
    match v.trim().to_ascii_uppercase().as_str() {
        "Y" | "*YES" => Ok(true),
        "N" | "*NO" => Ok(false),
        _ => Err(PdmError::BadValue {
            field,
            value: v.trim().to_string(),
        }),
    }
}

/// A flag as `Y`/`N`.
#[must_use]
pub fn yn(b: bool) -> &'static str {
    if b { "Y" } else { "N" }
}

/// A flag as `*YES`/`*NO`, as commands take it.
#[must_use]
pub fn yes_no(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

/// An object or library name, or one of the special values given.
fn name(field: &'static str, v: &str, special: &[&str]) -> Result<String, PdmError> {
    let v = v.trim().to_ascii_uppercase();
    let ok = special.contains(&v.as_str())
        || (!v.is_empty()
            && v.len() <= 10
            && v.chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || "$#@".contains(c))
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric() || "$#@_.".contains(c)));
    if ok {
        Ok(v)
    } else {
        Err(PdmError::BadValue { field, value: v })
    }
}

/// The fields as the panel and the stored form name them.
pub const FIELDS: &[&str] = &[
    "OBJLIB", "REPLACE", "BCHCMP", "BCHRUN", "JOBD", "JOBDLIB", "CHGTXT", "OPTFILE", "OPTLIB",
    "OPTMBR", "FULLSCR", "LOGCMD", "EXITENT", "SAVSES", "SAVRST",
];

impl Defaults {
    /// A field's value as the panel shows it.
    #[must_use]
    pub fn get(&self, field: &str) -> String {
        match field {
            "OBJLIB" => self.object_library.clone(),
            "REPLACE" => yn(self.replace).into(),
            "BCHCMP" => yn(self.batch_compile).into(),
            "BCHRUN" => yn(self.batch_run).into(),
            "JOBD" => self.jobd.clone(),
            "JOBDLIB" => self.jobd_lib.clone(),
            "CHGTXT" => yn(self.change_type_text).into(),
            "OPTFILE" => self.option_file.clone(),
            "OPTLIB" => self.option_lib.clone(),
            "OPTMBR" => self.option_member.clone(),
            "FULLSCR" => yn(self.full_screen).into(),
            "LOGCMD" => yn(self.log_commands).into(),
            "EXITENT" => yn(self.exit_on_enter).into(),
            "SAVSES" => yn(self.save_session).into(),
            "SAVRST" => self.save_restore.clone(),
            _ => String::new(),
        }
    }

    /// Set a field from what was typed, checked.
    ///
    /// # Errors
    ///
    /// A value outside the field's set.
    pub fn set(&mut self, field: &str, v: &str) -> Result<(), PdmError> {
        match field {
            "OBJLIB" => self.object_library = name("Object library", v, &["*CURLIB", "*SRCLIB"])?,
            "REPLACE" => self.replace = flag("Replace object", v)?,
            "BCHCMP" => self.batch_compile = flag("Compile in batch", v)?,
            "BCHRUN" => self.batch_run = flag("Run in batch", v)?,
            "JOBD" => self.jobd = name("Job description", v, &["*USRPRF"])?,
            "JOBDLIB" => {
                self.jobd_lib = if v.trim().is_empty() {
                    String::new()
                } else {
                    name("Library", v, &["*LIBL", "*CURLIB"])?
                };
            }
            "CHGTXT" => self.change_type_text = flag("Change type and text", v)?,
            "OPTFILE" => self.option_file = name("Option file", v, &[])?,
            "OPTLIB" => self.option_lib = name("Library", v, &["*LIBL", "*CURLIB"])?,
            "OPTMBR" => self.option_member = name("Member", v, &[])?,
            "FULLSCR" => self.full_screen = flag("Full screen mode", v)?,
            "LOGCMD" => self.log_commands = flag("Log option commands", v)?,
            "EXITENT" => self.exit_on_enter = flag("Exit lists on Enter", v)?,
            "SAVSES" => self.save_session = flag("Save session defaults", v)?,
            "SAVRST" => {
                let t = v.trim();
                if t != "1" && t != "2" {
                    return Err(PdmError::BadValue {
                        field: "Save/Restore option",
                        value: t.to_string(),
                    });
                }
                self.save_restore = t.to_string();
            }
            _ => {}
        }
        Ok(())
    }

    /// The stored form: one `KEY=VALUE` line per field.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut s = FIELDS.iter().fold(String::new(), |mut s, f| {
            let _ = writeln!(s, "{f}={}", self.get(f));
            s
        });
        for (k, v) in &self.prev {
            let _ = writeln!(s, "PRV.{k}={v}");
        }
        s
    }

    /// The stored form read back; fields missing or bad keep their
    /// defaults.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut d = Self::default();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim().strip_prefix("PRV.") {
                    Some(name) => {
                        d.prev.insert(name.to_string(), v.trim().to_string());
                    }
                    None => {
                        let _ = d.set(k.trim(), v);
                    }
                }
            }
        }
        d
    }

    /// The value `*PRV` stands for, `dft` when none was kept.
    #[must_use]
    pub fn previous(&self, name: &str, dft: &str) -> String {
        self.prev
            .get(name)
            .filter(|v| !v.is_empty())
            .cloned()
            .unwrap_or_else(|| dft.to_string())
    }

    /// Keep a value for `*PRV`.
    pub fn remember(&mut self, name: &str, value: &str) {
        self.prev.insert(name.to_string(), value.trim().to_string());
    }

    /// The object library for an item in `source_lib`, with the job's
    /// current library: `*SRCLIB` is the source's, `*CURLIB` the job's
    /// (`QGPL` when it has none).
    #[must_use]
    pub fn object_lib(&self, source_lib: &str, curlib: Option<&str>) -> String {
        match self.object_library.as_str() {
            "*SRCLIB" => source_lib.to_string(),
            "*CURLIB" => curlib.unwrap_or("QGPL").to_string(),
            other => other.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut d = Defaults::default();
        d.set("OBJLIB", "mylib").unwrap();
        d.set("REPLACE", "y").unwrap();
        d.set("BCHCMP", "N").unwrap();
        d.remember("LIB", "QGPL");
        let back = Defaults::parse(&d.to_text());
        assert_eq!(back, d);
        assert_eq!(back.object_lib("SRC", Some("CUR")), "MYLIB");
        assert_eq!(back.previous("LIB", "*LIBL"), "QGPL");
        assert_eq!(back.previous("FILE", "QCLSRC"), "QCLSRC");
    }

    #[test]
    fn checks() {
        let mut d = Defaults::default();
        assert!(d.set("REPLACE", "X").is_err());
        assert!(d.set("OBJLIB", "1BAD").is_err());
        assert!(d.set("OBJLIB", "*SRCLIB").is_ok());
        assert_eq!(d.object_lib("SRC", None), "SRC");
        d.set("OBJLIB", "*CURLIB").unwrap();
        assert_eq!(d.object_lib("SRC", None), "QGPL");
        let p = Defaults::parse("REPLACE=Q\nJUNK\nBCHRUN=Y\n");
        assert!(!p.replace);
        assert!(p.batch_run);
    }
}
