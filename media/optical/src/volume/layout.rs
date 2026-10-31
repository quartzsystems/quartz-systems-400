// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Quartz layout of a writable virtual optical volume: an ISO 9660
//! image whose root holds the volume label [`LABEL_FILE`]; the volume
//! identifier rules; the names save operations give their files.

use crate::error::{Error, Result};

/// The label's name in the root.
pub const LABEL_FILE: &str = "QS400VOL.LBL";
/// The label's first line.
pub const LABEL_MAGIC: &str = "QS400-OPTVOL 1";
/// The label's keys, in the order written.
pub const LABEL_KEYS: [&str; 5] = ["volid", "text", "capacity", "created", "changed"];
/// Longest volume identifier.
pub const MAX_VOLUME_ID: usize = 32;
/// Longest text description.
pub const MAX_TEXT: usize = 50;
/// Longest name of a file or directory on a writable volume (Joliet's).
pub const MAX_NAME: usize = crate::iso::name::MAX_JOLIET;
/// The application identifier of a writable volume's descriptors.
pub const APPLICATION: &str = "QS/400 OPTICAL VOLUME";

/// The label.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Label {
    /// Volume identifier.
    pub volid: String,
    /// Text description.
    pub text: String,
    /// Capacity in bytes (the image may grow to this).
    pub capacity: u64,
    /// Unix time the volume was initialized.
    pub created: u64,
    /// Unix time of the last change.
    pub changed: u64,
}

/// Escape a value for a one-line field: `\\`, `\t`, `\n`, `\r`.
#[must_use]
pub fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '\t' => o.push_str("\\t"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            _ => o.push(c),
        }
    }
    o
}

/// Undo [`escape`].
#[must_use]
pub fn unescape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            o.push(c);
            continue;
        }
        match it.next() {
            Some('t') => o.push('\t'),
            Some('n') => o.push('\n'),
            Some('r') => o.push('\r'),
            Some(other) => o.push(other),
            None => o.push('\\'),
        }
    }
    o
}

impl Label {
    /// The file's text: the magic line, then `key<TAB>value` lines.
    #[must_use]
    pub fn to_text(&self) -> String {
        let vals = [
            self.volid.clone(),
            self.text.clone(),
            self.capacity.to_string(),
            self.created.to_string(),
            self.changed.to_string(),
        ];
        let mut s = format!("{LABEL_MAGIC}\n");
        for (k, v) in LABEL_KEYS.iter().zip(vals) {
            s.push_str(k);
            s.push('\t');
            s.push_str(&escape(&v));
            s.push('\n');
        }
        s
    }

    /// Parse the file. Unknown keys are ignored; a missing identifier is
    /// an error.
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        if lines.next() != Some(LABEL_MAGIC) {
            return Err(Error::bad("not a QS/400 volume label"));
        }
        let mut l = Self::default();
        let num = |v: &str| {
            v.parse::<u64>()
                .map_err(|_| Error::bad("volume label number"))
        };
        for line in lines {
            let Some((k, v)) = line.split_once('\t') else {
                continue;
            };
            let v = unescape(v);
            match k {
                "volid" => l.volid = v,
                "text" => l.text = v,
                "capacity" => l.capacity = num(&v)?,
                "created" => l.created = num(&v)?,
                "changed" => l.changed = num(&v)?,
                _ => {}
            }
        }
        if l.volid.is_empty() {
            return Err(Error::bad("volume label without an identifier"));
        }
        Ok(l)
    }
}

/// A volume identifier as recorded: upper case, 1 to 32 of `A-Z 0-9`
/// and `$ # @ _ . -`, starting with a letter, digit, `$`, `#` or `@`.
pub fn check_volume_id(id: &str) -> Result<String> {
    let up = id.trim().to_ascii_uppercase();
    let ok_char = |c: char| c.is_ascii_uppercase() || c.is_ascii_digit() || "$#@_.-".contains(c);
    let first_ok = up
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || "$#@".contains(c));
    if up.is_empty() || up.len() > MAX_VOLUME_ID || !first_ok || !up.chars().all(ok_char) {
        return Err(Error::BadVolumeId(id.to_string()));
    }
    Ok(up)
}

/// A volume identifier made from another name (an image file name):
/// what is not allowed becomes `_`, cut to 32.
#[must_use]
pub fn volume_id_from(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    let mut id: String = stem
        .chars()
        .map(|c| {
            let u = c.to_ascii_uppercase();
            if u.is_ascii_uppercase() || u.is_ascii_digit() || "$#@_.-".contains(u) {
                u
            } else {
                '_'
            }
        })
        .take(MAX_VOLUME_ID)
        .collect();
    if !id.starts_with(|c: char| c.is_ascii_alphanumeric() || "$#@".contains(c)) {
        id.insert(0, 'V');
        id.truncate(MAX_VOLUME_ID);
    }
    id
}

/// What a save writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveTarget<'a> {
    /// `SAVLIB`, `SAVOBJ`, `SAVCHGOBJ` of one library.
    Library(&'a str),
    /// `SAV` of IFS objects; the number makes the name unique.
    Ifs(u32),
    /// `SAVSYS`, `SAVSECDTA`, `SAVCFG`: the system's own data, by kind.
    System(&'a str),
}

/// The path a save with `OPTFILE(*)` writes: a file in the root named
/// after the library (`/MYLIB`), `/QSAVIFS.nnn` for `SAV`, and
/// `/QSAVSYS/<KIND>` for the system saves.
#[must_use]
pub fn generated_save_path(t: SaveTarget<'_>) -> String {
    match t {
        SaveTarget::Library(lib) => format!("/{}", lib.to_ascii_uppercase()),
        SaveTarget::Ifs(n) => format!("/QSAVIFS.{n:03}"),
        SaveTarget::System(kind) => format!("/QSAVSYS/{}", kind.to_ascii_uppercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_ids() {
        let l = Label {
            volid: "SAVE01".into(),
            text: "tab\there\\ and\nline".into(),
            capacity: 1 << 30,
            created: 5,
            changed: 6,
        };
        assert_eq!(Label::parse(&l.to_text()).unwrap(), l);
        assert!(Label::parse("x").is_err());
        assert_eq!(check_volume_id("vol-1.a").unwrap(), "VOL-1.A");
        assert!(check_volume_id("_x").is_err());
        assert!(check_volume_id(&"A".repeat(33)).is_err());
        assert!(check_volume_id("a b").is_err());
        assert_eq!(volume_id_from("my image.iso"), "MY_IMAGE");
        assert_eq!(volume_id_from("_x.iso"), "V_X");
        assert_eq!(generated_save_path(SaveTarget::Library("qgpl")), "/QGPL");
        assert_eq!(generated_save_path(SaveTarget::Ifs(7)), "/QSAVIFS.007");
        assert_eq!(
            generated_save_path(SaveTarget::System("savsys")),
            "/QSAVSYS/SAVSYS"
        );
    }
}
