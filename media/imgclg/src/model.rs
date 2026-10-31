// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*IMGCLG` model: a catalog, its entries and their states, and
//! the metadata file that keeps them in the catalog's directory.

use optical::volume::layout::{escape, unescape};

use crate::error::{Error, Result};

/// The metadata file's name in the catalog directory.
pub const META_FILE: &str = "QS400CLG.INF";
/// The metadata file's first line.
pub const META_MAGIC: &str = "QS400-IMGCLG 1";
/// The catalog keys, in the order written.
pub const KEYS: [&str; 7] = ["name", "type", "dir", "text", "status", "device", "created"];
/// The fields of an `entry` line after the key, in order.
pub const ENTRY_FIELDS: [&str; 6] = ["index", "file", "volume", "status", "wrtptc", "text"];
/// The most entries a catalog holds.
pub const MAX_ENTRIES: u16 = 256;

/// `TYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImgType {
    /// `*OPT`: virtual optical images.
    Opt,
    /// `*TAP`: virtual tape volumes.
    Tap,
}

impl ImgType {
    /// The special value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Opt => "*OPT",
            Self::Tap => "*TAP",
        }
    }

    /// From the special value.
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "*OPT" => Ok(Self::Opt),
            "*TAP" => Ok(Self::Tap),
            _ => Err(Error::Invalid(format!("{s} is not *OPT or *TAP"))),
        }
    }
}

/// A catalog's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogStatus {
    /// `*READY`: loaded on a virtual device.
    Ready,
    /// `*NOTREADY`: not loaded.
    NotReady,
}

impl CatalogStatus {
    /// The special value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "*READY",
            Self::NotReady => "*NOTREADY",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "*READY" => Ok(Self::Ready),
            "*NOTREADY" => Ok(Self::NotReady),
            _ => Err(Error::Damaged(format!("catalog status {s}"))),
        }
    }
}

/// An entry's status, in IBM's sense.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryStatus {
    /// `*MOUNTED`: loaded and the volume the device presents; one per
    /// catalog at most.
    Mounted,
    /// `*LOADED`: in the device, ready to be mounted.
    Loaded,
    /// `*AVAILABLE`: in the catalog only; it must be loaded to be used.
    Available,
}

impl EntryStatus {
    /// The special value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mounted => "*MOUNTED",
            Self::Loaded => "*LOADED",
            Self::Available => "*AVAILABLE",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "*MOUNTED" => Ok(Self::Mounted),
            "*LOADED" => Ok(Self::Loaded),
            "*AVAILABLE" => Ok(Self::Available),
            _ => Err(Error::Damaged(format!("entry status {s}"))),
        }
    }
}

/// One entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `IMGCLGIDX`, 1 to 256.
    pub index: u16,
    /// The image file's name in the catalog directory.
    pub file: String,
    /// The volume identifier.
    pub volume: String,
    /// Status.
    pub status: EntryStatus,
    /// `WRTPTC(*YES)`.
    pub write_protect: bool,
    /// Text.
    pub text: String,
}

/// A catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    /// Name.
    pub name: String,
    /// Type.
    pub kind: ImgType,
    /// Its directory.
    pub dir: String,
    /// Text.
    pub text: String,
    /// Status.
    pub status: CatalogStatus,
    /// The device it is loaded on.
    pub device: Option<String>,
    /// Unix time it was made.
    pub created: u64,
    /// Entries by index.
    pub entries: Vec<Entry>,
}

fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

impl Catalog {
    /// The metadata file's text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let vals = [
            self.name.clone(),
            self.kind.as_str().to_string(),
            self.dir.clone(),
            self.text.clone(),
            self.status.as_str().to_string(),
            self.device.clone().unwrap_or_default(),
            self.created.to_string(),
        ];
        let mut s = format!("{META_MAGIC}\n");
        for (k, v) in KEYS.iter().zip(vals) {
            s.push_str(k);
            s.push('\t');
            s.push_str(&escape(&v));
            s.push('\n');
        }
        for e in &self.entries {
            let fields = [
                e.index.to_string(),
                e.file.clone(),
                e.volume.clone(),
                e.status.as_str().to_string(),
                yes(e.write_protect).to_string(),
                e.text.clone(),
            ];
            s.push_str("entry");
            for f in fields {
                s.push('\t');
                s.push_str(&escape(&f));
            }
            s.push('\n');
        }
        s
    }

    /// Parse the metadata file.
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        if lines.next() != Some(META_MAGIC) {
            return Err(Error::Damaged("not a QS/400 image catalog file".into()));
        }
        let mut c = Self {
            name: String::new(),
            kind: ImgType::Opt,
            dir: String::new(),
            text: String::new(),
            status: CatalogStatus::NotReady,
            device: None,
            created: 0,
            entries: Vec::new(),
        };
        for line in lines {
            let f: Vec<String> = line.split('\t').map(unescape).collect();
            match (f[0].as_str(), f.len()) {
                ("entry", 7) => {
                    let index = f[1]
                        .parse()
                        .map_err(|_| Error::Damaged(format!("entry index {}", f[1])))?;
                    c.entries.push(Entry {
                        index,
                        file: f[2].clone(),
                        volume: f[3].clone(),
                        status: EntryStatus::parse(&f[4])?,
                        write_protect: f[5] == "*YES",
                        text: f[6].clone(),
                    });
                }
                ("entry", _) => return Err(Error::Damaged(format!("entry line: {line}"))),
                ("name", 2) => c.name.clone_from(&f[1]),
                ("type", 2) => {
                    c.kind = ImgType::parse(&f[1]).map_err(|_| Error::Damaged(f[1].clone()))?;
                }
                ("dir", 2) => c.dir.clone_from(&f[1]),
                ("text", 2) => c.text.clone_from(&f[1]),
                ("status", 2) => c.status = CatalogStatus::parse(&f[1])?,
                ("device", 2) => c.device = Some(f[1].clone()).filter(|d| !d.is_empty()),
                ("created", 2) => {
                    c.created = f[1].parse().map_err(|_| Error::Damaged("created".into()))?;
                }
                _ => {}
            }
        }
        if c.name.is_empty() || c.dir.is_empty() {
            return Err(Error::Damaged("catalog without a name or directory".into()));
        }
        c.entries.sort_by_key(|e| e.index);
        Ok(c)
    }

    /// The entry at `index`.
    #[must_use]
    pub fn entry(&self, index: u16) -> Option<&Entry> {
        self.entries.iter().find(|e| e.index == index)
    }

    /// The mounted entry.
    #[must_use]
    pub fn mounted(&self) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.status == EntryStatus::Mounted)
    }

    /// The image file's path.
    #[must_use]
    pub fn path_of(&self, e: &Entry) -> String {
        crate::fs::join(&self.dir, &e.file)
    }

    /// With nothing mounted, mount the lowest loaded entry (a loaded
    /// catalog's device always presents a volume when it has one).
    pub(crate) fn settle(&mut self) {
        if self.mounted().is_some() {
            return;
        }
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.status == EntryStatus::Loaded)
        {
            e.status = EntryStatus::Mounted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let c = Catalog {
            name: "MYCLG".into(),
            kind: ImgType::Tap,
            dir: "/imgs/my clg".into(),
            text: "tabs\tand\\slashes".into(),
            status: CatalogStatus::Ready,
            device: Some("TAPVRT01".into()),
            created: 42,
            entries: vec![Entry {
                index: 2,
                file: "VOL002".into(),
                volume: "VOL002".into(),
                status: EntryStatus::Mounted,
                write_protect: true,
                text: String::new(),
            }],
        };
        assert_eq!(Catalog::parse(&c.to_text()).unwrap(), c);
        assert!(Catalog::parse("junk").is_err());
        assert!(Catalog::parse(&format!("{META_MAGIC}\nname\tX\ndir\t/d\nentry\t1\tf\n")).is_err());
    }
}
